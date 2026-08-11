//! Create, offer, approve, resume, cancel, and delete targeted transfers.

use std::sync::Arc;

use anyhow::{Context, Result};
use iroh_blobs::{ticket::BlobTicket, BlobFormat};
use uuid::Uuid;

use super::CoreInner;
use crate::{
    api::{
        experimental_saved_device_capabilities, PendingTargetedOffer, ShareMetadataInput,
        ShareSource, TargetedTransfer, TargetedTransferState, TransferAccessMode, TransferMetadata,
    },
    error::VnidropError,
    secure_secret::{SecretHandle, SecretKind},
    targeted_transfer::{
        auth_secret_material,
        protocol::{
            map_offer_refuse_reason, CancelTargetedOffer, DeliverTargetedAuthorization,
            SubmitTargetedOffer, TargetedOfferResponse, TargetedTransferProtocol,
        },
        reconstruct_authorization, TargetedAuthorization, TargetedAuthorizationDraft,
        TargetedTransferRole, TargetedTransferRow, TargetedTransferStore,
    },
    ticket::VnidropTicket,
    util::{non_empty, now_ms},
};

impl CoreInner {
    fn connection_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.limits.connection_timeout_ms)
    }

    fn offer_wait_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.limits.offer_timeout_ms)
    }

    pub(super) fn targeted_store(&self) -> TargetedTransferStore {
        TargetedTransferStore::new(self.repository.sqlite_pool())
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
        for row in self.targeted_store().list_resumable_sender_rows().await? {
            self.access_policy
                .approve_endpoint_until(row.protocol_transfer_id, row.receiver_endpoint_id, None)
                .await;
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
        // Signal active transfers synchronously before awaiting share teardown.
        for protocol_transfer_id in &protocol_ids {
            let _ = self.take_active_transfer(*protocol_transfer_id);
        }
        for protocol_transfer_id in &protocol_ids {
            let _ = self.cancel_idle_or_share(*protocol_transfer_id).await;
        }
        self.targeted_store().cancel_by_peer(peer_endpoint_id).await
    }

    /// Synchronously stop streaming for one transfer (facade calls this first).
    pub(super) fn signal_targeted_transfer_cancel(&self, protocol_transfer_id: u64) -> bool {
        self.take_active_transfer(protocol_transfer_id).is_some()
    }

    pub(super) async fn cancel_targeted_transfer(&self, id: String) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        let Some(row) = store.get_row(&id).await? else {
            // Still drop any live-session offer under this id.
            self.targeted_offers.discard(&id).await;
            return Ok(());
        };
        let _ = self.take_active_transfer(row.protocol_transfer_id);
        self.targeted_offers.discard(&id).await;
        self.access_policy
            .remove_transfer(row.protocol_transfer_id)
            .await;
        let _ = self.cancel_idle_or_share(row.protocol_transfer_id).await;
        if !matches!(
            row.state,
            TargetedTransferState::Completed
                | TargetedTransferState::Declined
                | TargetedTransferState::Cancelled
                | TargetedTransferState::Failed
                | TargetedTransferState::Deleted
        ) {
            let _ = store
                .set_state_from_any(&id, TargetedTransferState::Cancelled)
                .await;
        }
        // Best-effort remote withdraw of an unapproved live offer.
        if row.role == TargetedTransferRole::Sender
            && matches!(
                row.state,
                TargetedTransferState::Offering | TargetedTransferState::AwaitingApproval
            )
        {
            if let Ok(addr) = self
                .device_relationships
                .peer_addr(&row.receiver_endpoint_id)
                .await
            {
                let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
                let _ = tokio::time::timeout(
                    self.connection_timeout(),
                    client.cancel_offer(CancelTargetedOffer {
                        transfer_id: id.clone(),
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
        // Durable local denial first — remote cleanup is best-effort.
        let _ = self.take_active_transfer(row.protocol_transfer_id);
        self.targeted_offers.discard(&id).await;
        self.access_policy
            .remove_transfer(row.protocol_transfer_id)
            .await;
        let _ = self.cancel_idle_or_share(row.protocol_transfer_id).await;
        if let Some(handle) = &row.authorization_secret_handle {
            if let Some(custody) = &self.secret_custody {
                let _ = custody
                    .remove(&SecretHandle::from_stored(handle.clone()))
                    .await;
            }
        }
        store.clear_authorization(&id).await?;
        if row.state != TargetedTransferState::Deleted {
            if !matches!(
                row.state,
                TargetedTransferState::Completed
                    | TargetedTransferState::Declined
                    | TargetedTransferState::Cancelled
                    | TargetedTransferState::Failed
            ) {
                let _ = store
                    .set_state_from_any(&id, TargetedTransferState::Cancelled)
                    .await;
            }
            let _ = store
                .set_state_from_any(&id, TargetedTransferState::Deleted)
                .await;
        }
        Ok(())
    }

    pub(super) async fn respond_to_targeted_offer(
        self: &Arc<Self>,
        transfer_id: String,
        accepted: bool,
    ) -> Result<Option<String>, VnidropError> {
        if let Some(auth) = self
            .targeted_offers
            .settled_authorization(&transfer_id)
            .await
        {
            return Ok(Some(auth));
        }
        if let Ok(Some(row)) = self.targeted_store().get_row(&transfer_id).await {
            if let Some(encoded) = self.load_stored_authorization(&row).await? {
                return Ok(Some(encoded));
            }
        }

        match self.targeted_offers.respond(&transfer_id, accepted).await {
            Ok(Some(auth)) => {
                self.persist_receiver_authorization(&auth).await?;
                Ok(Some(auth))
            }
            Ok(None) => Ok(None),
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

        let transfer_uuid = Uuid::new_v4().to_string();
        let protocol_transfer_id = allocate_protocol_transfer_id(&transfer_uuid);
        let sender_endpoint_id = self.endpoint.id().to_string();
        let now = now_ms();

        let share = self
            .share_files(
                sources,
                ShareMetadataInput {
                    transfer_id: protocol_transfer_id,
                    transfer_name: transfer_name.clone(),
                    sender_name: None,
                    access_mode: TransferAccessMode::ApprovalRequired,
                },
            )
            .await
            .map_err(VnidropError::transfer)?;

        let store = self.targeted_store();
        let row = TargetedTransferRow {
            id: transfer_uuid.clone(),
            protocol_transfer_id,
            sender_endpoint_id: sender_endpoint_id.clone(),
            receiver_endpoint_id: receiver_endpoint_id.clone(),
            manifest_id: share.hash.clone(),
            content_hash: share.hash.clone(),
            transfer_name: share.transfer_name.clone(),
            file_count: share.file_count,
            total_size: share.total_size,
            verified_bytes: 0,
            blob_ticket: None,
            authorization_secret_handle: None,
            role: TargetedTransferRole::Sender,
            state: TargetedTransferState::Preparing,
            created_at: now,
            updated_at: now,
        };
        store.insert(&row).await?;
        store
            .set_state(
                &transfer_uuid,
                TargetedTransferState::Preparing,
                TargetedTransferState::Offering,
            )
            .await?;

        let addr = self
            .device_relationships
            .peer_addr(&receiver_endpoint_id)
            .await?;
        let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
        let challenge =
            match tokio::time::timeout(self.connection_timeout(), client.request_challenge()).await
            {
                Ok(Ok(challenge)) => challenge,
                Ok(Err(error)) => {
                    let _ = store
                        .set_state(
                            &transfer_uuid,
                            TargetedTransferState::Offering,
                            TargetedTransferState::Failed,
                        )
                        .await;
                    let _ = self.cancel_idle_or_share(protocol_transfer_id).await;
                    return Err(map_connect_failure(error));
                }
                Err(_) => {
                    let _ = store
                        .set_state(
                            &transfer_uuid,
                            TargetedTransferState::Offering,
                            TargetedTransferState::Failed,
                        )
                        .await;
                    let _ = self.cancel_idle_or_share(protocol_transfer_id).await;
                    return Err(VnidropError::device_unavailable(anyhow::anyhow!(
                        "device did not answer in time"
                    )));
                }
            };

        let (proof, generation, relationship_protocol_version) = self
            .device_relationships
            .prove_saved_possession(&receiver_endpoint_id, &challenge)
            .await?;

        let protocol_version =
            experimental_saved_device_capabilities().targeted_transfer_protocol_version;
        store
            .set_state(
                &transfer_uuid,
                TargetedTransferState::Offering,
                TargetedTransferState::AwaitingApproval,
            )
            .await?;

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
                manifest_id: share.hash.clone(),
                content_hash: share.hash.clone(),
                transfer_name: share.transfer_name.clone(),
                file_count: share.file_count,
                total_size: share.total_size,
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
                let _ = store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Failed,
                    )
                    .await;
                let _ = self.cancel_idle_or_share(protocol_transfer_id).await;
                return Err(map_connect_failure(error));
            }
            Err(_) => {
                let _ = store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Failed,
                    )
                    .await;
                let _ = self.cancel_idle_or_share(protocol_transfer_id).await;
                return Err(VnidropError::offer_timeout(anyhow::anyhow!(
                    "offer timed out"
                )));
            }
        };

        match response {
            TargetedOfferResponse::Accepted => {}
            TargetedOfferResponse::Declined { reason } => {
                let _ = store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Declined,
                    )
                    .await;
                let _ = self.cancel_idle_or_share(protocol_transfer_id).await;
                return Err(VnidropError::permission(anyhow::anyhow!(
                    "targeted offer declined: {reason}"
                )));
            }
            TargetedOfferResponse::Refused { reason } => {
                let _ = store
                    .set_state(
                        &transfer_uuid,
                        TargetedTransferState::AwaitingApproval,
                        TargetedTransferState::Failed,
                    )
                    .await;
                let _ = self.cancel_idle_or_share(protocol_transfer_id).await;
                return Err(map_offer_refuse_reason(&reason));
            }
        }

        // Permanent until cancel/delete — approved targeted transfers must resume.
        self.access_policy
            .approve_endpoint_until(protocol_transfer_id, receiver_endpoint_id.clone(), None)
            .await;

        let parsed = crate::ticket::parse_transfer_ticket_with_limits(&share.ticket, &self.limits)
            .map_err(VnidropError::ticket)?;
        let blob_ticket = BlobTicket::new(
            parsed.blob_ticket.addr().clone(),
            parsed.blob_ticket.hash(),
            BlobFormat::HashSeq,
        );
        let authorization = TargetedAuthorization::issue(TargetedAuthorizationDraft {
            transfer_id: transfer_uuid.clone(),
            protocol_transfer_id,
            sender_endpoint_id,
            receiver_endpoint_id,
            manifest_id: share.hash.clone(),
            content_hash: share.hash.clone(),
            file_count: share.file_count,
            total_size: share.total_size,
            protocol_version,
            transfer_name: share.transfer_name.clone(),
            blob_ticket: blob_ticket.to_string(),
        })?;
        self.persist_authorization_secret(&transfer_uuid, &authorization)
            .await?;
        let encoded = authorization.encode()?;

        let deliver = client
            .deliver_authorization(DeliverTargetedAuthorization {
                transfer_id: transfer_uuid.clone(),
                authorization: encoded,
            })
            .await
            .context("failed to deliver targeted authorization")
            .map_err(VnidropError::network)?;
        if deliver != crate::targeted_transfer::protocol::DeliverAuthorizationResponse::Stored {
            let _ = store
                .set_state(
                    &transfer_uuid,
                    TargetedTransferState::AwaitingApproval,
                    TargetedTransferState::Failed,
                )
                .await;
            return Err(VnidropError::network(anyhow::anyhow!(
                "receiver rejected authorization delivery"
            )));
        }

        store
            .set_state(
                &transfer_uuid,
                TargetedTransferState::AwaitingApproval,
                TargetedTransferState::Approved,
            )
            .await?;

        store
            .get(&transfer_uuid)
            .await?
            .ok_or_else(|| VnidropError::internal(anyhow::anyhow!("targeted transfer missing")))
    }

    pub(super) async fn receive_targeted_transfer(
        self: &Arc<Self>,
        authorization: String,
        output_dir: String,
    ) -> Result<(), VnidropError> {
        let auth = TargetedAuthorization::decode(&authorization)?;
        auth.verify_for_receiver(&self.endpoint.id().to_string())?;
        self.run_targeted_receive(&auth, output_dir).await
    }

    pub(super) async fn resume_targeted_transfer(
        self: &Arc<Self>,
        id: String,
        output_dir: String,
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
        let encoded = self.load_stored_authorization(&row).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!(
                "targeted transfer has no durable authorization"
            ))
        })?;
        let auth = TargetedAuthorization::decode(&encoded)?;
        auth.verify_for_receiver(&self.endpoint.id().to_string())?;
        self.run_targeted_receive(&auth, output_dir).await
    }

    async fn run_targeted_receive(
        self: &Arc<Self>,
        auth: &TargetedAuthorization,
        output_dir: String,
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
                    store
                        .set_state(
                            &auth.transfer_id,
                            TargetedTransferState::Connecting,
                            TargetedTransferState::Transferring,
                        )
                        .await?;
                }
                TargetedTransferState::Connecting => {
                    store
                        .set_state(
                            &auth.transfer_id,
                            TargetedTransferState::Connecting,
                            TargetedTransferState::Transferring,
                        )
                        .await?;
                }
                TargetedTransferState::Transferring => {}
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
        let metadata = TransferMetadata::new(
            auth.protocol_transfer_id,
            non_empty(auth.transfer_name.clone()).unwrap_or_else(|| "transfer".to_string()),
            None,
            blob_ticket.hash(),
            auth.file_count,
            auth.total_size,
        );
        let ticket =
            VnidropTicket::new_with_relay_urls(blob_ticket, metadata, &self.custom_relay_urls)
                .encode()
                .map_err(VnidropError::ticket)?;

        let receive_result = self
            .receive(ticket, std::path::PathBuf::from(output_dir), None)
            .await;

        match receive_result {
            Ok(()) => {
                if let Ok(Some(row)) = store.get_row(&auth.transfer_id).await {
                    let _ = store
                        .set_verified_bytes(&auth.transfer_id, row.total_size)
                        .await;
                    let _ = store
                        .set_state(
                            &auth.transfer_id,
                            TargetedTransferState::Transferring,
                            TargetedTransferState::Completed,
                        )
                        .await;
                }
                Ok(())
            }
            Err(error) => {
                if let Ok(Some(row)) = store.get_row(&auth.transfer_id).await {
                    if matches!(
                        row.state,
                        TargetedTransferState::Connecting | TargetedTransferState::Transferring
                    ) {
                        let _ = store
                            .set_state_from_any(
                                &auth.transfer_id,
                                TargetedTransferState::Interrupted,
                            )
                            .await;
                    }
                }
                Err(VnidropError::transfer(error))
            }
        }
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
        self.targeted_store()
            .store_authorization(transfer_id, &authorization.blob_ticket, handle.as_str())
            .await
    }

    async fn persist_receiver_authorization(&self, encoded: &str) -> Result<(), VnidropError> {
        let auth = TargetedAuthorization::decode(encoded)?;
        let store = self.targeted_store();
        if store.get_row(&auth.transfer_id).await?.is_none() {
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

    async fn load_stored_authorization(
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
