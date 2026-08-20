//! Durable lifecycle authority for Targeted transfers.

use std::{future::Future, pin::Pin, sync::Arc};

use iroh::RelayUrl;

use crate::{
    api::{
        saved_device_capabilities, CoreLimits, CoreRelayMode, PendingTargetedOffer,
        TargetedTransfer, TargetedTransferState,
    },
    device_relationship::DeviceRelationshipService,
    error::VnidropError,
    grant::Challenge,
    invitation::Repository,
    secure_secret::SecretCustody,
    ticket::relay_profiles_compatible,
    util::now_ms,
};

use super::{
    authorization_custody::AuthorizationCustody,
    inbox::{TargetedOfferDecision, TargetedOfferInbox},
    protocol::{
        parse_offer_relay_urls, CancelTargetedOffer, CancelWireOfferResponse,
        CompleteTargetedTransfer, CompletionResponse, DeliverAuthorizationResponse,
        DeliverTargetedAuthorization, SubmitTargetedOffer, WireOfferResponse,
    },
    state_as_str, TargetedAuthorization, TargetedTransferRole, TargetedTransferRow,
    TargetedTransferStore,
};

pub(crate) type TargetedCleanupFuture =
    Pin<Box<dyn Future<Output = Result<(), VnidropError>> + Send>>;

pub(crate) type Cleanup = Arc<dyn Fn(TargetedTransferRow) -> TargetedCleanupFuture + Send + Sync>;
pub(crate) type EmitLifecycle = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub(crate) struct TargetedTransferModuleConfig {
    pub(crate) store: TargetedTransferStore,
    pub(crate) relationships: Arc<DeviceRelationshipService>,
    pub(crate) inbox: TargetedOfferInbox,
    pub(crate) limits: CoreLimits,
    pub(crate) local_endpoint_id: String,
    pub(crate) relay_mode: CoreRelayMode,
    pub(crate) custom_relay_urls: Vec<RelayUrl>,
    pub(crate) repository: Repository,
    pub(crate) custody: Option<Arc<SecretCustody>>,
    pub(crate) cleanup: Cleanup,
    pub(crate) emit_lifecycle: EmitLifecycle,
}

struct TargetedProtocolAuthority {
    relationships: TargetedRelationshipAccess,
    inbox: TargetedOfferInbox,
    limits: CoreLimits,
    local_endpoint_id: String,
    relay_mode: CoreRelayMode,
    custom_relay_urls: Vec<RelayUrl>,
}

/// The Targeted module consults relationship authority without owning it.
struct TargetedRelationshipAccess(Arc<DeviceRelationshipService>);

impl TargetedRelationshipAccess {
    async fn require_saved(&self, endpoint_id: &str) -> Result<(), VnidropError> {
        self.0.require_saved(endpoint_id).await.map(|_| ())
    }

    async fn verify_saved_possession(
        &self,
        endpoint_id: &str,
        challenge: &Challenge,
        proof: &crate::device_relationship::WireProof,
        generation: u64,
        protocol_version: u16,
    ) -> Result<(), VnidropError> {
        self.0
            .verify_saved_possession(endpoint_id, challenge, proof, generation, protocol_version)
            .await
    }
}

#[derive(Clone)]
pub(crate) struct TargetedTransferModule {
    store: TargetedTransferStore,
    authorizations: AuthorizationCustody,
    protocol: Option<Arc<TargetedProtocolAuthority>>,
    cleanup: Cleanup,
    emit_lifecycle: EmitLifecycle,
}

