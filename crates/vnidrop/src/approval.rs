use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::{
    access_policy::{AccessPolicy, APPROVAL_SESSION_TTL_MS},
    event_hub::EventHub,
    handshake::{
        DeliveryFailureReceipt, DeliveryReceipt, DeliveryReceiptResponse, HandshakeResponse,
        RequestTransfer,
    },
    repository::{ReceiverRequestInsert, Repository},
    transfer_state::ReceiverRequestStatus,
    util::now_ms,
};

const APPROVAL_TTL_MS: i64 = APPROVAL_SESSION_TTL_MS;
const APPROVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalDecision {
    pub(crate) request_id: String,
    pub(crate) accepted: bool,
    pub(crate) reason: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ApprovalService {
    repository: Repository,
    event_hub: Arc<EventHub>,
    access_policy: Arc<AccessPolicy>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>>,
    max_pending: usize,
    max_metadata_bytes: u64,
}

impl ApprovalService {
    pub(crate) async fn complete_delivery(
        &self,
        remote_endpoint_id: String,
        receipt: DeliveryReceipt,
    ) -> DeliveryReceiptResponse {
        let token_hash = receipt_token_hash(&receipt.token);
        match self
            .repository
            .complete_receiver_delivery(
                &receipt.request_id,
                receipt.transfer_id,
                &remote_endpoint_id,
                &token_hash,
            )
            .await
        {
            Ok(()) => {
                self.event_hub.emit_transfer(
					receipt.transfer_id,
					"send",
					"delivery",
					"receiver-completed",
					json!({ "request_id": receipt.request_id, "remote_endpoint_id": remote_endpoint_id }),
				);
                DeliveryReceiptResponse::Recorded
            }
            Err(error) => {
                tracing::warn!(%error, "rejected receiver delivery receipt");
                DeliveryReceiptResponse::Rejected {
                    reason: "invalid-receipt".to_string(),
                }
            }
        }
    }

    pub(crate) async fn fail_delivery(
        &self,
        remote_endpoint_id: String,
        receipt: DeliveryFailureReceipt,
    ) -> DeliveryReceiptResponse {
        let token_hash = receipt_token_hash(&receipt.token);
        match self
            .repository
            .fail_receiver_delivery(
                &receipt.request_id,
                receipt.transfer_id,
                &remote_endpoint_id,
                &token_hash,
                &receipt.reason,
            )
            .await
        {
            Ok(()) => {
                self.event_hub.emit_transfer(
                    receipt.transfer_id,
                    "send",
                    "delivery",
                    "receiver-failed",
                    json!({
                        "request_id": receipt.request_id,
                        "remote_endpoint_id": remote_endpoint_id,
                        "reason": receipt.reason,
                    }),
                );
                DeliveryReceiptResponse::Recorded
            }
            Err(error) => {
                tracing::warn!(%error, "rejected receiver delivery failure");
                DeliveryReceiptResponse::Rejected {
                    reason: "invalid-receipt".to_string(),
                }
            }
        }
    }

    pub(crate) fn new(
        repository: Repository,
        event_hub: Arc<EventHub>,
        access_policy: Arc<AccessPolicy>,
        max_pending: usize,
        max_metadata_bytes: u64,
    ) -> Self {
        Self {
            repository,
            event_hub,
            access_policy,
            pending: Arc::new(Mutex::new(HashMap::new())),
            max_pending,
            max_metadata_bytes,
        }
    }

    pub(crate) async fn respond(
        &self,
        request_id: String,
        accepted: bool,
        reason: Option<String>,
    ) -> anyhow::Result<()> {
        let sender = self.pending.lock().await.remove(&request_id);
        let status = if accepted {
            ReceiverRequestStatus::Accepted
        } else {
            ReceiverRequestStatus::Refused
        };
        self.repository
            .update_receiver_request_status(&request_id, status, reason.as_deref())
            .await?;

        if let Some(sender) = sender {
            let _ = sender.send(ApprovalDecision {
                request_id,
                accepted,
                reason,
            });
        }

        Ok(())
    }

    pub(crate) async fn request_transfer(
        &self,
        remote_endpoint_id: String,
        request: RequestTransfer,
    ) -> HandshakeResponse {
        let metadata_values = [
            request.transfer_hash.as_str(),
            request.transfer_name.as_str(),
            request.receiver_name.as_deref().unwrap_or_default(),
            request.receiver_device_name.as_deref().unwrap_or_default(),
            request.app_version.as_str(),
        ];
        if metadata_values
            .iter()
            .any(|value| value.len() as u64 > self.max_metadata_bytes)
        {
            return self
                .deny(
                    request.transfer_id,
                    remote_endpoint_id,
                    "metadata-too-large",
                )
                .await;
        }
        self.event_hub.emit_transfer(
            request.transfer_id,
            "send",
            "handshake",
            "transfer-requested",
            json!({
                "remote_endpoint_id": remote_endpoint_id,
                "request": request,
            }),
        );

        match self
            .repository
            .send_exists(request.transfer_id, &request.transfer_hash)
            .await
        {
            Ok(true) => {
                if self
                    .access_policy
                    .allows_without_approval(request.transfer_id)
                    .await
                {
                    self.allow_without_sender_decision(remote_endpoint_id, request)
                        .await
                } else {
                    self.wait_for_sender_decision(remote_endpoint_id, request)
                        .await
                }
            }
            Ok(false) => {
                self.deny(request.transfer_id, remote_endpoint_id, "unknown-transfer")
                    .await
            }
            Err(error) => {
                tracing::error!(%error, "failed to validate handshake transfer request");
                self.deny(request.transfer_id, remote_endpoint_id, "repository-error")
                    .await
            }
        }
    }

    async fn allow_without_sender_decision(
        &self,
        remote_endpoint_id: String,
        request: RequestTransfer,
    ) -> HandshakeResponse {
        let request_id = Uuid::new_v4().to_string();
        if self
            .repository
            .insert_receiver_request(ReceiverRequestInsert {
                id: &request_id,
                transfer_id: request.transfer_id,
                remote_endpoint_id: &remote_endpoint_id,
                transfer_name: &request.transfer_name,
                receiver_name: request.receiver_name.as_deref(),
                receiver_device_name: request.receiver_device_name.as_deref(),
                app_version: &request.app_version,
            })
            .await
            .is_err()
            || self
                .repository
                .update_receiver_request_status(&request_id, ReceiverRequestStatus::Accepted, None)
                .await
                .is_err()
        {
            return self
                .deny(request.transfer_id, remote_endpoint_id, "repository-error")
                .await;
        }
        let token = Uuid::new_v4().to_string();
        if self
            .repository
            .set_receiver_receipt_token(&request_id, &receipt_token_hash(&token))
            .await
            .is_err()
        {
            return self
                .deny(request.transfer_id, remote_endpoint_id, "repository-error")
                .await;
        }
        let expires_at = now_ms() + APPROVAL_TTL_MS;
        self.event_hub.emit_transfer(
            request.transfer_id,
            "send",
            "access",
            "receiver-auto-approved",
            json!({
                "remote_endpoint_id": remote_endpoint_id,
                "expires_at": expires_at,
            }),
        );
        HandshakeResponse::Approved {
            request_id,
            token,
            expires_at,
        }
    }

    async fn wait_for_sender_decision(
        &self,
        remote_endpoint_id: String,
        request: RequestTransfer,
    ) -> HandshakeResponse {
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        if pending.len() >= self.max_pending {
            drop(pending);
            return self
                .deny(
                    request.transfer_id,
                    remote_endpoint_id,
                    "too-many-pending-approvals",
                )
                .await;
        }
        pending.insert(request_id.clone(), tx);
        drop(pending);

        let insert_result = self
            .repository
            .insert_receiver_request(ReceiverRequestInsert {
                id: &request_id,
                transfer_id: request.transfer_id,
                remote_endpoint_id: &remote_endpoint_id,
                transfer_name: &request.transfer_name,
                receiver_name: request.receiver_name.as_deref(),
                receiver_device_name: request.receiver_device_name.as_deref(),
                app_version: &request.app_version,
            })
            .await;

        if let Err(error) = insert_result {
            self.pending.lock().await.remove(&request_id);
            tracing::error!(%error, "failed to persist receiver request");
            return self
                .deny(request.transfer_id, remote_endpoint_id, "repository-error")
                .await;
        }

        self.event_hub.emit_transfer(
            request.transfer_id,
            "send",
            "approval",
            "receiver-requested",
            json!({
                "request_id": request_id,
                "remote_endpoint_id": remote_endpoint_id,
                "receiver_name": request.receiver_name,
                "receiver_device_name": request.receiver_device_name,
                "transfer_name": request.transfer_name,
            }),
        );

        match tokio::time::timeout(APPROVAL_WAIT_TIMEOUT, rx).await {
            Ok(Ok(decision)) if decision.accepted => {
                let token = Uuid::new_v4().to_string();
                let expires_at = now_ms() + APPROVAL_TTL_MS;
                self.access_policy
                    .approve_endpoint_until(
                        request.transfer_id,
                        remote_endpoint_id.clone(),
                        Some(expires_at),
                    )
                    .await;
                self.event_hub.emit_transfer(
                    request.transfer_id,
                    "send",
                    "approval",
                    "receiver-accepted",
                    json!({
                        "request_id": decision.request_id,
                        "remote_endpoint_id": remote_endpoint_id,
                        "expires_at": expires_at,
                    }),
                );
                if let Err(error) = self
                    .repository
                    .set_receiver_receipt_token(&decision.request_id, &receipt_token_hash(&token))
                    .await
                {
                    tracing::error!(%error, "failed to attach delivery receipt token");
                    return HandshakeResponse::Denied {
                        reason: "repository-error".to_string(),
                    };
                }
                HandshakeResponse::Approved {
                    request_id: decision.request_id,
                    token,
                    expires_at,
                }
            }
            Ok(Ok(decision)) => {
                self.deny(
                    request.transfer_id,
                    remote_endpoint_id,
                    decision
                        .reason
                        .unwrap_or_else(|| "sender-refused".to_string()),
                )
                .await
            }
            Ok(Err(_)) | Err(_) => {
                self.pending.lock().await.remove(&request_id);
                let _ = self
                    .repository
                    .update_receiver_request_status(
                        &request_id,
                        ReceiverRequestStatus::Expired,
                        Some("approval timed out"),
                    )
                    .await;
                self.deny(request.transfer_id, remote_endpoint_id, "approval-timeout")
                    .await
            }
        }
    }

    async fn deny(
        &self,
        transfer_id: u64,
        remote_endpoint_id: String,
        reason: impl Into<String>,
    ) -> HandshakeResponse {
        let reason = reason.into();
        self.event_hub.emit_transfer(
            transfer_id,
            "send",
            "approval",
            "receiver-refused",
            json!({
                "remote_endpoint_id": remote_endpoint_id,
                "reason": reason,
            }),
        );
        HandshakeResponse::Denied { reason }
    }
}

fn receipt_token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}
