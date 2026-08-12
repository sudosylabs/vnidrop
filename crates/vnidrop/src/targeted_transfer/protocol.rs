//! Targeted-transfer control-plane protocol.
//!
//! Separate ALPN from ordinary offers: pre-approval messages carry a manifest
//! summary and relationship proof only — never a reusable share ticket.

use std::fmt;
use std::{future::Future, pin::Pin};

use anyhow::Result;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, RelayUrl,
};
use irpc::{channel::oneshot, rpc_requests, Client, WithChannels};
use irpc_iroh::{read_request, IrohLazyRemoteConnection};
use serde::{Deserialize, Serialize};

use super::{
    auth::TargetedAuthorization,
    inbox::{TargetedOfferDecision, TargetedOfferInbox},
    state_as_str, TargetedTransferRole, TargetedTransferStore,
};
use crate::{
    api::{
        experimental_saved_device_capabilities, CoreRelayMode, PendingTargetedOffer,
        TargetedTransferState,
    },
    device_relationship::{DeviceRelationshipService, WireProof},
    error::VnidropError,
    grant::Challenge,
    ticket::relay_profiles_compatible,
    util::now_ms,
};

#[derive(Clone)]
pub(crate) struct TargetedTransferProtocol {
    relationships: std::sync::Arc<DeviceRelationshipService>,
    inbox: TargetedOfferInbox,
    store: TargetedTransferStore,
    limits: crate::api::CoreLimits,
    local_endpoint_id: String,
    relay_mode: CoreRelayMode,
    custom_relay_urls: Vec<RelayUrl>,
    event_hub: std::sync::Arc<crate::event_hub::EventHub>,
    access_policy: std::sync::Arc<crate::access_policy::AccessPolicy>,
    cleanup:
        std::sync::Arc<dyn Fn(super::TargetedTransferRow) -> TargetedCleanupFuture + Send + Sync>,
}

pub(crate) type TargetedCleanupFuture =
    Pin<Box<dyn Future<Output = Result<(), VnidropError>> + Send>>;

impl fmt::Debug for TargetedTransferProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TargetedTransferProtocol")
    }
}

