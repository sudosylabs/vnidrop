//! Create, offer, approve, resume, cancel, and delete targeted transfers.

use std::sync::Arc;

use anyhow::{Context, Result};
use iroh_blobs::{ticket::BlobTicket, BlobFormat};
use uuid::Uuid;

use super::{receive::ReceiveTarget, targeted_tag_name, CoreInner};
use crate::{
    api::{
        experimental_saved_device_capabilities, PendingTargetedOffer, ShareSource,
        TargetedOfferResponse, TargetedTransfer, TargetedTransferState, TransferAccessMode,
    },
    error::VnidropError,
    secure_secret::{SecretHandle, SecretKind},
    targeted_transfer::{
        auth_secret_material,
        protocol::{
            map_offer_refuse_reason, CancelTargetedOffer, CompleteTargetedTransfer,
            CompletionResponse, DeliverTargetedAuthorization, SubmitTargetedOffer,
            TargetedTransferProtocol, WireOfferResponse,
        },
        reconstruct_authorization, TargetedAuthorization, TargetedAuthorizationDraft,
        TargetedTransferRole, TargetedTransferRow,
    },
    util::{non_empty, now_ms},
};

impl CoreInner {
    pub(crate) fn emit_targeted_lifecycle(&self, transfer_id: &str, kind: &str) {
        self.emit_endpoint(
            "targeted_transfer",
            kind,
            serde_json::json!({ "targeted_transfer_id": transfer_id }),
        );
    }

