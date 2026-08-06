//! Consent and grant exchange for device history.
//!
//! Mirrors [`crate::approval`]: the protocol handler stays thin and the
//! decisions live here. The rule this module exists to enforce is that a device
//! is remembered only if *both* sides agree — refusing to issue a grant leaves
//! the peer with a contact entry that cannot do anything.

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::json;
use tokio::sync::Mutex;

use crate::{
    contacts::ContactStore,
    event_hub::EventHub,
    grant::{
        Challenge, GrantLifetime, GrantProof, GrantRejection, GrantSecret, HeldGrant, IssuedGrant,
    },
    offer::{DeliverGrant, GrantDeliveryResponse, PolledOffer, RevocationResponse, RevokeGrant},
    util::now_ms,
};

/// How long an incoming grant waits for the local user's decision.
///
/// Bounded so a peer cannot park entries in memory indefinitely, and short
/// enough that a stale prompt does not outlive the context the user remembers.
const CONSENT_WINDOW: Duration = Duration::from_secs(10 * 60);

/// A grant a peer has offered, waiting on the local user.
///
/// Not persisted: if the app restarts, the prompt is gone and the peer can
/// offer again. Persisting would resurrect prompts whose context the user has
/// long forgotten.
#[derive(Debug, Clone)]
pub(crate) struct PendingGrant {
    pub(crate) peer_endpoint_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) received_at: i64,
    grant: HeldGrant,
}

#[derive(Clone)]
pub(crate) struct PairingService {
    contacts: ContactStore,
    event_hub: Arc<EventHub>,
    /// Keyed by peer endpoint id: one outstanding offer per peer, so a peer
    /// cannot flood the prompt queue by reconnecting.
    pending: Arc<Mutex<HashMap<String, PendingGrant>>>,
    max_pending: usize,
    max_metadata_bytes: u64,
    lifetime: Arc<Mutex<GrantLifetime>>,
}

impl PairingService {
    pub(crate) fn new(
        contacts: ContactStore,
        event_hub: Arc<EventHub>,
        max_pending: usize,
        max_metadata_bytes: u64,
    ) -> Self {
        Self {
            contacts,
            event_hub,
            pending: Arc::new(Mutex::new(HashMap::new())),
            max_pending,
            max_metadata_bytes,
            lifetime: Arc::new(Mutex::new(GrantLifetime::default())),
        }
    }

    pub(crate) async fn set_grant_lifetime(&self, lifetime: GrantLifetime) {
        *self.lifetime.lock().await = lifetime;
    }

    pub(crate) async fn grant_lifetime(&self) -> GrantLifetime {
        *self.lifetime.lock().await
    }

    // -- inbound ----------------------------------------------------------

    /// A peer offers this device the capability to reach it.
    ///
    /// Never stored on arrival: an unsolicited grant would otherwise create a
    /// contact the local user never agreed to. It waits for consent instead.
    pub(crate) async fn receive_grant(
        &self,
        peer_endpoint_id: String,
        delivery: DeliverGrant,
    ) -> GrantDeliveryResponse {
        if self
            .contacts
            .is_blocked(&peer_endpoint_id)
            .await
            .unwrap_or(false)
        {
            // Indistinguishable from any other refusal: blocking must not be
            // detectable by probing.
            return GrantDeliveryResponse::Rejected {
                reason: "not-accepted".to_string(),
            };
        }
        if delivery
            .display_name
            .as_deref()
            .is_some_and(|name| name.len() as u64 > self.max_metadata_bytes)
        {
            return GrantDeliveryResponse::Rejected {
                reason: "metadata-too-large".to_string(),
            };
        }
        let secret = match GrantSecret::decode(&delivery.secret) {
            Ok(secret) => secret,
            Err(_) => {
                return GrantDeliveryResponse::Rejected {
                    reason: "malformed-grant".to_string(),
                }
            }
        };

        let now = now_ms();
        let held = HeldGrant {
            grant_id: delivery.grant_id,
            secret,
            peer_endpoint_id: peer_endpoint_id.clone(),
            created_at: now,
            expires_at: delivery.expires_at,
        };

        // Already a contact: the user agreed to this relationship, so a refreshed
        // grant (re-pairing, or a renewal after reinstall) replaces the old one
        // without prompting again.
        let already_known = self
            .contacts
            .find_contact(&peer_endpoint_id)
            .await
            .ok()
            .flatten()
            .is_some();
        if already_known {
            if self.contacts.insert_held_grant(&held).await.is_err() {
                return GrantDeliveryResponse::Rejected {
                    reason: "storage-error".to_string(),
                };
            }
            self.emit(
                "grant-refreshed",
                json!({ "peer_endpoint_id": peer_endpoint_id }),
            );
            return GrantDeliveryResponse::Stored;
        }

        let mut pending = self.pending.lock().await;
        self.drop_expired(&mut pending, now);
        if !pending.contains_key(&peer_endpoint_id) && pending.len() >= self.max_pending {
            drop(pending);
            return GrantDeliveryResponse::Rejected {
                reason: "too-many-pending".to_string(),
            };
        }
        pending.insert(
            peer_endpoint_id.clone(),
            PendingGrant {
                peer_endpoint_id: peer_endpoint_id.clone(),
                display_name: delivery.display_name.clone(),
                received_at: now,
                grant: held,
            },
        );
        drop(pending);

        self.emit(
            "pairing-requested",
            json!({
                "peer_endpoint_id": peer_endpoint_id,
                "display_name": delivery.display_name,
            }),
        );
        GrantDeliveryResponse::AwaitingConsent
    }

