//! Sender-side targeted-transfer creation and offer approval.

use std::sync::Arc;

use iroh_blobs::{ticket::BlobTicket, BlobFormat};
use uuid::Uuid;

use super::{
    targeted::{allocate_protocol_transfer_id, map_connect_failure},
    targeted_tag_name, CoreInner,
};
use crate::{
    api::{
        saved_device_capabilities, ShareSource, TargetedTransfer, TargetedTransferState,
        TransferAccessMode,
    },
    error::VnidropError,
    targeted_transfer::{
        protocol::{
            map_offer_refuse_reason, SubmitTargetedOffer, TargetedTransferProtocol,
            WireOfferResponse,
        },
        TargetedAuthorization, TargetedAuthorizationDraft, TargetedTransferRole,
        TargetedTransferRow,
    },
    util::{non_empty, now_ms},
};

impl CoreInner {
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

        let protocol_version = saved_device_capabilities().targeted_transfer_protocol_version;
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
            .persist_sender_authorization_and_approve(&authorization)
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
        self.emit_targeted_lifecycle(&transfer_uuid, "approved");

        store
            .get(&transfer_uuid)
            .await?
            .ok_or_else(|| VnidropError::internal(anyhow::anyhow!("targeted transfer missing")))
    }
}