    fn connection_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.limits.connection_timeout_ms)
    }

    fn offer_wait_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.limits.offer_timeout_ms)
    }

    pub(super) async fn spawn_targeted_completion_task(self: &Arc<Self>) {
        let core = self.clone();
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                core.release_completed_targeted_payloads().await;
                core.retry_targeted_completions().await;
            }
        });
        *self.targeted_completion_task.lock().await = Some(task);
    }

    async fn retry_targeted_completions(&self) {
        let Ok(rows) = self.targeted_store().list_pending_completions().await else {
            return;
        };
        for row in rows.into_iter().take(1) {
            let Ok(Some(encoded)) = self.load_stored_authorization(&row).await else {
                let _ = self
                    .targeted_store()
                    .defer_pending_completion(&row.id, now_ms() + 30_000)
                    .await;
                continue;
            };
            let Ok(auth) = TargetedAuthorization::decode(&encoded) else {
                let _ = self
                    .targeted_store()
                    .defer_pending_completion(&row.id, now_ms() + 30_000)
                    .await;
                continue;
            };
            if self.acknowledge_targeted_completion(&auth).await.is_ok() {
                let _ = self
                    .targeted_store()
                    .clear_pending_completion(&row.id)
                    .await;
            } else {
                let _ = self
                    .targeted_store()
                    .defer_pending_completion(&row.id, now_ms() + 5_000)
                    .await;
            }
        }
    }

    async fn release_completed_targeted_payloads(&self) {
        let Ok(rows) = self.targeted_store().list_completed_sender_rows().await else {
            return;
        };
        for row in rows {
            if self
                .try_teardown_targeted_payload(row.protocol_transfer_id, Some(&row.id))
                .await
                .is_ok()
            {
                let _ = self
                    .targeted_store()
                    .clear_pending_payload_release(&row.id)
                    .await;
            }
        }
    }

    pub(super) fn targeted_store(&self) -> crate::targeted_transfer::TargetedTransferStore {
        self.targeted_transfers.clone()
    }

    pub(super) async fn list_pending_targeted_offers(&self) -> Vec<PendingTargetedOffer> {
        self.targeted_offers.list().await
    }

    pub(super) async fn get_targeted_transfer(
        &self,
        id: String,
    ) -> Result<Option<TargetedTransfer>, VnidropError> {
        self.targeted_store().get(&id).await
    }

    pub(super) async fn list_targeted_transfers(
        &self,
    ) -> Result<Vec<TargetedTransfer>, VnidropError> {
        self.targeted_store().list().await
    }

    pub(crate) async fn restore_targeted_transfer_access(&self) -> Result<(), VnidropError> {
        let mut active_tags = std::collections::HashSet::new();
        for row in self.targeted_store().list_resumable_sender_rows().await? {
            let root_hash = row
                .content_hash
                .parse::<iroh_blobs::Hash>()
                .map_err(|error| VnidropError::transfer(anyhow::anyhow!(error)))?;
            let collection =
                iroh_blobs::format::collection::Collection::load(root_hash, self.store.as_ref())
                    .await
                    .map_err(VnidropError::transfer)?;
            let tag_name = targeted_tag_name(&row.id);
            self.store
                .tags()
                .set(&tag_name, (root_hash, iroh_blobs::BlobFormat::HashSeq))
                .await
                .map_err(VnidropError::transfer)?;
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
        self.targeted_offers.discard_from(peer_endpoint_id).await;
        let protocol_ids = self
            .targeted_store()
            .protocol_ids_for_peer(peer_endpoint_id)
            .await?;
        let sender_payloads = self
            .targeted_store()
            .sender_payloads_for_peer(peer_endpoint_id)
            .await?;
        let active_ids = self.targeted_store().ids_for_peer(peer_endpoint_id).await?;
        for id in active_ids {
            let _ = self.signal_targeted_transfer_cancel_by_id(&id);
        }
        let cancelled = self
            .targeted_store()
            .cancel_by_peer(peer_endpoint_id)
            .await?;
        for id in &cancelled {
            self.emit_targeted_lifecycle(id, "cancelled");
        }
        for protocol_transfer_id in &protocol_ids {
            self.teardown_targeted_payload(*protocol_transfer_id, None)
                .await;
        }
        for (id, protocol_transfer_id) in sender_payloads {
            self.teardown_targeted_payload(protocol_transfer_id, Some(&id))
                .await;
        }
        Ok(cancelled.len() as u64)
    }

    pub(super) fn signal_targeted_transfer_cancel_by_id(&self, id: &str) -> bool {
        self.active_targeted_transfers
            .lock()
            .expect("active_targeted_transfers")
            .remove(id)
            .is_some_and(|active| active.cancel.send(()).is_ok())
    }

    pub(super) async fn cancel_targeted_transfer(&self, id: String) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        let Some(row) = store.get_row(&id).await? else {
            // Still drop any live-session offer under this id.
            self.targeted_offers.discard(&id).await;
            return Ok(());
        };
        self.targeted_offers.discard(&id).await;
        let changed = store
            .transition_terminal(&id, TargetedTransferState::Cancelled, false)
            .await?;
        if changed {
            self.emit_targeted_lifecycle(&id, "cancelled");
        }
        self.access_policy
            .remove_transfer(row.protocol_transfer_id)
            .await;
        self.teardown_targeted_payload(row.protocol_transfer_id, Some(&row.id))
            .await;
        if row.role == TargetedTransferRole::Receiver {
            if let Some(handle) = &row.authorization_secret_handle {
                if let Some(custody) = &self.secret_custody {
                    custody
                        .remove(&SecretHandle::from_stored(handle.clone()))
                        .await?;
                }
                store.clear_authorization(&id).await?;
            }
        }
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

    pub(super) async fn delete_targeted_transfer(
        self: &Arc<Self>,
        id: String,
    ) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        let Some(row) = store.get_row(&id).await? else {
            self.targeted_offers.discard(&id).await;
            return Ok(());
        };
        let already_deleted = row.state == TargetedTransferState::Deleted;
        if already_deleted && row.authorization_secret_handle.is_none() && row.blob_ticket.is_none()
        {
            self.targeted_offers.discard(&id).await;
            return Ok(());
        }
        // Signal first, then commit denial before any fallible remote or blob cleanup.
        let peer_id = match row.role {
            TargetedTransferRole::Sender => &row.receiver_endpoint_id,
            TargetedTransferRole::Receiver => &row.sender_endpoint_id,
        };
        let changed = if already_deleted {
            false
        } else {
            store
                .transition_terminal(&id, TargetedTransferState::Deleted, false)
                .await?
        };
        if changed {
            self.emit_targeted_lifecycle(&id, "deleted");
        }
        self.targeted_offers.discard(&id).await;
        self.access_policy
            .remove_transfer(row.protocol_transfer_id)
            .await;
        self.teardown_targeted_payload(row.protocol_transfer_id, Some(&row.id))
            .await;
        if let Some(handle) = &row.authorization_secret_handle {
            if let Some(custody) = &self.secret_custody {
                custody
                    .remove(&SecretHandle::from_stored(handle.clone()))
                    .await?;
                store.clear_authorization(&id).await?;
            }
        } else {
            store.clear_authorization(&id).await?;
        }
        if !already_deleted {
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
        if let Ok(Some(row)) = self.targeted_store().get_row(&transfer_id).await {
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

        match self.targeted_offers.respond(&transfer_id, accepted).await {
            Ok(Some(auth)) => match self.persist_receiver_authorization(&auth).await {
                Ok(()) => {
                    self.emit_targeted_lifecycle(&transfer_id, "approved");
                    Ok(TargetedOfferResponse::Approved { transfer_id })
                }
                Err(error) => {
                    if self
                        .targeted_store()
                        .set_state_from_any(&transfer_id, TargetedTransferState::Failed)
                        .await
                        .is_ok()
                    {
                        self.emit_targeted_lifecycle(&transfer_id, "failed");
                    }
                    Err(error)
                }
            },
            Ok(None) => Ok(TargetedOfferResponse::Declined),
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

    pub(super) async fn create_targeted_transfer(
        self: &Arc<Self>,
        receiver_endpoint_id: String,
        sources: Vec<ShareSource>,
        transfer_name: Option<String>,
    ) -> Result<TargetedTransfer, VnidropError> {
        self.device_relationships
            .require_saved(&receiver_endpoint_id)
            .await?;

        let (transfer_uuid, protocol_transfer_id) = loop {
            let transfer_uuid = Uuid::new_v4().to_string();
            let protocol_transfer_id = allocate_protocol_transfer_id(&transfer_uuid);
            let invitation_collision = self
                .repository
                .list_transfers()
                .await
                .map_err(VnidropError::repository)?
                .into_iter()
                .any(|transfer| transfer.transfer_id == protocol_transfer_id);
            if !invitation_collision
                && !self
                    .targeted_store()
                    .contains_protocol_id(protocol_transfer_id)
                    .await?
            {
                break (transfer_uuid, protocol_transfer_id);
            }
        };
        let sender_endpoint_id = self.endpoint.id().to_string();
        let now = now_ms();

        if sources.is_empty() {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "at least one source is required"
            )));
        }
        if sources.len() as u64 > self.limits.max_sources {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "source count {} exceeds limit {}",
                sources.len(),
                self.limits.max_sources
            )));
        }
        self.limits
            .validate_metadata_text("transfer name", transfer_name.as_deref())
            .map_err(VnidropError::invalid_input)?;
        let import = self
            .import_sources(protocol_transfer_id, sources)
            .await
            .map_err(VnidropError::transfer)?;
        let payload_name = transfer_name
            .and_then(non_empty)
            .unwrap_or_else(|| import.default_name.clone());
        let blob_ticket =
            BlobTicket::new(self.endpoint.addr(), import.root_hash, BlobFormat::HashSeq);
        self.store
            .tags()
            .set(
                targeted_tag_name(&transfer_uuid),
                (import.root_hash, BlobFormat::HashSeq),
            )
            .await
            .map_err(VnidropError::transfer)?;
        self.register_share_hashes(
            protocol_transfer_id,
            std::iter::once(import.root_hash).chain(import.member_hashes.iter().copied()),
        )
        .await;
        self.access_policy
            .set_mode(protocol_transfer_id, TransferAccessMode::ApprovalRequired)
            .await;
        drop(import.tag);

        let store = self.targeted_store();
        let row = TargetedTransferRow {
            id: transfer_uuid.clone(),
            protocol_transfer_id,
            sender_endpoint_id: sender_endpoint_id.clone(),
            receiver_endpoint_id: receiver_endpoint_id.clone(),
            manifest_id: blob_ticket.hash().to_string(),
            content_hash: blob_ticket.hash().to_string(),
            transfer_name: payload_name.clone(),
            file_count: import.file_count,
            total_size: import.total_size,
            verified_bytes: 0,
            blob_ticket: None,
            authorization_secret_handle: None,
            role: TargetedTransferRole::Sender,
            state: TargetedTransferState::Preparing,
            created_at: now,
            updated_at: now,
        };
        if let Err(error) = store.insert(&row).await {
            self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                .await;
            return Err(error);
        }
        self.emit_targeted_lifecycle(&transfer_uuid, "created");
        if let Err(error) = store
            .set_state(
                &transfer_uuid,
                TargetedTransferState::Preparing,
                TargetedTransferState::Offering,
            )
            .await
        {
            self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                .await;
            return Err(error);
        }
        self.emit_targeted_lifecycle(&transfer_uuid, "offering");

        let addr = match self
            .device_relationships
            .peer_addr(&receiver_endpoint_id)
            .await
        {
            Ok(addr) => addr,
            Err(error) => {
                if store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::Offering,
                        TargetedTransferState::Failed,
                    )
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(error);
            }
        };
        let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
        let challenge =
            match tokio::time::timeout(self.connection_timeout(), client.request_challenge()).await
            {
                Ok(Ok(challenge)) => challenge,
                Ok(Err(error)) => {
                    if store
                        .set_state(
                            &transfer_uuid,
                            TargetedTransferState::Offering,
                            TargetedTransferState::Failed,
                        )
                        .await
                        .is_ok()
                    {
                        self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                    }
                    self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                        .await;
                    return Err(map_connect_failure(error));
                }
                Err(_) => {
                    if store
                        .set_state(
                            &transfer_uuid,
                            TargetedTransferState::Offering,
                            TargetedTransferState::Failed,
                        )
                        .await
                        .is_ok()
                    {
                        self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                    }
                    self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                        .await;
                    return Err(VnidropError::device_unavailable(anyhow::anyhow!(
                        "device did not answer in time"
                    )));
                }
            };

        let (proof, generation, relationship_protocol_version) = match self
            .device_relationships
            .prove_saved_possession(&receiver_endpoint_id, &challenge)
            .await
        {
            Ok(proof) => proof,
            Err(error) => {
                if store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::Offering,
                        TargetedTransferState::Failed,
                    )
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(error);
            }
        };

        let protocol_version =
            experimental_saved_device_capabilities().targeted_transfer_protocol_version;
        if let Err(error) = store
            .set_state(
                &transfer_uuid,
                TargetedTransferState::Offering,
                TargetedTransferState::AwaitingApproval,
            )
            .await
        {
            self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                .await;
            return Err(error);
        }
        self.emit_targeted_lifecycle(&transfer_uuid, "awaiting-approval");

        let response = match tokio::time::timeout(
            self.connection_timeout() + self.offer_wait_timeout(),
            client.submit_offer(SubmitTargetedOffer {
                proof,
                generation,
                relationship_protocol_version,
                protocol_version,
                transfer_id: transfer_uuid.clone(),
                sender_endpoint_id: sender_endpoint_id.clone(),
                receiver_endpoint_id: receiver_endpoint_id.clone(),
                manifest_id: blob_ticket.hash().to_string(),
                content_hash: blob_ticket.hash().to_string(),
                transfer_name: payload_name.clone(),
                file_count: import.file_count,
                total_size: import.total_size,
                relay_mode: self.relay_mode,
                relay_urls: self
                    .custom_relay_urls
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                if store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Failed,
                    )
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(map_connect_failure(error));
            }
            Err(_) => {
                if store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Failed,
                    )
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(VnidropError::offer_timeout(anyhow::anyhow!(
                    "offer timed out"
                )));
            }
        };

        match response {
            WireOfferResponse::Accepted => {}
            WireOfferResponse::Declined { reason } => {
                if store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Declined,
                    )
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "offer-declined");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(VnidropError::permission(anyhow::anyhow!(
                    "targeted offer declined: {reason}"
                )));
            }
            WireOfferResponse::Refused { reason } => {
                if store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Failed,
                    )
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(map_offer_refuse_reason(&reason));
            }
        }

        // Permanent until cancel/delete — approved targeted transfers must resume.
        self.access_policy
            .approve_endpoint_until(protocol_transfer_id, receiver_endpoint_id.clone(), None)
            .await;

        let authorization = match TargetedAuthorization::issue(TargetedAuthorizationDraft {
            transfer_id: transfer_uuid.clone(),
            protocol_transfer_id,
            sender_endpoint_id,
            receiver_endpoint_id,
            manifest_id: blob_ticket.hash().to_string(),
            content_hash: blob_ticket.hash().to_string(),
            file_count: import.file_count,
            total_size: import.total_size,
            protocol_version,
            transfer_name: payload_name,
            blob_ticket: blob_ticket.to_string(),
        }) {
            Ok(authorization) => authorization,
            Err(error) => {
                if store
                    .set_state_from_any(&transfer_uuid, TargetedTransferState::Failed)
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(error);
            }
        };
        if let Err(error) = self
            .persist_authorization_secret(&transfer_uuid, &authorization)
            .await
        {
            if store
                .set_state_from_any(&transfer_uuid, TargetedTransferState::Failed)
                .await
                .is_ok()
            {
                self.emit_targeted_lifecycle(&transfer_uuid, "failed");
            }
            self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                .await;
            return Err(error);
        }
        let encoded = match authorization.encode() {
            Ok(encoded) => encoded,
            Err(error) => {
                if store
                    .set_state_from_any(&transfer_uuid, TargetedTransferState::Failed)
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(error);
            }
        };

        if let Err(error) = store
            .set_state(
                &transfer_uuid,
                TargetedTransferState::AwaitingApproval,
                TargetedTransferState::Approved,
            )
            .await
        {
            self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                .await;
            return Err(error);
        }
        self.emit_targeted_lifecycle(&transfer_uuid, "approved");

        let deliver = match client
            .deliver_authorization(DeliverTargetedAuthorization {
                transfer_id: transfer_uuid.clone(),
                authorization: encoded,
            })
            .await
            .context("failed to deliver targeted authorization")
            .map_err(VnidropError::network)
        {
            Ok(deliver) => deliver,
            Err(error) => {
                if store
                    .set_state_from_any(&transfer_uuid, TargetedTransferState::Failed)
                    .await
                    .is_ok()
                {
                    self.emit_targeted_lifecycle(&transfer_uuid, "failed");
                }
                self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                    .await;
                return Err(error);
            }
        };
        if deliver != crate::targeted_transfer::protocol::DeliverAuthorizationResponse::Stored {
            if store
                .set_state_from_any(&transfer_uuid, TargetedTransferState::Failed)
                .await
                .is_ok()
            {
                self.emit_targeted_lifecycle(&transfer_uuid, "failed");
            }
            self.teardown_targeted_payload(protocol_transfer_id, Some(&transfer_uuid))
                .await;
            return Err(VnidropError::network(anyhow::anyhow!(
                "receiver rejected authorization delivery"
            )));
        }

        store
            .get(&transfer_uuid)
            .await?
            .ok_or_else(|| VnidropError::internal(anyhow::anyhow!("targeted transfer missing")))
    }

    async fn teardown_targeted_payload(&self, protocol_transfer_id: u64, id: Option<&str>) {
        if let Err(error) = self
            .try_teardown_targeted_payload(protocol_transfer_id, id)
            .await
        {
            tracing::warn!(%error, "failed to release targeted payload");
        }
    }

    async fn try_teardown_targeted_payload(
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

    pub(super) async fn receive_targeted_transfer(
        self: &Arc<Self>,
        transfer_id: String,
        output_dir: String,
    ) -> Result<(), VnidropError> {
        let output_dir =
            crate::filesystem::platform_path(&output_dir).map_err(VnidropError::filesystem)?;
        self.receive_targeted_to_target(transfer_id, ReceiveTarget::Directory(output_dir))
            .await
    }

    pub(super) async fn receive_targeted_transfer_with_output_sink(
        self: &Arc<Self>,
        transfer_id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSink>,
    ) -> Result<(), VnidropError> {
        self.receive_targeted_to_target(transfer_id, ReceiveTarget::OutputSink(output_sink))
            .await
    }

    pub(super) async fn receive_targeted_transfer_with_output_sink_v2(
        self: &Arc<Self>,
        transfer_id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSinkV2>,
    ) -> Result<(), VnidropError> {
        self.receive_targeted_to_target(transfer_id, ReceiveTarget::OutputSinkV2(output_sink))
            .await
    }

    pub(super) async fn resume_targeted_transfer(
        self: &Arc<Self>,
        id: String,
        output_dir: String,
    ) -> Result<(), VnidropError> {
        let output_dir =
            crate::filesystem::platform_path(&output_dir).map_err(VnidropError::filesystem)?;
        self.resume_targeted_to_target(id, ReceiveTarget::Directory(output_dir))
            .await
    }

    pub(super) async fn resume_targeted_transfer_with_output_sink(
        self: &Arc<Self>,
        id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSink>,
    ) -> Result<(), VnidropError> {
        self.resume_targeted_to_target(id, ReceiveTarget::OutputSink(output_sink))
            .await
    }

    pub(super) async fn resume_targeted_transfer_with_output_sink_v2(
        self: &Arc<Self>,
        id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSinkV2>,
    ) -> Result<(), VnidropError> {
        self.resume_targeted_to_target(id, ReceiveTarget::OutputSinkV2(output_sink))
            .await
    }

    async fn receive_targeted_to_target(
        self: &Arc<Self>,
        transfer_id: String,
        target: ReceiveTarget,
    ) -> Result<(), VnidropError> {
        let auth = self.load_receiver_authorization(&transfer_id).await?;
        self.run_targeted_receive(&auth, target).await
    }

    async fn resume_targeted_to_target(
        self: &Arc<Self>,
        id: String,
        target: ReceiveTarget,
    ) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        let row = store.get_row(&id).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("unknown targeted transfer"))
        })?;
        if !matches!(
            row.state,
            TargetedTransferState::Approved
                | TargetedTransferState::Connecting
                | TargetedTransferState::Transferring
                | TargetedTransferState::Interrupted
        ) {
            return Err(VnidropError::InvalidTransition {
                reason: format!(
                    "cannot resume from {}",
                    crate::targeted_transfer::state_as_str(row.state)
                ),
            });
        }
        let auth = self.load_receiver_authorization(&id).await?;
        self.run_targeted_receive(&auth, target).await
    }

    async fn load_receiver_authorization(
        &self,
        transfer_id: &str,
    ) -> Result<TargetedAuthorization, VnidropError> {
        let store = self.targeted_store();
        let row = store.get_row(transfer_id).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("unknown targeted transfer"))
        })?;
        let encoded = self.load_stored_authorization(&row).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!(
                "targeted transfer has no durable authorization"
            ))
        })?;
        let auth = TargetedAuthorization::decode(&encoded)?;
        auth.verify_for_receiver(&self.endpoint.id().to_string())?;
        Ok(auth)
    }

    async fn run_targeted_receive(
        self: &Arc<Self>,
        auth: &TargetedAuthorization,
        target: ReceiveTarget,
    ) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        if let Ok(Some(row)) = store.get_row(&auth.transfer_id).await {
            match row.state {
                TargetedTransferState::Approved | TargetedTransferState::Interrupted => {
                    store
                        .set_state(
                            &auth.transfer_id,
                            row.state,
                            TargetedTransferState::Connecting,
                        )
                        .await?;
                    self.emit_targeted_lifecycle(&auth.transfer_id, "connecting");
                    store
                        .set_state(
                            &auth.transfer_id,
                            TargetedTransferState::Connecting,
                            TargetedTransferState::Transferring,
                        )
                        .await?;
                    self.emit_targeted_lifecycle(&auth.transfer_id, "transferring");
                }
                TargetedTransferState::Connecting | TargetedTransferState::Transferring => {
                    return Err(VnidropError::InvalidTransition {
                        reason: "targeted receive is already active".to_string(),
                    });
                }
                other => {
                    return Err(VnidropError::InvalidTransition {
                        reason: format!(
                            "cannot receive from {}",
                            crate::targeted_transfer::state_as_str(other)
                        ),
                    });
                }
            }
        }

        let blob_ticket = BlobTicket::from_str_compat(&auth.blob_ticket)
            .map_err(|error| VnidropError::ticket(anyhow::anyhow!(error)))?;
        let receive_result = self
            .receive_targeted_payload(
                &auth.transfer_id,
                auth.protocol_transfer_id,
                auth.file_count,
                auth.total_size,
                blob_ticket,
                target,
            )
            .await;

        match receive_result {
            Ok(()) => {
                let row = store.get_row(&auth.transfer_id).await?.ok_or_else(|| {
                    VnidropError::internal(anyhow::anyhow!("targeted transfer missing"))
                })?;
                store
                    .complete_receiver_and_enqueue(&auth.transfer_id, row.total_size)
                    .await?;
                self.emit_targeted_lifecycle(&auth.transfer_id, "completed");
                if self.acknowledge_targeted_completion(auth).await.is_ok() {
                    store.clear_pending_completion(&auth.transfer_id).await?;
                }
                Ok(())
            }
            Err(error) => {
                if let Ok(Some(row)) = store.get_row(&auth.transfer_id).await {
                    if matches!(
                        row.state,
                        TargetedTransferState::Connecting | TargetedTransferState::Transferring
                    ) && store
                        .set_state_from_any(&auth.transfer_id, TargetedTransferState::Interrupted)
                        .await
                        .is_ok()
                    {
                        self.emit_targeted_lifecycle(&auth.transfer_id, "interrupted");
                    }
                }
                Err(VnidropError::transfer(error))
            }
        }
    }

    async fn acknowledge_targeted_completion(
        &self,
        auth: &TargetedAuthorization,
    ) -> Result<(), VnidropError> {
        #[cfg(test)]
        if self
            .suppress_targeted_completion
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(VnidropError::device_unavailable(anyhow::anyhow!(
                "completion delivery suppressed by test"
            )));
        }
        let addr = self
            .device_relationships
            .peer_addr(&auth.sender_endpoint_id)
            .await?;
        let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
        let response = tokio::time::timeout(
            self.connection_timeout(),
            client.complete_transfer(CompleteTargetedTransfer {
                transfer_id: auth.transfer_id.clone(),
                verified_bytes: auth.total_size,
                authorization: auth.encode()?,
            }),
        )
        .await
        .map_err(|_| VnidropError::device_unavailable(anyhow::anyhow!("completion timed out")))?
        .map_err(|error| VnidropError::network(anyhow::anyhow!(error)))?;
        if response != CompletionResponse::Recorded {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "sender rejected targeted completion"
            )));
        }
        Ok(())
    }

    async fn persist_authorization_secret(
        &self,
        transfer_id: &str,
        authorization: &TargetedAuthorization,
    ) -> Result<(), VnidropError> {
        let custody =
            self.secret_custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "targeted authorization requires protected custody".to_string(),
                })?;
        let material = auth_secret_material(authorization)?;
        let handle = custody
            .protect(SecretKind::TargetedAuthorization, material, None)
            .await?;
        if let Err(error) = self
            .targeted_store()
            .store_authorization(transfer_id, &authorization.blob_ticket, handle.as_str())
            .await
        {
            if let Err(cleanup_error) = custody.remove(&handle).await {
                tracing::warn!(%cleanup_error, "failed to roll back targeted authorization secret");
            }
            return Err(error);
        }
        Ok(())
    }

    async fn persist_receiver_authorization(&self, encoded: &str) -> Result<(), VnidropError> {
        let auth = TargetedAuthorization::decode(encoded)?;
        let store = self.targeted_store();
        if store.get_row(&auth.transfer_id).await?.is_none() {
            let invitation_collision = self
                .repository
                .list_transfers()
                .await
                .map_err(VnidropError::repository)?
                .into_iter()
                .any(|transfer| transfer.transfer_id == auth.protocol_transfer_id);
            if invitation_collision
                || store
                    .contains_protocol_id(auth.protocol_transfer_id)
                    .await?
            {
                return Err(VnidropError::invalid_input(anyhow::anyhow!(
                    "targeted transfer protocol id collides with local work"
                )));
            }
            let now = now_ms();
            store
                .insert(&TargetedTransferRow {
                    id: auth.transfer_id.clone(),
                    protocol_transfer_id: auth.protocol_transfer_id,
                    sender_endpoint_id: auth.sender_endpoint_id.clone(),
                    receiver_endpoint_id: auth.receiver_endpoint_id.clone(),
                    manifest_id: auth.manifest_id.clone(),
                    content_hash: auth.content_hash.clone(),
                    transfer_name: auth.transfer_name.clone(),
                    file_count: auth.file_count,
                    total_size: auth.total_size,
                    verified_bytes: 0,
                    blob_ticket: Some(auth.blob_ticket.clone()),
                    authorization_secret_handle: None,
                    role: TargetedTransferRole::Receiver,
                    state: TargetedTransferState::Approved,
                    created_at: now,
                    updated_at: now,
                })
                .await?;
        }
        self.persist_authorization_secret(&auth.transfer_id, &auth)
            .await
    }

    pub(crate) async fn load_stored_authorization(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<Option<String>, VnidropError> {
        let (Some(handle), Some(blob_ticket)) = (
            row.authorization_secret_handle.as_ref(),
            row.blob_ticket.as_ref(),
        ) else {
            return Ok(None);
        };
        let custody =
            self.secret_custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "targeted authorization requires protected custody".to_string(),
                })?;
        let material = custody
            .load(&SecretHandle::from_stored(handle.clone()))
            .await?;
        let auth = reconstruct_authorization(
            TargetedAuthorizationDraft {
                transfer_id: row.id.clone(),
                protocol_transfer_id: row.protocol_transfer_id,
                sender_endpoint_id: row.sender_endpoint_id.clone(),
                receiver_endpoint_id: row.receiver_endpoint_id.clone(),
                manifest_id: row.manifest_id.clone(),
                content_hash: row.content_hash.clone(),
                file_count: row.file_count,
                total_size: row.total_size,
                protocol_version: experimental_saved_device_capabilities()
                    .targeted_transfer_protocol_version,
                transfer_name: row.transfer_name.clone(),
                blob_ticket: blob_ticket.clone(),
            },
            &material,
        )?;
        Ok(Some(auth.encode()?))
    }
}

fn allocate_protocol_transfer_id(transfer_uuid: &str) -> u64 {
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

fn map_connect_failure(error: irpc::Error) -> VnidropError {
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

trait BlobTicketParse {
    fn from_str_compat(value: &str) -> Result<BlobTicket, String>;
}

impl BlobTicketParse for BlobTicket {
    fn from_str_compat(value: &str) -> Result<BlobTicket, String> {
        use std::str::FromStr;
        BlobTicket::from_str(value).map_err(|error| error.to_string())
    }
}