    /// A peer reports that a grant this device holds is dead.
    ///
    /// Only the issuer may retire its own grant, so the held record must name
    /// this peer. A mismatch answers `Unknown` rather than an error, so a
    /// stranger cannot probe for grant ids belonging to someone else.
    pub(crate) async fn receive_revocation(
        &self,
        peer_endpoint_id: String,
        revocation: RevokeGrant,
    ) -> RevocationResponse {
        let held = self
            .contacts
            .held_grant_for(&peer_endpoint_id)
            .await
            .ok()
            .flatten();
        let Some(held) = held else {
            return RevocationResponse::Unknown;
        };
        if held.grant_id != revocation.grant_id {
            return RevocationResponse::Unknown;
        }
        if self
            .contacts
            .delete_held_grant(revocation.grant_id)
            .await
            .is_err()
        {
            return RevocationResponse::Unknown;
        }
        self.emit(
            "contact-revoked-by-peer",
            json!({ "peer_endpoint_id": peer_endpoint_id }),
        );
        RevocationResponse::Removed
    }

    // -- local decisions --------------------------------------------------

    pub(crate) async fn list_pending_grants(&self) -> Vec<PendingGrant> {
        let mut pending = self.pending.lock().await;
        self.drop_expired(&mut pending, now_ms());
        pending.values().cloned().collect()
    }

    /// Accept a peer's offer to be remembered.
    ///
    /// Stores their grant and records the contact. Issuing our own grant in
    /// return is a separate decision the caller makes, because "I want to reach
    /// them" and "they may reach me" are independent.
    pub(crate) async fn accept_pending_grant(
        &self,
        peer_endpoint_id: &str,
    ) -> anyhow::Result<bool> {
        let pending = {
            let mut pending = self.pending.lock().await;
            self.drop_expired(&mut pending, now_ms());
            pending.remove(peer_endpoint_id)
        };
        let Some(pending) = pending else {
            return Ok(false);
        };

        self.contacts
            .upsert_contact(
                peer_endpoint_id,
                pending.display_name.as_deref(),
                pending.received_at,
            )
            .await?;
        self.contacts.insert_held_grant(&pending.grant).await?;
        self.emit(
            "contact-added",
            json!({ "peer_endpoint_id": peer_endpoint_id }),
        );
        Ok(true)
    }

    /// Decline to be reachable through this peer's grant. The grant is dropped
    /// unstored, so nothing about the peer is retained.
    pub(crate) async fn decline_pending_grant(&self, peer_endpoint_id: &str) -> bool {
        let removed = {
            let mut pending = self.pending.lock().await;
            pending.remove(peer_endpoint_id).is_some()
        };
        if removed {
            self.emit(
                "pairing-declined",
                json!({ "peer_endpoint_id": peer_endpoint_id }),
            );
        }
        removed
    }

