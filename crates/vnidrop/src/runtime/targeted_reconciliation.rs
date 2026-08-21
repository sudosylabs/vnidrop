//! Durable targeted-transfer background reconciliation.

use std::sync::Arc;

use anyhow::Result;

use super::{targeted::map_connect_failure, CoreInner};
use crate::{
    error::VnidropError,
    targeted_transfer::{
        protocol::{DeliverTargetedAuthorization, TargetedTransferProtocol},
        TargetedAuthorization, TargetedTransferRow,
    },
    util::now_ms,
};

impl CoreInner {
    pub(super) async fn spawn_targeted_reconciliation_task(self: &Arc<Self>) {
        let core = self.clone();
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                core.release_completed_targeted_payloads().await;
                core.retry_targeted_authorization_deliveries().await;
                core.retry_targeted_completions().await;
            }
        });
        *self.targeted_reconciliation_task.lock().await = Some(task);
    }

    async fn retry_targeted_completions(&self) {
        let rows = match self.targeted_transfers.list_pending_completions().await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "failed to load pending targeted completions");
                return;
            }
        };
        for row in rows.into_iter().take(1) {
            let encoded = match self.load_stored_authorization(&row).await {
                Ok(Some(encoded)) => encoded,
                Ok(None) => {
                    tracing::warn!(transfer_id = %row.id, "pending targeted completion has no authorization");
                    self.defer_targeted_completion_with_log(&row.id, now_ms() + 30_000)
                        .await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(transfer_id = %row.id, %error, "failed to load pending targeted completion authorization");
                    self.defer_targeted_completion_with_log(&row.id, now_ms() + 30_000)
                        .await;
                    continue;
                }
            };
            let auth = match TargetedAuthorization::decode(&encoded) {
                Ok(auth) => auth,
                Err(error) => {
                    tracing::warn!(transfer_id = %row.id, %error, "failed to decode pending targeted completion authorization");
                    self.defer_targeted_completion_with_log(&row.id, now_ms() + 30_000)
                        .await;
                    continue;
                }
            };
            match self.acknowledge_targeted_completion(&auth).await {
                Ok(()) => {
                    if let Err(error) = self
                        .targeted_transfers
                        .clear_pending_completion(&row.id)
                        .await
                    {
                        tracing::warn!(transfer_id = %row.id, %error, "failed to settle targeted completion");
                    }
                }
                Err(error) => {
                    tracing::warn!(transfer_id = %row.id, %error, "targeted completion retry failed");
                    self.defer_targeted_completion_with_log(&row.id, now_ms() + 5_000)
                        .await;
                }
            }
        }
    }

    async fn defer_targeted_completion_with_log(&self, id: &str, next_attempt_at: i64) {
        if let Err(error) = self
            .targeted_transfers
            .defer_pending_completion(id, next_attempt_at)
            .await
        {
            tracing::warn!(transfer_id = %id, %error, "failed to defer targeted completion");
        }
    }

    async fn retry_targeted_authorization_deliveries(&self) {
        let rows = match self
            .targeted_transfers
            .list_pending_authorization_deliveries()
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "failed to load pending targeted authorization deliveries");
                return;
            }
        };
        for row in rows.into_iter().take(1) {
            match self.deliver_stored_targeted_authorization(&row).await {
                Ok(true) => {
                    if let Err(error) = self
                        .targeted_transfers
                        .clear_pending_authorization_delivery(&row.id)
                        .await
                    {
                        tracing::warn!(transfer_id = %row.id, %error, "failed to settle targeted authorization delivery");
                    }
                }
                Ok(false) => {
                    tracing::warn!(transfer_id = %row.id, "receiver rejected targeted authorization delivery");
                    if let Err(error) = self
                        .targeted_transfers
                        .defer_pending_authorization_delivery(&row.id, now_ms() + 5_000)
                        .await
                    {
                        tracing::warn!(transfer_id = %row.id, %error, "failed to defer rejected targeted authorization delivery");
                    }
                }
                Err(error) => {
                    tracing::warn!(transfer_id = %row.id, %error, "targeted authorization delivery retry failed");
                    if let Err(error) = self
                        .targeted_transfers
                        .defer_pending_authorization_delivery(&row.id, now_ms() + 5_000)
                        .await
                    {
                        tracing::warn!(transfer_id = %row.id, %error, "failed to defer targeted authorization delivery");
                    }
                }
            }
        }
    }

    pub(super) async fn deliver_stored_targeted_authorization(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<bool, VnidropError> {
        #[cfg(test)]
        self.targeted_authorization_delivery_attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        #[cfg(test)]
        if self
            .suppress_targeted_authorization_delivery
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(VnidropError::device_unavailable(anyhow::anyhow!(
                "targeted authorization delivery suppressed by test"
            )));
        }
        let encoded = self.load_stored_authorization(row).await?.ok_or_else(|| {
            VnidropError::SecureStorageMissing {
                reason: "targeted authorization is missing".to_string(),
            }
        })?;
        let addr = self
            .device_relationships
            .peer_addr(&row.receiver_endpoint_id)
            .await?;
        let response = tokio::time::timeout(
            self.connection_timeout(),
            TargetedTransferProtocol::client(self.endpoint.clone(), addr).deliver_authorization(
                DeliverTargetedAuthorization {
                    transfer_id: row.id.clone(),
                    authorization: encoded,
                },
            ),
        )
        .await
        .map_err(|_| {
            VnidropError::device_unavailable(anyhow::anyhow!(
                "targeted authorization delivery timed out"
            ))
        })?
        .map_err(map_connect_failure)?;
        Ok(response == crate::targeted_transfer::protocol::DeliverAuthorizationResponse::Stored)
    }

    async fn release_completed_targeted_payloads(&self) {
        let rows = match self.targeted_transfers.list_completed_sender_rows().await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "failed to load pending targeted payload releases");
                return;
            }
        };
        for row in rows {
            match self
                .try_teardown_targeted_payload(row.protocol_transfer_id, Some(&row.id))
                .await
            {
                Ok(()) => {
                    if let Err(error) = self
                        .targeted_transfers
                        .clear_pending_payload_release(&row.id)
                        .await
                    {
                        tracing::warn!(transfer_id = %row.id, %error, "failed to settle targeted payload release");
                    }
                }
                Err(error) => {
                    tracing::warn!(transfer_id = %row.id, %error, "targeted payload release retry failed")
                }
            }
        }
    }
}
