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
    grant::{Challenge, GrantId, GrantProof},
    offer_inbox::OfferInbox,
    pairing::PairingService,
};

#[derive(Clone)]
pub(crate) struct OfferService {
    pairing: PairingService,
    inbox: OfferInbox,
    /// This device's endpoint id. Grants we issued are bound to it, so proofs
    /// must be verified against it rather than against whatever a peer claims.
    self_endpoint_id: String,
}

impl fmt::Debug for OfferService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OfferService")
    }
}

impl OfferService {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/offer/1";

    pub(crate) fn new(
        pairing: PairingService,
        inbox: OfferInbox,
        self_endpoint_id: String,
    ) -> Self {
        Self {
            pairing,
            inbox,
            self_endpoint_id,
        }
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

impl OfferService {
    /// Validate the grant, then hand the offer to the local user.
    ///
    /// A refusal names the grant failure so the peer can drop a dead entry;
    /// `Unknown` covers both "never issued" and "blocked", which is what keeps
    /// blocking undetectable.
    async fn handle_offer(
        &self,
        remote_endpoint_id: &str,
        challenge: &Challenge,
        offer: SubmitOffer,
    ) -> OfferResponse {
        if let Err(rejection) = self
            .pairing
            .verify_and_renew(
                &offer.proof,
                challenge,
                &self.self_endpoint_id,
                remote_endpoint_id,
            )
            .await
        {
            return OfferResponse::Refused {
                reason: rejection.as_str().to_string(),
            };
        }

        self.inbox
            .submit(
                remote_endpoint_id.to_string(),
                offer.transfer_name,
                offer.sender_display_name,
                offer.file_count,
                offer.total_bytes,
                offer.ticket,
            )
            .await
    }
}

impl OfferService {
    /// Hand a device the offers this one is holding for it.
    ///
    /// Needs no grant proof: iroh has already authenticated the remote endpoint
    /// key, and the only thing returned is what this device already decided to
    /// send to precisely that endpoint. A stranger polling gets an empty list.
    async fn handle_poll(&self, remote_endpoint_id: &str) -> PolledOffers {
        PolledOffers {
            offers: self.pairing.collect_held_offers(remote_endpoint_id).await,
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
                OfferMessage::PollOffers(message) => {
                    let WithChannels { tx, .. } = message;
                    let response = self.handle_poll(&remote_endpoint_id).await;
                    let _ = tx.send(response).await;
                }
                OfferMessage::SubmitOffer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .handle_offer(&remote_endpoint_id, &challenge, inner)
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
    pub(crate) async fn poll_offers(&self) -> Result<PolledOffers, irpc::Error> {
        self.inner.rpc(PollOffers).await
    }

    pub(crate) async fn request_challenge(&self) -> Result<Challenge, irpc::Error> {
        Ok(self.inner.rpc(RequestChallenge).await?.challenge)
    }

    pub(crate) async fn submit_offer(
        &self,
        offer: SubmitOffer,
    ) -> Result<OfferResponse, irpc::Error> {
        self.inner.rpc(offer).await
    }

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

/// Hand a paired device a ticket for content it may fetch.
///
/// The ticket is a capability, so this is sent only over a connection where the
/// grant proof has already been presented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SubmitOffer {
    pub(crate) proof: GrantProof,
    pub(crate) ticket: String,
    pub(crate) transfer_name: String,
    pub(crate) sender_display_name: Option<String>,
    pub(crate) file_count: u64,
    pub(crate) total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum OfferResponse {
    /// The receiving user agreed. They fetch the content themselves next.
    Accepted,
    /// The receiving user said no, or never answered.
    Declined { reason: String },
    /// The grant did not validate. Names the reason so a peer holding a dead
    /// grant can clear it.
    Refused { reason: String },
}

/// Ask a device whether it is holding anything for this one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PollOffers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PolledOffers {
    pub(crate) offers: Vec<PolledOffer>,
}

/// An offer collected by polling rather than pushed. Carries the ticket because
/// the sender already decided to send it to this endpoint; the local user still
/// confirms before anything is fetched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PolledOffer {
    pub(crate) ticket: String,
    pub(crate) transfer_name: String,
    pub(crate) sender_display_name: Option<String>,
    pub(crate) file_count: u64,
    pub(crate) total_bytes: u64,
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
    #[rpc(tx=oneshot::Sender<OfferResponse>)]
    SubmitOffer(SubmitOffer),
    #[rpc(tx=oneshot::Sender<PolledOffers>)]
    PollOffers(PollOffers),
}
