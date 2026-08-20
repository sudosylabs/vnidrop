//! Create, offer, approve, resume, cancel, and delete targeted transfers.

use std::sync::Arc;

use anyhow::Result;
use iroh_blobs::ticket::BlobTicket;

use super::{targeted_tag_name, CoreInner};
use crate::{
    api::{
        PendingTargetedOffer, TargetedOfferResponse, TargetedTransfer, TargetedTransferState,
        TransferAccessMode,
    },
    error::VnidropError,
    secure_secret::SecretHandle,
    targeted_transfer::{
        protocol::{CancelTargetedOffer, TargetedTransferProtocol},
        TargetedTransferRole,
    },
};

impl CoreInner {
    pub(crate) fn emit_targeted_lifecycle(&self, transfer_id: &str, kind: &str) {
        self.emit_endpoint(
            "targeted_transfer",
            kind,
            serde_json::json!({ "targeted_transfer_id": transfer_id }),
        );
    }

    pub(super) fn connection_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.limits.connection_timeout_ms)
    }

    pub(super) fn offer_wait_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.limits.offer_timeout_ms)
    }

    pub(super) async fn list_pending_targeted_offers(&self) -> Vec<PendingTargetedOffer> {
        self.targeted_offers.list().await
    }

    pub(super) async fn get_targeted_transfer(
        &self,
        id: String,
    ) -> Result<Option<TargetedTransfer>, VnidropError> {
        self.targeted_transfers.get(&id).await
    }

    pub(super) async fn list_targeted_transfers(
        &self,
    ) -> Result<Vec<TargetedTransfer>, VnidropError> {
        self.targeted_transfers.list().await
    }

    pub(crate) async fn restore_targeted_transfer_access(&self) -> Result<(), VnidropError> {
        let mut active_tags = std::collections::HashSet::new();
        for (id, sender) in self
            .targeted_transfers
            .list_accepted_intent_senders()
            .await?
        {
            if self
                .device_relationships
                .require_saved(&sender)
                .await
                .is_err()
            {
                self.targeted_transfers
                    .clear_accepted_intent_if_sender(&id, &sender)
                    .await?;
            }
        }
        let mut revoked_peers = std::collections::HashSet::new();
        for row in self.targeted_transfers.list_resumable_rows().await? {
            let peer = match row.role {
                TargetedTransferRole::Sender => &row.receiver_endpoint_id,
                TargetedTransferRole::Receiver => &row.sender_endpoint_id,
            };
            let blocked = self
                .blocked_devices
                .is_blocked(peer)
                .await
                .map_err(VnidropError::repository)?;
            if (blocked || self.device_relationships.require_saved(peer).await.is_err())
                && revoked_peers.insert(peer.clone())
            {
                self.cancel_targeted_transfers_for_peer(peer).await?;
            }
        }
        for row in self.targeted_transfers.authorization_rows().await? {
            let blocked = self
                .blocked_devices
                .is_blocked(match row.role {
                    TargetedTransferRole::Sender => &row.receiver_endpoint_id,
                    TargetedTransferRole::Receiver => &row.sender_endpoint_id,
                })
                .await
                .map_err(VnidropError::repository)?;
            let terminal = matches!(
                row.state,
                TargetedTransferState::Cancelled
                    | TargetedTransferState::Deleted
                    | TargetedTransferState::Failed
            );
            let revoked_receiver = row.role == TargetedTransferRole::Receiver
                && (blocked
                    || self
                        .device_relationships
                        .require_saved(&row.sender_endpoint_id)
                        .await
                        .is_err());
            if terminal || revoked_receiver {
                if let Some(handle) = row.authorization_secret_handle {
                    if let Some(custody) = &self.secret_custody {
                        custody.remove(&SecretHandle::from_stored(handle)).await?;
                    }
                    self.targeted_transfers.clear_authorization(&row.id).await?;
                }
            }
        }
        for row in self.targeted_transfers.list_resumable_sender_rows().await? {
            let blocked = self
                .blocked_devices
                .is_blocked(&row.receiver_endpoint_id)
                .await
                .map_err(VnidropError::repository)?;
            let saved = self
                .device_relationships
                .require_saved(&row.receiver_endpoint_id)
                .await
                .is_ok();
            if blocked || !saved {
                self.targeted_transfers
                    .transition_terminal(&row.id, TargetedTransferState::Cancelled, false)
                    .await?;
                self.teardown_targeted_payload(row.protocol_transfer_id, Some(&row.id))
                    .await;
                continue;
            }
            let tag_name = targeted_tag_name(&row.id);
            let restored = async {
                if row.content_hash.len() != iroh_blobs::Hash::new([0; 32]).to_string().len() {
                    return Err(VnidropError::transfer(anyhow::anyhow!(
                        "invalid targeted content hash"
                    )));
                }
                let root_hash = row
                    .content_hash
                    .parse::<iroh_blobs::Hash>()
                    .map_err(|error| VnidropError::transfer(anyhow::anyhow!(error)))?;
                let collection = iroh_blobs::format::collection::Collection::load(
                    root_hash,
                    self.store.as_ref(),
                )
                .await
                .map_err(VnidropError::transfer)?;
                self.store
                    .tags()
                    .set(&tag_name, (root_hash, iroh_blobs::BlobFormat::HashSeq))
                    .await
                    .map_err(VnidropError::transfer)?;
                Ok::<_, VnidropError>((root_hash, collection))
            }
            .await;
            let (root_hash, collection) = match restored {
                Ok(restored) => restored,
                Err(error) => {
                    tracing::warn!(transfer_id = %row.id, %error, "failed to restore targeted payload; marking transfer failed");
                    match self.targeted_transfers.fail_resumable(&row.id).await {
                        Ok(_) => {}
                        Err(fail_error) => {
                            tracing::warn!(transfer_id = %row.id, %fail_error, "failed to mark unrestorable targeted payload failed")
                        }
                    }
                    self.teardown_targeted_payload(row.protocol_transfer_id, Some(&row.id))
                        .await;
                    continue;
                }
            };
            self.register_share_hashes(
                row.protocol_transfer_id,
                std::iter::once(root_hash).chain(collection.iter().map(|(_, hash)| *hash)),
            )
            .await;
            self.access_policy
                .set_mode(
                    row.protocol_transfer_id,
                    TransferAccessMode::ApprovalRequired,
                )
                .await;
            self.access_policy
                .approve_endpoint_until(row.protocol_transfer_id, row.receiver_endpoint_id, None)
                .await;
            active_tags.insert(tag_name);
        }
        use futures_lite::StreamExt as _;
        let mut tags = self
            .store
            .tags()
            .list_prefix("vnidrop/targeted/")
            .await
            .map_err(VnidropError::transfer)?;
        while let Some(tag) = tags.next().await {
            let tag = tag.map_err(VnidropError::transfer)?;
            let name = String::from_utf8_lossy(tag.name.as_ref()).to_string();
            if !active_tags.contains(&name) {
                self.store
                    .tags()
                    .delete(name)
                    .await
                    .map_err(VnidropError::transfer)?;
            }
        }
        Ok(())
    }

    /// Cancel in-flight targeted transfers involving `peer` (for forget/block).
    pub(crate) async fn cancel_targeted_transfers_for_peer(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<u64, VnidropError> {
        #[cfg(test)]
        {
            self.targeted_cancel_log
                .lock()
                .expect("targeted cancel log")
                .push(peer_endpoint_id.to_string());
        }
        self.targeted_transfers
            .terminate_peer(peer_endpoint_id)
            .await
    }

    pub(super) fn signal_targeted_transfer_cancel_by_id(&self, id: &str) -> bool {
        self.active_targeted_transfers
            .lock()
            .expect("active_targeted_transfers")
            .remove(id)
            .is_some_and(|active| active.cancel.send(()).is_ok())
    }

    pub(super) async fn cancel_targeted_transfer(&self, id: String) -> Result<(), VnidropError> {
        let Some((row, _)) = self
            .targeted_transfers
            .terminate_local(&id, TargetedTransferState::Cancelled)
            .await?
        else {
            return Ok(());
        };
        // Best-effort idempotent peer teardown for both pre- and post-approval work.
        let peer_id = match row.role {
            TargetedTransferRole::Sender => &row.receiver_endpoint_id,
            TargetedTransferRole::Receiver => &row.sender_endpoint_id,
        };
        if row.state != TargetedTransferState::Deleted {
            if let Ok(addr) = self.device_relationships.peer_addr(peer_id).await {
                let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
                let _ = tokio::time::timeout(
                    self.connection_timeout(),
                    client.cancel_offer(CancelTargetedOffer {
                        transfer_id: id.clone(),
                        terminal: Some(TargetedTransferState::Cancelled),
                    }),
                )
                .await;
            }
        }
        Ok(())
    }

    pub(super) async fn stop_targeted_preparation(
        &self,
        id: &str,
    ) -> Result<crate::api::TargetedPreparationStopOutcome, VnidropError> {
        let row = self.targeted_transfers.get_row(id).await?;
        let outcome = self.targeted_transfers.stop_preparation(id).await?;
        if matches!(
            outcome,
            crate::api::TargetedPreparationStopOutcome::TransferAbandoned
                | crate::api::TargetedPreparationStopOutcome::TransferCancelled
        ) {
            if let Some(row) = row {
                if let Ok(addr) = self
                    .device_relationships
                    .peer_addr(&row.receiver_endpoint_id)
                    .await
                {
                    let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
                    let _ = tokio::time::timeout(
                        self.connection_timeout(),
                        client.cancel_offer(CancelTargetedOffer {
                            transfer_id: id.to_string(),
                            terminal: if outcome
                                == crate::api::TargetedPreparationStopOutcome::TransferCancelled
                            {
                                Some(TargetedTransferState::Cancelled)
                            } else {
                                None
                            },
                        }),
                    )
                    .await;
                }
            }
        }
        Ok(outcome)
    }

    pub(super) async fn delete_targeted_transfer(
        self: &Arc<Self>,
        id: String,
    ) -> Result<(), VnidropError> {
        let Some((row, changed)) = self
            .targeted_transfers
            .terminate_local(&id, TargetedTransferState::Deleted)
            .await?
        else {
            return Ok(());
        };
        let peer_id = match row.role {
            TargetedTransferRole::Sender => &row.receiver_endpoint_id,
            TargetedTransferRole::Receiver => &row.sender_endpoint_id,
        };
        if changed {
            if let Ok(addr) = self.device_relationships.peer_addr(peer_id).await {
                let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
                let _ = tokio::time::timeout(
                    self.connection_timeout(),
                    client.cancel_offer(CancelTargetedOffer {
                        transfer_id: id.clone(),
                        terminal: Some(TargetedTransferState::Deleted),
                    }),
                )
                .await;
            }
        }
        Ok(())
    }

    pub(super) async fn respond_to_targeted_offer(
        self: &Arc<Self>,
        transfer_id: String,
        accepted: bool,
    ) -> Result<TargetedOfferResponse, VnidropError> {
        if self.targeted_offers.is_settled(&transfer_id).await {
            return Ok(TargetedOfferResponse::AlreadySettled { transfer_id });
        }
        if let Ok(Some(row)) = self.targeted_transfers.get_row(&transfer_id).await {
            if self.load_stored_authorization(&row).await?.is_some()
                || matches!(
                    row.state,
                    TargetedTransferState::Approved
                        | TargetedTransferState::Connecting
                        | TargetedTransferState::Transferring
                        | TargetedTransferState::Interrupted
                        | TargetedTransferState::Completed
                        | TargetedTransferState::Declined
                        | TargetedTransferState::Cancelled
                        | TargetedTransferState::Failed
                        | TargetedTransferState::Deleted
                )
            {
                return Ok(TargetedOfferResponse::AlreadySettled { transfer_id });
            }
        }

        if accepted {
            let offer = self
                .targeted_offers
                .pending_for_acceptance(&transfer_id)
                .await
                .ok_or_else(|| {
                    VnidropError::invalid_input(anyhow::anyhow!("unknown targeted offer"))
                })?;
            self.targeted_transfers
                .persist_accepted_offer_intent(&offer)
                .await?;
        }

        match self.targeted_offers.respond(&transfer_id, accepted).await {
            Ok(Some(_auth)) => match self.targeted_transfers.get_row(&transfer_id).await? {
                Some(row)
                    if row.role == TargetedTransferRole::Receiver
                        && row.state == TargetedTransferState::Approved
                        && row.authorization_secret_handle.is_some() =>
                {
                    Ok(TargetedOfferResponse::Approved { transfer_id })
                }
                _ => Err(VnidropError::internal(anyhow::anyhow!(
                    "receiver acknowledged authorization without durable custody"
                ))),
            },
            Ok(None) => {
                self.targeted_transfers
                    .clear_accepted_offer_intent(&transfer_id)
                    .await?;
                Ok(TargetedOfferResponse::Declined)
            }
            Err(crate::targeted_transfer::RespondError::Unknown) => Err(
                VnidropError::invalid_input(anyhow::anyhow!("unknown targeted offer")),
            ),
            Err(crate::targeted_transfer::RespondError::SenderGone) => {
                Err(VnidropError::device_unavailable(anyhow::anyhow!(
                    "sender disconnected before approval completed"
                )))
            }
            Err(crate::targeted_transfer::RespondError::AuthorizationTimeout) => {
                Err(VnidropError::offer_timeout(anyhow::anyhow!(
                    "authorization was not delivered in time"
                )))
            }
        }
    }

    pub(super) async fn teardown_targeted_payload(
        &self,
        protocol_transfer_id: u64,
        id: Option<&str>,
    ) {
        if let Err(error) = self
            .try_teardown_targeted_payload(protocol_transfer_id, id)
            .await
        {
            tracing::warn!(%error, "failed to release targeted payload");
        }
    }

    pub(super) async fn try_teardown_targeted_payload(
        &self,
        protocol_transfer_id: u64,
        id: Option<&str>,
    ) -> Result<(), VnidropError> {
        self.unregister_transfer_hashes(protocol_transfer_id).await;
        self.access_policy
            .remove_transfer(protocol_transfer_id)
            .await;
        if let Some(id) = id {
            self.store
                .tags()
                .delete(targeted_tag_name(id))
                .await
                .map_err(VnidropError::transfer)?;
        }
        Ok(())
    }
}

