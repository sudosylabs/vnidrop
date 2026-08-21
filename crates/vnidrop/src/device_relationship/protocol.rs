//! Iroh ALPN handler and client for mutual-consent pairing.
//!
//! Wire messages and transport live here; durable state and grant custody stay on
//! [`super::service::DeviceRelationshipService`].

use std::{fmt, sync::Arc};

use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr,
};
use irpc::{channel::oneshot, rpc_requests, Client, WithChannels};
use irpc_iroh::{read_request, IrohLazyRemoteConnection};
use serde::{Deserialize, Serialize};

use super::DeviceRelationshipService;

#[derive(Clone)]
pub(crate) struct RelationshipProtocol {
    relationships: Arc<DeviceRelationshipService>,
}

impl RelationshipProtocol {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/relationship/1";

    pub(crate) fn new(relationships: Arc<DeviceRelationshipService>) -> Self {
        Self { relationships }
    }
}

impl fmt::Debug for RelationshipProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RelationshipProtocol")
    }
}

impl ProtocolHandler for RelationshipProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        while let Some(message) = read_request::<RelationshipMessages>(&connection).await? {
            match message {
                RelationshipMessage::PairingRequest(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .relationships
                        .handle_pairing_request(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                RelationshipMessage::PairingConsent(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .relationships
                        .handle_pairing_consent(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                RelationshipMessage::PairingAck(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .relationships
                        .handle_pairing_ack(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                RelationshipMessage::RevokeNotice(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let acknowledged = self
                        .relationships
                        .handle_remote_revoke(remote_endpoint_id.clone(), inner.generation)
                        .await;
                    let response = if acknowledged {
                        RevokeNoticeResponse::Acknowledged
                    } else {
                        RevokeNoticeResponse::Rejected
                    };
                    let _ = tx.send(response).await;
                }
            }
        }
        connection.closed().await;
        Ok(())
    }
}

pub(super) struct RelationshipClient {
    inner: Client<RelationshipMessages>,
}

impl RelationshipClient {
    pub(super) fn connect(endpoint: Endpoint, addr: EndpointAddr) -> Self {
        Self {
            inner: Client::boxed(IrohLazyRemoteConnection::new(
                endpoint,
                addr,
                RelationshipProtocol::ALPN.to_vec(),
            )),
        }
    }

    pub(super) async fn pairing_request(
        &self,
        request: PairingRequest,
    ) -> Result<PairingRequestResponse, irpc::Error> {
        self.inner.rpc(request).await
    }

    pub(super) async fn pairing_consent(
        &self,
        consent: PairingConsent,
    ) -> Result<PairingConsentResponse, irpc::Error> {
        self.inner.rpc(consent).await
    }

    pub(super) async fn pairing_ack(
        &self,
        ack: PairingAck,
    ) -> Result<PairingAckResponse, irpc::Error> {
        self.inner.rpc(ack).await
    }

    pub(super) async fn revoke_notice(
        &self,
        notice: RevokeNotice,
    ) -> Result<RevokeNoticeResponse, irpc::Error> {
        self.inner.rpc(notice).await
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PairingRequest {
    pub(super) session_id: String,
    pub(super) capability: Vec<u8>,
    pub(super) protocol_version: u16,
    pub(super) generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum PairingRequestResponse {
    AwaitingConsent { generation: u64 },
    Merged { generation: u64 },
    AlreadySaved,
    Rejected,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PairingConsent {
    pub(super) accepted: bool,
    pub(super) grant: Option<WireGrant>,
    pub(super) challenge: Option<String>,
    pub(super) generation: u64,
    pub(super) protocol_version: u16,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum PairingConsentResponse {
    Completed {
        grant: Box<WireGrant>,
        possession_proof: WireProof,
        ack_challenge: String,
    },
    AlreadySaved,
    Rejected,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PairingAck {
    pub(super) possession_proof: WireProof,
    pub(super) challenge: String,
    pub(super) generation: u64,
    pub(super) protocol_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum PairingAckResponse {
    Acknowledged,
    AlreadySaved,
    Rejected,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WireGrant {
    pub(super) grant_id: String,
    pub(super) secret: String,
    pub(super) issuer_endpoint_id: String,
    pub(super) holder_endpoint_id: String,
    pub(super) generation: u64,
    pub(super) protocol_version: u16,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WireProof {
    pub(crate) grant_id: String,
    pub(crate) mac: String,
    pub(crate) challenge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RevokeNotice {
    pub(super) generation: u64,
    pub(super) issued_grant_id: Option<String>,
}

macro_rules! redacted_debug {
    ($($type:ty),+ $(,)?) => {
        $(
            impl fmt::Debug for $type {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str(concat!(stringify!($type), "(redacted)"))
                }
            }
        )+
    };
}

redacted_debug!(
    PairingRequest,
    PairingConsent,
    PairingConsentResponse,
    PairingAck,
    WireGrant,
    WireProof,
);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RevokeNoticeResponse {
    Acknowledged,
    Rejected,
}

#[rpc_requests(message = RelationshipMessage)]
#[derive(Debug, Serialize, Deserialize)]
#[allow(
    clippy::enum_variant_names,
    reason = "Pairing* names mirror the wire RPC surface"
)]
enum RelationshipMessages {
    #[rpc(tx = oneshot::Sender<PairingRequestResponse>)]
    PairingRequest(PairingRequest),
    #[rpc(tx = oneshot::Sender<PairingConsentResponse>)]
    PairingConsent(PairingConsent),
    #[rpc(tx = oneshot::Sender<PairingAckResponse>)]
    PairingAck(PairingAck),
    #[rpc(tx = oneshot::Sender<RevokeNoticeResponse>)]
    RevokeNotice(RevokeNotice),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_pairing_wire_debug_is_redacted() {
        let grant = WireGrant {
            grant_id: "grant-id".to_string(),
            secret: "grant-secret".to_string(),
            issuer_endpoint_id: "issuer-endpoint".to_string(),
            holder_endpoint_id: "holder-endpoint".to_string(),
            generation: 1,
            protocol_version: 1,
        };
        let proof = WireProof {
            grant_id: "grant-id".to_string(),
            mac: "proof-mac".to_string(),
            challenge: "proof-challenge".to_string(),
        };

        assert_eq!(
            format!(
                "{:?}",
                PairingRequest {
                    session_id: "session-id".to_string(),
                    capability: b"pairing-capability".to_vec(),
                    protocol_version: 1,
                    generation: 1,
                }
            ),
            "PairingRequest(redacted)"
        );
        assert_eq!(format!("{grant:?}"), "WireGrant(redacted)");
        assert_eq!(format!("{proof:?}"), "WireProof(redacted)");
        assert_eq!(
            format!(
                "{:?}",
                PairingConsent {
                    accepted: true,
                    grant: Some(grant.clone()),
                    challenge: Some("consent-challenge".to_string()),
                    generation: 1,
                    protocol_version: 1,
                }
            ),
            "PairingConsent(redacted)"
        );
        assert_eq!(
            format!(
                "{:?}",
                PairingConsentResponse::Completed {
                    grant: Box::new(grant),
                    possession_proof: proof.clone(),
                    ack_challenge: "ack-challenge".to_string(),
                }
            ),
            "PairingConsentResponse(redacted)"
        );
        assert_eq!(
            format!(
                "{:?}",
                PairingAck {
                    possession_proof: proof,
                    challenge: "ack-challenge".to_string(),
                    generation: 1,
                    protocol_version: 1,
                }
            ),
            "PairingAck(redacted)"
        );
    }
}