impl TargetedTransferModule {
    pub(crate) fn new(config: TargetedTransferModuleConfig) -> Self {
        Self {
            store: config.store.clone(),
            authorizations: AuthorizationCustody::new(
                config.store,
                config.repository,
                config.custody,
                config.emit_lifecycle.clone(),
            ),
            protocol: Some(Arc::new(TargetedProtocolAuthority {
                relationships: TargetedRelationshipAccess(config.relationships),
                inbox: config.inbox,
                limits: config.limits,
                local_endpoint_id: config.local_endpoint_id,
                relay_mode: config.relay_mode,
                custom_relay_urls: config.custom_relay_urls,
            })),
            cleanup: config.cleanup,
            emit_lifecycle: config.emit_lifecycle,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        store: TargetedTransferStore,
        cleaned: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            authorizations: AuthorizationCustody::new(
                store.clone(),
                Repository::from_pool(store.pool.clone()),
                None,
                Arc::new(|_, _| {}),
            ),
            store,
            protocol: None,
            cleanup: Arc::new(move |row| {
                let cleaned = cleaned.clone();
                Box::pin(async move {
                    let mut cleaned = cleaned.lock().expect("cleanup log");
                    if !cleaned.contains(&row.id) {
                        cleaned.push(row.id);
                    }
                    Ok(())
                })
            }),
            emit_lifecycle: Arc::new(|_, _| {}),
        }
    }

    fn protocol(&self) -> &TargetedProtocolAuthority {
        self.protocol
            .as_deref()
            .expect("Targeted wire authority is configured")
    }

