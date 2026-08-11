//! Targeted-transfer control-plane protocol (design §10).
//!
//! Separate ALPN from ordinary offers: pre-approval messages carry a manifest
//! summary and relationship proof only — never a reusable share ticket.

use std::fmt;

use anyhow::Result;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr,
};
use irpc::{channel::oneshot, rpc_requests, Client, WithChannels};
use irpc_iroh::{read_request, IrohLazyRemoteConnection};
use serde::{Deserialize, Serialize};

use super::{
    auth::TargetedAuthorization,
    inbox::{TargetedOfferDecision, TargetedOfferInbox},
};
use crate::{
    api::{experimental_saved_device_capabilities, PendingTargetedOffer},
    device_relationship::{DeviceRelationshipService, WireProof},
    error::VnidropError,
    grant::Challenge,
    util::now_ms,
};

#[derive(Clone)]
pub(crate) struct TargetedTransferProtocol {
    relationships: std::sync::Arc<DeviceRelationshipService>,
    inbox: TargetedOfferInbox,
    limits: crate::api::CoreLimits,
    local_endpoint_id: String,
}

impl fmt::Debug for TargetedTransferProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TargetedTransferProtocol")
    }
}

impl TargetedTransferProtocol {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/targeted-transfer/1";

    pub(crate) fn new(
        relationships: std::sync::Arc<DeviceRelationshipService>,
        inbox: TargetedOfferInbox,
        limits: crate::api::CoreLimits,
        local_endpoint_id: String,
    ) -> Self {
        Self {
            relationships,
            inbox,
            limits,
            local_endpoint_id,
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
    ) -> TargetedOfferResponse {
        let expected = experimental_saved_device_capabilities().targeted_transfer_protocol_version;
        if offer.protocol_version != expected {
            return TargetedOfferResponse::Refused {
                reason: "protocol-incompatible".to_string(),
            };
        }
        if offer.receiver_endpoint_id != self.local_endpoint_id {
            return TargetedOfferResponse::Refused {
                reason: "receiver-mismatch".to_string(),
            };
        }
        if offer.sender_endpoint_id != remote_endpoint_id {
            return TargetedOfferResponse::Refused {
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
            return TargetedOfferResponse::Refused {
                reason: "manifest-limits".to_string(),
            };
        }
        if let Err(error) = self
            .limits
            .validate_metadata_text("transfer name", Some(offer.transfer_name.as_str()))
        {
            return TargetedOfferResponse::Refused {
                reason: error.to_string(),
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
            tracing::debug!(%error, "targeted offer relationship proof rejected");
            return TargetedOfferResponse::Refused {
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
            TargetedOfferDecision::Accepted => TargetedOfferResponse::Accepted,
            TargetedOfferDecision::Declined { reason } => {
                TargetedOfferResponse::Declined { reason }
            }
            TargetedOfferDecision::Refused { reason } => TargetedOfferResponse::Refused { reason },
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
        if self
            .inbox
            .deliver_authorization(&delivery.transfer_id, delivery.authorization)
            .await
        {
            DeliverAuthorizationResponse::Stored
        } else {
            DeliverAuthorizationResponse::Rejected
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
    ) -> Result<TargetedOfferResponse, irpc::Error> {
        self.inner.rpc(offer).await
    }

    pub(crate) async fn deliver_authorization(
        &self,
        delivery: DeliverTargetedAuthorization,
    ) -> Result<DeliverAuthorizationResponse, irpc::Error> {
        self.inner.rpc(delivery).await
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum TargetedOfferResponse {
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

#[rpc_requests(message = TargetedTransferMessage)]
#[derive(Debug, Serialize, Deserialize)]
enum TargetedTransferMessages {
    #[rpc(tx = oneshot::Sender<ChallengeResponse>)]
    RequestChallenge(RequestChallenge),
    #[rpc(tx = oneshot::Sender<TargetedOfferResponse>)]
    SubmitTargetedOffer(SubmitTargetedOffer),
    #[rpc(tx = oneshot::Sender<DeliverAuthorizationResponse>)]
    DeliverTargetedAuthorization(DeliverTargetedAuthorization),
}

/// Helper kept for type visibility in callers that map refuse reasons.
#[allow(dead_code)]
pub(crate) fn map_offer_error(reason: &str) -> VnidropError {
    VnidropError::permission(anyhow::anyhow!("targeted offer refused: {reason}"))
}