impl TargetedTransferProtocol {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/targeted-transfer/3";

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        relationships: std::sync::Arc<DeviceRelationshipService>,
        inbox: TargetedOfferInbox,
        store: TargetedTransferStore,
        limits: crate::api::CoreLimits,
        local_endpoint_id: String,
        relay_mode: CoreRelayMode,
        custom_relay_urls: Vec<RelayUrl>,
        event_hub: std::sync::Arc<crate::event_hub::EventHub>,
        access_policy: std::sync::Arc<crate::access_policy::AccessPolicy>,
        cleanup: std::sync::Arc<
            dyn Fn(super::TargetedTransferRow) -> TargetedCleanupFuture + Send + Sync,
        >,
    ) -> Self {
        Self {
            relationships,
            inbox,
            store,
            limits,
            local_endpoint_id,
            relay_mode,
            custom_relay_urls,
            event_hub,
            access_policy,
            cleanup,
        }
    }

    pub(crate) fn client(endpoint: Endpoint, addr: EndpointAddr) -> TargetedTransferClient {
        TargetedTransferClient {
            inner: Client::boxed(IrohLazyRemoteConnection::new(
                endpoint,
                addr,
                Self::ALPN.to_vec(),
            )),
        }
    }

    async fn handle_offer(
        &self,
        remote_endpoint_id: &str,
        challenge: &Challenge,
        offer: SubmitTargetedOffer,
    ) -> WireOfferResponse {
        let expected = experimental_saved_device_capabilities().targeted_transfer_protocol_version;
        if self.inbox.cooldown().is_cooling(remote_endpoint_id) {
            return WireOfferResponse::Refused {
                reason: "identity-cooldown".to_string(),
            };
        }
        if offer.protocol_version != expected {
            self.inbox.cooldown().record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "protocol-incompatible".to_string(),
            };
        }
        if offer.receiver_endpoint_id != self.local_endpoint_id {
            self.inbox.cooldown().record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "receiver-mismatch".to_string(),
            };
        }
        if offer.sender_endpoint_id != remote_endpoint_id {
            self.inbox.cooldown().record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "sender-mismatch".to_string(),
            };
        }
        if offer.file_count == 0
            || offer.file_count > self.limits.max_collection_files
            || offer.total_size == 0
            || offer.total_size > self.limits.max_total_bytes
            || offer.transfer_id.is_empty()
            || offer.manifest_id.is_empty()
            || offer.content_hash.is_empty()
        {
            self.inbox.cooldown().record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "manifest-limits".to_string(),
            };
        }
        if let Err(error) = self
            .limits
            .validate_metadata_text("transfer name", Some(offer.transfer_name.as_str()))
        {
            self.inbox.cooldown().record_malformed(remote_endpoint_id);
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
            Err(_) => {
                return WireOfferResponse::Refused {
                    reason: "relay-policy-incompatible".to_string(),
                };
            }
        };
        if !relay_profiles_compatible(
            self.relay_mode,
            &self.custom_relay_urls,
            offer.relay_mode,
            &remote_urls,
        ) {
            return WireOfferResponse::Refused {
                reason: "relay-policy-incompatible".to_string(),
            };
        }

        if let Err(error) = self
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
            self.inbox.cooldown().record_malformed(remote_endpoint_id);
            if matches!(error, VnidropError::ProtocolIncompatible { .. }) {
                return WireOfferResponse::Refused {
                    reason: "protocol-incompatible".to_string(),
                };
            }
            return WireOfferResponse::Refused {
                reason: "unauthenticated".to_string(),
            };
        }

        let pending = PendingTargetedOffer {
            transfer_id: offer.transfer_id,
            sender_endpoint_id: remote_endpoint_id.to_string(),
            receiver_endpoint_id: self.local_endpoint_id.clone(),
            manifest_id: offer.manifest_id,
            content_hash: offer.content_hash,
            transfer_name: offer.transfer_name,
            file_count: offer.file_count,
            total_size: offer.total_size,
            protocol_version: offer.protocol_version,
            received_at: now_ms(),
        };

        match self.inbox.submit(pending).await {
            TargetedOfferDecision::Accepted => WireOfferResponse::Accepted,
            TargetedOfferDecision::Declined { reason } => WireOfferResponse::Declined { reason },
            TargetedOfferDecision::Refused { reason } => WireOfferResponse::Refused { reason },
        }
    }

    async fn handle_deliver_authorization(
        &self,
        remote_endpoint_id: &str,
        delivery: DeliverTargetedAuthorization,
    ) -> DeliverAuthorizationResponse {
        let Ok(auth) = TargetedAuthorization::decode(&delivery.authorization) else {
            return DeliverAuthorizationResponse::Rejected;
        };
        if auth.sender_endpoint_id != remote_endpoint_id
            || auth.receiver_endpoint_id != self.local_endpoint_id
            || auth.transfer_id != delivery.transfer_id
        {
            return DeliverAuthorizationResponse::Rejected;
        }
        if let Ok(Some(row)) = self.store.get_row(&delivery.transfer_id).await {
            if row.authorization_secret_handle.is_some()
                && row.protocol_transfer_id == auth.protocol_transfer_id
                && row.sender_endpoint_id == auth.sender_endpoint_id
                && row.receiver_endpoint_id == auth.receiver_endpoint_id
                && row.manifest_id == auth.manifest_id
                && row.content_hash == auth.content_hash
                && row.transfer_name == auth.transfer_name
                && row.file_count == auth.file_count
                && row.total_size == auth.total_size
                && row.blob_ticket.as_deref() == Some(auth.blob_ticket.as_str())
                && auth.protocol_version
                    == experimental_saved_device_capabilities().targeted_transfer_protocol_version
            {
                return DeliverAuthorizationResponse::Stored;
            }
            if row.authorization_secret_handle.is_some() {
                return DeliverAuthorizationResponse::Rejected;
            }
        }
        if !self.inbox.authorization_matches_pending(&auth).await {
            return DeliverAuthorizationResponse::Rejected;
        }
        if self
            .inbox
            .deliver_authorization(&delivery.transfer_id, delivery.authorization)
            .await
        {
            let deadline = tokio::time::Instant::now()
                + std::time::Duration::from_millis(self.limits.offer_timeout_ms);
            loop {
                if let Ok(Some(row)) = self.store.get_row(&delivery.transfer_id).await {
                    if row.authorization_secret_handle.is_some() {
                        return DeliverAuthorizationResponse::Stored;
                    }
                    if matches!(
                        row.state,
                        TargetedTransferState::Failed
                            | TargetedTransferState::Cancelled
                            | TargetedTransferState::Deleted
                    ) {
                        return DeliverAuthorizationResponse::Rejected;
                    }
                }
                if tokio::time::Instant::now() >= deadline {
                    return DeliverAuthorizationResponse::Rejected;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        } else {
            DeliverAuthorizationResponse::Rejected
        }
    }

    async fn handle_cancel(
        &self,
        remote_endpoint_id: &str,
        cancel: CancelTargetedOffer,
    ) -> CancelWireOfferResponse {
        if let Some(pending) = self.inbox.get_pending(&cancel.transfer_id).await {
            if pending.sender_endpoint_id != remote_endpoint_id {
                return CancelWireOfferResponse::Rejected;
            }
            self.inbox.discard(&cancel.transfer_id).await;
            return CancelWireOfferResponse::Cancelled;
        }
        if let Ok(Some(row)) = self.store.get_row(&cancel.transfer_id).await {
            let remote_is_peer = match row.role {
                TargetedTransferRole::Sender => row.receiver_endpoint_id == remote_endpoint_id,
                TargetedTransferRole::Receiver => row.sender_endpoint_id == remote_endpoint_id,
            };
            if !remote_is_peer {
                return CancelWireOfferResponse::Rejected;
            }
            if let Some(terminal) = cancel.terminal {
                if matches!(
                    terminal,
                    TargetedTransferState::Cancelled | TargetedTransferState::Deleted
                ) {
                    if row.role == TargetedTransferRole::Sender {
                        self.access_policy
                            .remove_transfer(row.protocol_transfer_id)
                            .await;
                    }
                    let changed = if row.state == TargetedTransferState::Cancelled {
                        false
                    } else if matches!(
                        row.state,
                        TargetedTransferState::Completed
                            | TargetedTransferState::Declined
                            | TargetedTransferState::Failed
                            | TargetedTransferState::Deleted
                    ) {
                        return CancelWireOfferResponse::Cancelled;
                    } else {
                        let Ok(changed) = self
                            .store
                            .transition_terminal(
                                &cancel.transfer_id,
                                TargetedTransferState::Cancelled,
                                false,
                            )
                            .await
                        else {
                            return CancelWireOfferResponse::Rejected;
                        };
                        changed
                    };
                    if changed {
                        self.event_hub.emit_endpoint(
                            "targeted_transfer",
                            "cancelled",
                            serde_json::json!({ "targeted_transfer_id": cancel.transfer_id }),
                        );
                    }
                    if (self.cleanup)(row.clone()).await.is_err() {
                        return CancelWireOfferResponse::Rejected;
                    }
                }
            }
            return CancelWireOfferResponse::Cancelled;
        }
        CancelWireOfferResponse::Cancelled
    }

    async fn handle_completion(
        &self,
        remote_endpoint_id: &str,
        completion: CompleteTargetedTransfer,
    ) -> CompletionResponse {
        if self
            .relationships
            .require_saved(remote_endpoint_id)
            .await
            .is_err()
        {
            return CompletionResponse::Rejected;
        }
        let Ok(Some(row)) = self.store.get_row(&completion.transfer_id).await else {
            return CompletionResponse::Rejected;
        };
        let Ok(auth) = TargetedAuthorization::decode(&completion.authorization) else {
            return CompletionResponse::Rejected;
        };
        if row.role != TargetedTransferRole::Sender
            || row.receiver_endpoint_id != remote_endpoint_id
            || auth.receiver_endpoint_id != remote_endpoint_id
            || auth.sender_endpoint_id != self.local_endpoint_id
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
                != experimental_saved_device_capabilities().targeted_transfer_protocol_version
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
        match self
            .store
            .mark_sender_completed(&completion.transfer_id)
            .await
        {
            Ok(changed) => {
                if changed {
                    self.event_hub.emit_endpoint(
                        "targeted_transfer",
                        "completed",
                        serde_json::json!({ "targeted_transfer_id": completion.transfer_id }),
                    );
                }
                CompletionResponse::Recorded
            }
            Err(_) => CompletionResponse::Rejected,
        }
    }
}

impl ProtocolHandler for TargetedTransferProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        let challenge = Challenge::generate();
        while let Some(message) = read_request::<TargetedTransferMessages>(&connection).await? {
            match message {
                TargetedTransferMessage::RequestChallenge(message) => {
                    let WithChannels { tx, .. } = message;
                    let _ = tx
                        .send(ChallengeResponse {
                            challenge: challenge.clone(),
                        })
                        .await;
                }
                TargetedTransferMessage::SubmitTargetedOffer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .handle_offer(&remote_endpoint_id, &challenge, inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                TargetedTransferMessage::DeliverTargetedAuthorization(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .handle_deliver_authorization(&remote_endpoint_id, inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                TargetedTransferMessage::CancelTargetedOffer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self.handle_cancel(&remote_endpoint_id, inner).await;
                    let _ = tx.send(response).await;
                }
                TargetedTransferMessage::CompleteTargetedTransfer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self.handle_completion(&remote_endpoint_id, inner).await;
                    let _ = tx.send(response).await;
                }
            }
        }
        connection.closed().await;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TargetedTransferClient {
    inner: Client<TargetedTransferMessages>,
}

impl TargetedTransferClient {
    pub(crate) async fn request_challenge(&self) -> Result<Challenge, irpc::Error> {
        Ok(self.inner.rpc(RequestChallenge).await?.challenge)
    }

    pub(crate) async fn submit_offer(
        &self,
        offer: SubmitTargetedOffer,
    ) -> Result<WireOfferResponse, irpc::Error> {
        self.inner.rpc(offer).await
    }

    pub(crate) async fn deliver_authorization(
        &self,
        delivery: DeliverTargetedAuthorization,
    ) -> Result<DeliverAuthorizationResponse, irpc::Error> {
        self.inner.rpc(delivery).await
    }

    pub(crate) async fn cancel_offer(
        &self,
        cancel: CancelTargetedOffer,
    ) -> Result<CancelWireOfferResponse, irpc::Error> {
        self.inner.rpc(cancel).await
    }

    pub(crate) async fn complete_transfer(
        &self,
        completion: CompleteTargetedTransfer,
    ) -> Result<CompletionResponse, irpc::Error> {
        self.inner.rpc(completion).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RequestChallenge;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChallengeResponse {
    challenge: Challenge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SubmitTargetedOffer {
    pub(crate) proof: WireProof,
    pub(crate) generation: u64,
    pub(crate) relationship_protocol_version: u16,
    pub(crate) protocol_version: u16,
    pub(crate) transfer_id: String,
    pub(crate) sender_endpoint_id: String,
    pub(crate) receiver_endpoint_id: String,
    pub(crate) manifest_id: String,
    pub(crate) content_hash: String,
    pub(crate) transfer_name: String,
    pub(crate) file_count: u64,
    pub(crate) total_size: u64,
    pub(crate) relay_mode: CoreRelayMode,
    pub(crate) relay_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum WireOfferResponse {
    Accepted,
    Declined { reason: String },
    Refused { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeliverTargetedAuthorization {
    pub(crate) transfer_id: String,
    pub(crate) authorization: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum DeliverAuthorizationResponse {
    Stored,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CancelTargetedOffer {
    pub(crate) transfer_id: String,
    pub(crate) terminal: Option<TargetedTransferState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum CancelWireOfferResponse {
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompleteTargetedTransfer {
    pub(crate) transfer_id: String,
    pub(crate) verified_bytes: u64,
    pub(crate) authorization: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum CompletionResponse {
    Recorded,
    Rejected,
}

#[rpc_requests(message = TargetedTransferMessage)]
#[derive(Debug, Serialize, Deserialize)]
#[allow(
    clippy::large_enum_variant,
    reason = "offer payload carries relay profile + manifest summary; boxing breaks irpc channels"
)]
enum TargetedTransferMessages {
    #[rpc(tx = oneshot::Sender<ChallengeResponse>)]
    RequestChallenge(RequestChallenge),
    #[rpc(tx = oneshot::Sender<WireOfferResponse>)]
    SubmitTargetedOffer(SubmitTargetedOffer),
    #[rpc(tx = oneshot::Sender<DeliverAuthorizationResponse>)]
    DeliverTargetedAuthorization(DeliverTargetedAuthorization),
    #[rpc(tx = oneshot::Sender<CancelWireOfferResponse>)]
    CancelTargetedOffer(CancelTargetedOffer),
    #[rpc(tx = oneshot::Sender<CompletionResponse>)]
    CompleteTargetedTransfer(CompleteTargetedTransfer),
}

fn parse_offer_relay_urls(values: &[String]) -> Result<Vec<RelayUrl>, ()> {
    let mut urls = Vec::with_capacity(values.len());
    for value in values {
        let Ok(url) = value.parse::<RelayUrl>() else {
            return Err(());
        };
        urls.push(url);
    }
    Ok(urls)
}

/// Map a receiver refuse reason to a typed public error.
pub(crate) fn map_offer_refuse_reason(reason: &str) -> VnidropError {
    match reason {
        "relay-policy-incompatible" => VnidropError::relay_policy_incompatible(anyhow::anyhow!(
            "sender and receiver network profiles are incompatible"
        )),
        "protocol-incompatible" => VnidropError::protocol_incompatible(anyhow::anyhow!(
            "targeted-transfer protocol is incompatible"
        )),
        other => VnidropError::permission(anyhow::anyhow!("targeted offer refused: {other}")),
    }
}