    pub(crate) async fn handle_offer(
        &self,
        remote_endpoint_id: &str,
        challenge: &Challenge,
        offer: SubmitTargetedOffer,
    ) -> WireOfferResponse {
        let authority = self.protocol();
        let expected = saved_device_capabilities().targeted_transfer_protocol_version;
        if authority.inbox.cooldown().is_cooling(remote_endpoint_id) {
            return WireOfferResponse::Refused {
                reason: "identity-cooldown".to_string(),
            };
        }
        if offer.protocol_version != expected {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "protocol-incompatible".to_string(),
            };
        }
        if offer.receiver_endpoint_id != authority.local_endpoint_id {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "receiver-mismatch".to_string(),
            };
        }
        if offer.sender_endpoint_id != remote_endpoint_id {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "sender-mismatch".to_string(),
            };
        }
        if offer.file_count == 0
            || offer.file_count > authority.limits.max_collection_files
            || offer.total_size == 0
            || offer.total_size > authority.limits.max_total_bytes
            || offer.transfer_id.is_empty()
            || offer.manifest_id.is_empty()
            || offer.content_hash.is_empty()
        {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "manifest-limits".to_string(),
            };
        }
        if let Err(error) = authority
            .limits
            .validate_metadata_text("transfer name", Some(offer.transfer_name.as_str()))
        {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: error.to_string(),
            };
        }

        if let Ok(Some(existing)) = self.store.get_row(&offer.transfer_id).await {
            if existing.manifest_id != offer.manifest_id
                || existing.content_hash != offer.content_hash
                || existing.file_count != offer.file_count
                || existing.total_size != offer.total_size
                || existing.sender_endpoint_id != offer.sender_endpoint_id
                || existing.receiver_endpoint_id != offer.receiver_endpoint_id
            {
                return WireOfferResponse::Refused {
                    reason: "immutable-transfer-mismatch".to_string(),
                };
            }
            return match existing.state {
                TargetedTransferState::Approved
                | TargetedTransferState::Connecting
                | TargetedTransferState::Transferring
                | TargetedTransferState::Interrupted
                | TargetedTransferState::Completed => WireOfferResponse::Accepted,
                TargetedTransferState::Declined => WireOfferResponse::Declined {
                    reason: "receiver-declined".to_string(),
                },
                TargetedTransferState::Cancelled => WireOfferResponse::Declined {
                    reason: "cancelled".to_string(),
                },
                TargetedTransferState::Failed | TargetedTransferState::Deleted => {
                    WireOfferResponse::Refused {
                        reason: format!("transfer-{}", state_as_str(existing.state)),
                    }
                }
                TargetedTransferState::Preparing
                | TargetedTransferState::Offering
                | TargetedTransferState::AwaitingApproval => WireOfferResponse::Accepted,
            };
        }

        let remote_urls = match parse_offer_relay_urls(&offer.relay_urls) {
            Ok(urls) => urls,
            Err(()) => {
                return WireOfferResponse::Refused {
                    reason: "relay-policy-incompatible".to_string(),
                };
            }
        };
        if !relay_profiles_compatible(
            authority.relay_mode,
            &authority.custom_relay_urls,
            offer.relay_mode,
            &remote_urls,
        ) {
            return WireOfferResponse::Refused {
                reason: "relay-policy-incompatible".to_string(),
            };
        }

        if let Err(error) = authority
            .relationships
            .verify_saved_possession(
                remote_endpoint_id,
                challenge,
                &offer.proof,
                offer.generation,
                offer.relationship_protocol_version,
            )
            .await
        {
            tracing::debug!(error = %error, "targeted offer relationship proof rejected");
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: if matches!(error, VnidropError::ProtocolIncompatible { .. }) {
                    "protocol-incompatible"
                } else {
                    "unauthenticated"
                }
                .to_string(),
            };
        }

        let pending = PendingTargetedOffer {
            transfer_id: offer.transfer_id,
            sender_endpoint_id: remote_endpoint_id.to_string(),
            receiver_endpoint_id: authority.local_endpoint_id.clone(),
            manifest_id: offer.manifest_id,
            content_hash: offer.content_hash,
            transfer_name: offer.transfer_name,
            file_count: offer.file_count,
            total_size: offer.total_size,
            protocol_version: offer.protocol_version,
            received_at: now_ms(),
        };
        match authority.inbox.submit(pending).await {
            TargetedOfferDecision::Accepted => WireOfferResponse::Accepted,
            TargetedOfferDecision::Declined { reason } => WireOfferResponse::Declined { reason },
            TargetedOfferDecision::Refused { reason } => WireOfferResponse::Refused { reason },
        }
    }

    pub(crate) async fn handle_authorization(
        &self,
        remote_endpoint_id: &str,
        delivery: DeliverTargetedAuthorization,
    ) -> DeliverAuthorizationResponse {
        let authority = self.protocol();
        if authority
            .relationships
            .require_saved(remote_endpoint_id)
            .await
            .is_err()
        {
            return DeliverAuthorizationResponse::Rejected;
        }
        let Ok(auth) = TargetedAuthorization::decode(&delivery.authorization) else {
            return DeliverAuthorizationResponse::Rejected;
        };
        if auth.sender_endpoint_id != remote_endpoint_id
            || auth.receiver_endpoint_id != authority.local_endpoint_id
            || auth.transfer_id != delivery.transfer_id
            || auth.protocol_version
                != saved_device_capabilities().targeted_transfer_protocol_version
        {
            return DeliverAuthorizationResponse::Rejected;
        }
        let Ok(ticket) = auth.blob_ticket.parse::<iroh_blobs::ticket::BlobTicket>() else {
            return DeliverAuthorizationResponse::Rejected;
        };
        if ticket.hash().to_string() != auth.manifest_id || auth.manifest_id != auth.content_hash {
            return DeliverAuthorizationResponse::Rejected;
        }
        if self.persist_authorization(auth).await.is_err() {
            return DeliverAuthorizationResponse::Rejected;
        }
        let _ = authority
            .inbox
            .deliver_authorization(&delivery.transfer_id, delivery.authorization)
            .await;
        DeliverAuthorizationResponse::Stored
    }

    pub(crate) async fn handle_cancel(
        &self,
        remote_endpoint_id: &str,
        cancel: CancelTargetedOffer,
    ) -> CancelWireOfferResponse {
        let authority = self.protocol();
        if let Some(pending) = authority.inbox.get_pending(&cancel.transfer_id).await {
            if pending.sender_endpoint_id != remote_endpoint_id {
                return CancelWireOfferResponse::Rejected;
            }
            authority.inbox.discard(&cancel.transfer_id).await;
            return match self.clear_accepted_offer_intent(&cancel.transfer_id).await {
                Ok(()) => CancelWireOfferResponse::Cancelled,
                Err(_) => CancelWireOfferResponse::Rejected,
            };
        }
        if !matches!(
            cancel.terminal,
            Some(TargetedTransferState::Cancelled | TargetedTransferState::Deleted)
        ) {
            return CancelWireOfferResponse::Cancelled;
        }
        match self
            .cancel_from_peer(remote_endpoint_id, &cancel.transfer_id)
            .await
        {
            Ok(true) => CancelWireOfferResponse::Cancelled,
            Ok(false) | Err(_) => CancelWireOfferResponse::Rejected,
        }
    }

    pub(crate) async fn handle_completion(
        &self,
        remote_endpoint_id: &str,
        completion: CompleteTargetedTransfer,
    ) -> CompletionResponse {
        let authority = self.protocol();
        if authority
            .relationships
            .require_saved(remote_endpoint_id)
            .await
            .is_err()
        {
            return CompletionResponse::Rejected;
        }
        let Ok(Some(row)) = self.get_row(&completion.transfer_id).await else {
            return CompletionResponse::Rejected;
        };
        let Ok(auth) = TargetedAuthorization::decode(&completion.authorization) else {
            return CompletionResponse::Rejected;
        };
        if row.role != TargetedTransferRole::Sender
            || row.receiver_endpoint_id != remote_endpoint_id
            || auth.receiver_endpoint_id != remote_endpoint_id
            || auth.sender_endpoint_id != authority.local_endpoint_id
            || auth.transfer_id != row.id
            || auth.protocol_transfer_id != row.protocol_transfer_id
            || auth.manifest_id != row.manifest_id
            || auth.content_hash != row.content_hash
            || auth.transfer_name != row.transfer_name
            || auth.file_count != row.file_count
            || auth.total_size != row.total_size
            || auth.total_size != completion.verified_bytes
            || row.blob_ticket.as_deref() != Some(auth.blob_ticket.as_str())
            || auth.protocol_version
                != saved_device_capabilities().targeted_transfer_protocol_version
        {
            return CompletionResponse::Rejected;
        }
        if row.state == TargetedTransferState::Completed {
            return CompletionResponse::Recorded;
        }
        if matches!(
            row.state,
            TargetedTransferState::Cancelled
                | TargetedTransferState::Declined
                | TargetedTransferState::Failed
                | TargetedTransferState::Deleted
        ) {
            return CompletionResponse::Rejected;
        }
        match self.mark_sender_completed(&completion.transfer_id).await {
            Ok(_) => CompletionResponse::Recorded,
            Err(_) => CompletionResponse::Rejected,
        }
    }

    pub(crate) async fn get_row(
        &self,
        id: &str,
    ) -> Result<Option<TargetedTransferRow>, VnidropError> {
        self.store.get_row(id).await
    }

    pub(crate) async fn get(&self, id: &str) -> Result<Option<TargetedTransfer>, VnidropError> {
        self.store.get(id).await
    }

    pub(crate) async fn list(&self) -> Result<Vec<TargetedTransfer>, VnidropError> {
        self.store.list().await
    }

    pub(crate) async fn contains_protocol_id(&self, id: u64) -> Result<bool, VnidropError> {
        self.store.contains_protocol_id(id).await
    }

    pub(crate) async fn register_sender(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<(), VnidropError> {
        self.store.insert(row).await?;
        (self.emit_lifecycle)(&row.id, "created");
        self.store
            .set_state(
                &row.id,
                TargetedTransferState::Preparing,
                TargetedTransferState::Offering,
            )
            .await?;
        (self.emit_lifecycle)(&row.id, "offering");
        Ok(())
    }

    pub(crate) async fn transition(
        &self,
        id: &str,
        from: TargetedTransferState,
        to: TargetedTransferState,
        event: &str,
    ) -> Result<(), VnidropError> {
        self.store.set_state(id, from, to).await?;
        (self.emit_lifecycle)(id, event);
        Ok(())
    }

    pub(crate) async fn transition_from_any(
        &self,
        id: &str,
        to: TargetedTransferState,
        event: &str,
    ) -> Result<(), VnidropError> {
        self.store.set_state_from_any(id, to).await?;
        (self.emit_lifecycle)(id, event);
        Ok(())
    }

    pub(crate) async fn persist_accepted_offer_intent(
        &self,
        offer: &PendingTargetedOffer,
    ) -> Result<(), VnidropError> {
        self.store.persist_accepted_offer_intent(offer).await
    }

    pub(crate) async fn clear_accepted_offer_intent(&self, id: &str) -> Result<(), VnidropError> {
        self.store.clear_accepted_offer_intent(id).await
    }

    pub(crate) async fn clear_accepted_intent_if_sender(
        &self,
        id: &str,
        sender: &str,
    ) -> Result<(), VnidropError> {
        self.store.clear_accepted_intent_if_sender(id, sender).await
    }

    pub(crate) async fn list_accepted_intent_senders(
        &self,
    ) -> Result<Vec<(String, String)>, VnidropError> {
        self.store.list_accepted_intent_senders().await
    }

    pub(crate) async fn list_resumable_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        self.store.list_resumable_rows().await
    }

    pub(crate) async fn list_resumable_sender_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        self.store.list_resumable_sender_rows().await
    }

    pub(crate) async fn authorization_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        self.store.authorization_rows().await
    }

    pub(crate) async fn clear_authorization(&self, id: &str) -> Result<(), VnidropError> {
        self.store.clear_authorization(id).await
    }

    pub(crate) async fn fail_resumable(&self, id: &str) -> Result<bool, VnidropError> {
        let changed = self.store.fail_resumable_and_clear_delivery(id).await?;
        if changed {
            (self.emit_lifecycle)(id, "failed");
        }
        Ok(changed)
    }

    pub(crate) async fn transition_terminal(
        &self,
        id: &str,
        state: TargetedTransferState,
        clear_authorization: bool,
    ) -> Result<bool, VnidropError> {
        let changed = self
            .store
            .transition_terminal(id, state, clear_authorization)
            .await?;
        if changed {
            (self.emit_lifecycle)(id, state_as_str(state));
        }
        Ok(changed)
    }

    pub(crate) async fn terminate_local(
        &self,
        id: &str,
        state: TargetedTransferState,
    ) -> Result<Option<(TargetedTransferRow, bool)>, VnidropError> {
        let Some(row) = self.store.get_row(id).await? else {
            self.protocol().inbox.discard(id).await;
            self.store.clear_accepted_offer_intent(id).await?;
            return Ok(None);
        };
        let already_terminal = row.state == state;
        if already_terminal
            && row.authorization_secret_handle.is_none()
            && row.blob_ticket.is_none()
        {
            self.protocol().inbox.discard(id).await;
            return Ok(Some((row, false)));
        }
        let changed = if already_terminal {
            false
        } else {
            self.transition_terminal(id, state, true).await?
        };
        self.protocol().inbox.discard(id).await;
        (self.cleanup)(row.clone()).await?;
        Ok(Some((row, changed)))
    }

    pub(crate) async fn cancel_by_peer(&self, peer: &str) -> Result<Vec<String>, VnidropError> {
        let ids = self.store.cancel_by_peer(peer).await?;
        for id in &ids {
            (self.emit_lifecycle)(id, "cancelled");
        }
        Ok(ids)
    }

    pub(crate) async fn terminate_peer(&self, peer: &str) -> Result<u64, VnidropError> {
        self.protocol().inbox.discard_from(peer).await;
        let mut rows = Vec::new();
        for id in self.store.ids_for_peer(peer).await? {
            if let Some(row) = self.store.get_row(&id).await? {
                rows.push(row);
            }
        }
        let cancelled = self.cancel_by_peer(peer).await?;
        for row in rows {
            (self.cleanup)(row).await?;
        }
        Ok(cancelled.len() as u64)
    }

    pub(crate) async fn recover_in_flight(&self) -> Result<Vec<String>, VnidropError> {
        let ids = self.store.mark_interrupted_in_flight().await?;
        for id in &ids {
            (self.emit_lifecycle)(id, "interrupted");
        }
        Ok(ids)
    }

    pub(crate) async fn persist_authorization(
        &self,
        authorization: TargetedAuthorization,
    ) -> Result<bool, VnidropError> {
        self.authorizations.persist_receiver(authorization).await
    }

    /// Applies a peer stop before cleanup, so approval/stop races fail closed.
    pub(crate) async fn cancel_from_peer(
        &self,
        remote_endpoint_id: &str,
        transfer_id: &str,
    ) -> Result<bool, VnidropError> {
        let Some(row) = self.store.get_row(transfer_id).await? else {
            self.store
                .clear_accepted_intent_if_sender(transfer_id, remote_endpoint_id)
                .await?;
            return Ok(true);
        };
        let remote_is_peer = match row.role {
            TargetedTransferRole::Sender => row.receiver_endpoint_id == remote_endpoint_id,
            TargetedTransferRole::Receiver => row.sender_endpoint_id == remote_endpoint_id,
        };
        if !remote_is_peer {
            return Ok(false);
        }
        if row.state == TargetedTransferState::Cancelled {
            (self.cleanup)(row).await?;
            return Ok(true);
        }
        if matches!(
            row.state,
            TargetedTransferState::Completed
                | TargetedTransferState::Declined
                | TargetedTransferState::Failed
                | TargetedTransferState::Deleted
        ) {
            return Ok(true);
        }
        let changed = self
            .store
            .transition_terminal(transfer_id, TargetedTransferState::Cancelled, false)
            .await?;
        if changed {
            (self.emit_lifecycle)(transfer_id, "cancelled");
            (self.cleanup)(row).await?;
        }
        Ok(true)
    }

    pub(crate) async fn mark_sender_completed(
        &self,
        transfer_id: &str,
    ) -> Result<bool, VnidropError> {
        let changed = self.store.mark_sender_completed(transfer_id).await?;
        if changed {
            (self.emit_lifecycle)(transfer_id, "completed");
        }
        Ok(changed)
    }

    pub(crate) async fn protect_sender_authorization(
        &self,
        authorization: &TargetedAuthorization,
    ) -> Result<(), VnidropError> {
        self.authorizations.protect_sender(authorization).await
    }

    pub(crate) async fn load_authorization(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<Option<String>, VnidropError> {
        self.authorizations.load(row).await
    }

    pub(crate) async fn begin_receive(
        &self,
        id: &str,
        from: TargetedTransferState,
    ) -> Result<(), VnidropError> {
        self.transition(id, from, TargetedTransferState::Connecting, "connecting")
            .await?;
        self.transition(
            id,
            TargetedTransferState::Connecting,
            TargetedTransferState::Transferring,
            "transferring",
        )
        .await
    }

    pub(crate) async fn complete_receiver(
        &self,
        id: &str,
        verified_bytes: u64,
    ) -> Result<(), VnidropError> {
        self.store
            .complete_receiver_and_enqueue(id, verified_bytes)
            .await?;
        (self.emit_lifecycle)(id, "completed");
        Ok(())
    }

    pub(crate) async fn interrupt_receive(&self, id: &str) -> Result<(), VnidropError> {
        self.store
            .set_state_from_any(id, TargetedTransferState::Interrupted)
            .await?;
        (self.emit_lifecycle)(id, "interrupted");
        Ok(())
    }

    pub(crate) async fn advance_verified_bytes(
        &self,
        id: &str,
        bytes: u64,
    ) -> Result<bool, VnidropError> {
        self.store.advance_verified_bytes(id, bytes).await
    }

    pub(crate) async fn list_pending_authorization_deliveries(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        self.store.list_pending_authorization_deliveries().await
    }

    pub(crate) async fn clear_pending_authorization_delivery(
        &self,
        id: &str,
    ) -> Result<(), VnidropError> {
        self.store.clear_pending_authorization_delivery(id).await
    }

    pub(crate) async fn defer_pending_authorization_delivery(
        &self,
        id: &str,
        at: i64,
    ) -> Result<(), VnidropError> {
        self.store
            .defer_pending_authorization_delivery(id, at)
            .await
    }

    pub(crate) async fn list_pending_completions(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        self.store.list_pending_completions().await
    }

    pub(crate) async fn clear_pending_completion(&self, id: &str) -> Result<(), VnidropError> {
        self.store.clear_pending_completion(id).await
    }

    pub(crate) async fn defer_pending_completion(
        &self,
        id: &str,
        at: i64,
    ) -> Result<(), VnidropError> {
        self.store.defer_pending_completion(id, at).await
    }

    pub(crate) async fn list_completed_sender_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        self.store.list_completed_sender_rows().await
    }

    pub(crate) async fn clear_pending_payload_release(&self, id: &str) -> Result<(), VnidropError> {
        self.store.clear_pending_payload_release(id).await
    }

    #[cfg(test)]
    pub(crate) async fn corrupt_content_hash_for_test(&self, id: &str) -> Result<(), VnidropError> {
        self.store.corrupt_content_hash_for_test(id).await
    }
}