pub(super) fn allocate_protocol_transfer_id(transfer_uuid: &str) -> u64 {
    let hash = blake3::hash(transfer_uuid.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    // SQLite transfer ids are signed; keep within i64::MAX.
    let value = u64::from_le_bytes(bytes) & (i64::MAX as u64);
    if value == 0 {
        1
    } else {
        value
    }
}

pub(super) fn map_connect_failure(error: irpc::Error) -> VnidropError {
    let rendered = error.to_string();
    // ALPN / protocol negotiation failures are distinguishable from offline peers.
    if rendered.contains("ALPN")
        || rendered.contains("alpn")
        || rendered.contains("protocol")
        || rendered.contains("unsupported")
    {
        return VnidropError::protocol_incompatible(anyhow::anyhow!(
            "peer does not support saved-device targeted transfers"
        ));
    }
    VnidropError::device_unavailable(anyhow::anyhow!("device is not reachable: {rendered}"))
}

pub(super) trait BlobTicketParse {
    fn from_str_compat(value: &str) -> Result<BlobTicket, String>;
}

impl BlobTicketParse for BlobTicket {
    fn from_str_compat(value: &str) -> Result<BlobTicket, String> {
        use std::str::FromStr;
        BlobTicket::from_str(value).map_err(|error| error.to_string())
    }
}