    /// Mint a grant for a peer: our consent to be reached by them.
    ///
    /// The caller delivers it over the offer protocol. Persisted before
    /// delivery so a grant we may already have handed over is never forgotten.
    pub(crate) async fn issue_grant(&self, peer_endpoint_id: &str) -> anyhow::Result<IssuedGrant> {
        let lifetime = self.grant_lifetime().await;
        let grant = IssuedGrant::mint(peer_endpoint_id.to_string(), now_ms(), lifetime);
        self.contacts.insert_issued_grant(&grant).await?;
        self.contacts
            .upsert_contact(peer_endpoint_id, None, now_ms())
            .await?;
        self.emit(
            "grant-issued",
            json!({ "peer_endpoint_id": peer_endpoint_id }),
        );
        Ok(grant)
    }

    /// Held offers addressed to `endpoint_id`, consumed as they are handed over.
    ///
    /// Deleting on delivery is what keeps a device that polls twice from being
    /// offered the same transfer again.
    pub(crate) async fn collect_held_offers(&self, endpoint_id: &str) -> Vec<PolledOffer> {
        if self.contacts.is_blocked(endpoint_id).await.unwrap_or(false) {
            return Vec::new();
        }
        let Ok(held) = self.contacts.held_offers_for(endpoint_id).await else {
            return Vec::new();
        };
        if held.is_empty() {
            return Vec::new();
        }
        let ids: Vec<String> = held.iter().map(|offer| offer.offer_id.clone()).collect();
        if let Err(error) = self.contacts.delete_held_offers(&ids).await {
            // Handing the same offer over twice is worse than not handing it
            // over at all, so a failed consume aborts the delivery.
            tracing::warn!(%error, "failed to consume held offers");
            return Vec::new();
        }
        self.emit(
            "held-offers-collected",
            json!({ "peer_endpoint_id": endpoint_id, "count": held.len() }),
        );
        held.into_iter()
            .map(|offer| PolledOffer {
                ticket: offer.ticket,
                transfer_name: offer.transfer_name,
                sender_display_name: offer.sender_display_name,
                file_count: offer.file_count,
                total_bytes: offer.total_bytes,
            })
            .collect()
    }

    /// Validate a proof a peer presented, and push the idle deadline forward.
    ///
    /// The grant record is ours: we issued it, so we are the only party that
    /// can decide it is still alive. A blocked endpoint is answered `Unknown`,
    /// the same as one we never issued to.
    pub(crate) async fn verify_and_renew(
        &self,
        proof: &GrantProof,
        challenge: &Challenge,
        issuer_endpoint_id: &str,
        remote_endpoint_id: &str,
    ) -> Result<(), GrantRejection> {
        if self
            .contacts
            .is_blocked(remote_endpoint_id)
            .await
            .unwrap_or(false)
        {
            return Err(GrantRejection::Unknown);
        }
        let grant = self
            .contacts
            .find_issued_grant(proof.grant_id)
            .await
            .map_err(|_| GrantRejection::Unknown)?
            .ok_or(GrantRejection::Unknown)?;

        let now = now_ms();
        let lifetime = self.grant_lifetime().await;
        let renewed = grant.accept(
            proof,
            challenge,
            issuer_endpoint_id,
            remote_endpoint_id,
            now,
            lifetime,
        )?;
        // A failed renewal is not grounds to refuse a peer that just proved
        // possession; the grant stays valid until its existing deadline.
        if let Err(error) = self
            .contacts
            .renew_issued_grant(proof.grant_id, renewed)
            .await
        {
            tracing::warn!(%error, "failed to renew grant deadline");
        }
        Ok(())
    }

    fn drop_expired(&self, pending: &mut HashMap<String, PendingGrant>, now_ms: i64) {
        let window = CONSENT_WINDOW.as_millis() as i64;
        pending.retain(|_, entry| now_ms - entry.received_at < window);
    }

    fn emit(&self, kind: &str, data: serde_json::Value) {
        self.event_hub.emit_endpoint("contacts", kind, data);
    }
}
