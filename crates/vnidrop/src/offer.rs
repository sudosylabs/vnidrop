//! The contacts protocol: how paired devices reach each other directly.
//!
//! Separate ALPN from the transfer handshake because the trust model differs.
//! `/vnidrop/handshake/2` serves anyone holding a ticket, subject to sender
//! approval. This one serves nobody without a grant (see [`crate::grant`]), so
//! an unpaired device cannot even raise a prompt on the far side.
//!
//! Every request except grant delivery carries a proof over a challenge this
//! connection issued, so a captured proof cannot be replayed onto another
//! connection.

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

use crate::{
    grant::{Challenge, GrantId},
    pairing::PairingService,
};

#[derive(Clone)]
pub(crate) struct OfferService {
    pairing: PairingService,
}

impl fmt::Debug for OfferService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OfferService")
    }
}

impl OfferService {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/offer/1";

    pub(crate) fn new(pairing: PairingService) -> Self {
        Self { pairing }
    }

    pub(crate) fn client(endpoint: Endpoint, addr: EndpointAddr) -> OfferClient {
        OfferClient {
            inner: Client::boxed(IrohLazyRemoteConnection::new(
                endpoint,
                addr,
                Self::ALPN.to_vec(),
            )),
        }
    }
}

impl ProtocolHandler for OfferService {
    /// Accepts inbound connections from paired peers.
    ///
    /// The challenge is per connection and never leaves this scope, which is
    /// what binds a proof to one session: a proof captured from an earlier
    /// connection cannot be presented on a later one.
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        let challenge = Challenge::generate();

        while let Some(message) = read_request::<OfferProtocol>(&connection).await? {
            match message {
                OfferMessage::RequestChallenge(message) => {
                    let WithChannels { tx, .. } = message;
                    let _ = tx
                        .send(ChallengeResponse {
                            challenge: challenge.clone(),
                        })
                        .await;
                }
                OfferMessage::DeliverGrant(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .pairing
                        .receive_grant(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                OfferMessage::RevokeGrant(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .pairing
                        .receive_revocation(remote_endpoint_id.clone(), inner)
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
pub(crate) struct OfferClient {
    inner: Client<OfferProtocol>,
}

impl OfferClient {
    pub(crate) async fn deliver_grant(
        &self,
        grant: DeliverGrant,
    ) -> Result<GrantDeliveryResponse, irpc::Error> {
        self.inner.rpc(grant).await
    }

    pub(crate) async fn revoke_grant(
        &self,
        revocation: RevokeGrant,
    ) -> Result<RevocationResponse, irpc::Error> {
        self.inner.rpc(revocation).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestChallenge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChallengeResponse {
    pub(crate) challenge: Challenge,
}

/// Hand a peer the capability to reach this device.
///
/// Carries the secret itself, which is safe only because the iroh connection is
/// already authenticated and encrypted to the recipient's endpoint key. The
/// recipient still has to consent before it is stored.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct DeliverGrant {
    pub(crate) grant_id: GrantId,
    /// Hex-encoded grant secret.
    pub(crate) secret: String,
    pub(crate) expires_at: Option<i64>,
    /// Untrusted display data, shown only after the user consents.
    pub(crate) display_name: Option<String>,
}

// The secret must not reach a log line through a derived Debug.
impl fmt::Debug for DeliverGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeliverGrant")
            .field("grant_id", &self.grant_id)
            .field("display_name", &self.display_name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum GrantDeliveryResponse {
    /// Held pending the local user's decision. Not yet a contact.
    AwaitingConsent,
    /// Stored: the local user had already agreed to remember this device.
    Stored,
    Rejected {
        reason: String,
    },
}

/// Tell a peer that a grant it holds is dead, so its entry disappears promptly
/// rather than at its next attempt. Best effort: revocation is already complete
/// on the issuing side before this is sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RevokeGrant {
    pub(crate) grant_id: GrantId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RevocationResponse {
    Removed,
    /// No such grant held from this peer. Also returned when the grant belongs
    /// to someone else, so a stranger cannot probe for grant ids.
    Unknown,
}

#[rpc_requests(message = OfferMessage)]
#[derive(Debug, Serialize, Deserialize)]
enum OfferProtocol {
    #[rpc(tx=oneshot::Sender<ChallengeResponse>)]
    RequestChallenge(RequestChallenge),
    #[rpc(tx=oneshot::Sender<GrantDeliveryResponse>)]
    DeliverGrant(DeliverGrant),
    #[rpc(tx=oneshot::Sender<RevocationResponse>)]
    RevokeGrant(RevokeGrant),
}
