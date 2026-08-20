//! Targeted-transfer wire translation.
//!
//! This adapter owns framing and serialization only. Durable decisions live in
//! [`super::TargetedTransferModule`].

use std::fmt;

use anyhow::Result;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, RelayUrl,
};
use irpc::{channel::oneshot, rpc_requests, Client, WithChannels};
use irpc_iroh::{read_request, IrohLazyRemoteConnection};
use serde::{Deserialize, Serialize};

use super::TargetedTransferModule;
use crate::{
    api::{CoreRelayMode, TargetedTransferState},
    device_relationship::WireProof,
    error::VnidropError,
    grant::Challenge,
};

#[derive(Clone)]
pub(crate) struct TargetedTransferProtocol {
    transfers: TargetedTransferModule,
}

impl fmt::Debug for TargetedTransferProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TargetedTransferProtocol")
    }
}

impl TargetedTransferProtocol {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/targeted-transfer/3";

    pub(crate) fn new(transfers: TargetedTransferModule) -> Self {
        Self { transfers }
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
                        .transfers
                        .handle_offer(&remote_endpoint_id, &challenge, inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                TargetedTransferMessage::DeliverTargetedAuthorization(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .transfers
                        .handle_authorization(&remote_endpoint_id, inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                TargetedTransferMessage::CancelTargetedOffer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .transfers
                        .handle_cancel(&remote_endpoint_id, inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                TargetedTransferMessage::CompleteTargetedTransfer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .transfers
                        .handle_completion(&remote_endpoint_id, inner)
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

pub(crate) fn parse_offer_relay_urls(values: &[String]) -> Result<Vec<RelayUrl>, ()> {
    values
        .iter()
        .map(|value| value.parse::<RelayUrl>().map_err(|_| ()))
        .collect()
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
