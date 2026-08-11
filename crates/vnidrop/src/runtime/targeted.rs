//! Create, offer, approve, and receive targeted transfers between Saved devices.

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
    targeted_transfer::{
        protocol::{
            DeliverTargetedAuthorization, SubmitTargetedOffer, TargetedOfferResponse,
            TargetedTransferProtocol,
        },
        TargetedAuthorization, TargetedAuthorizationDraft, TargetedTransferRow,
        TargetedTransferStore,
    },
    ticket::VnidropTicket,
    util::{non_empty, now_ms},
};

const OFFER_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

impl CoreInner {
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
        self.targeted_store().cancel_by_peer(peer_endpoint_id).await
    }

    pub(super) async fn respond_to_targeted_offer(
        &self,
        transfer_id: String,
        accepted: bool,
    ) -> Result<Option<String>, VnidropError> {
        match self.targeted_offers.respond(&transfer_id, accepted).await {
            Ok(auth) => Ok(auth),
            Err(crate::targeted_transfer::RespondError::Unknown) => Err(
                VnidropError::invalid_input(anyhow::anyhow!("unknown targeted offer")),
            ),
            Err(crate::targeted_transfer::RespondError::SenderGone) => Err(VnidropError::network(
                anyhow::anyhow!("sender disconnected before approval completed"),
            )),
            Err(crate::targeted_transfer::RespondError::AuthorizationTimeout) => Err(
                VnidropError::network(anyhow::anyhow!("authorization was not delivered in time")),
            ),
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
        let challenge = tokio::time::timeout(OFFER_CONNECT_TIMEOUT, client.request_challenge())
            .await
            .map_err(|_| VnidropError::network(anyhow::anyhow!("device did not answer in time")))?
            .context("device is not reachable")
            .map_err(VnidropError::network)?;

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

        let response = tokio::time::timeout(
            OFFER_CONNECT_TIMEOUT + std::time::Duration::from_secs(120),
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
            }),
        )
        .await
        .map_err(|_| VnidropError::network(anyhow::anyhow!("offer timed out")))?
        .context("failed to submit targeted offer")
        .map_err(VnidropError::network)?;

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
                return Err(VnidropError::permission(anyhow::anyhow!(
                    "targeted offer refused: {reason}"
                )));
            }
        }

        // Bound authorization: only the approved receiver endpoint may fetch.
        self.access_policy
            .approve_endpoint(protocol_transfer_id, receiver_endpoint_id.clone())
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

        self.receive(ticket, std::path::PathBuf::from(output_dir), None)
            .await
            .map_err(VnidropError::transfer)?;

        if let Ok(Some(row)) = self.targeted_store().get_row(&auth.transfer_id).await {
            let _ = self
                .targeted_store()
                .set_state(
                    &auth.transfer_id,
                    row.state,
                    TargetedTransferState::Completed,
                )
                .await;
        }
        Ok(())
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

trait BlobTicketParse {
    fn from_str_compat(value: &str) -> Result<BlobTicket, String>;
}

impl BlobTicketParse for BlobTicket {
    fn from_str_compat(value: &str) -> Result<BlobTicket, String> {
        use std::str::FromStr;
        BlobTicket::from_str(value).map_err(|error| error.to_string())
    }
}
