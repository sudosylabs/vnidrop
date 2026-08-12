//! Live-session inbox for unapproved targeted-transfer offers.
//!
//! Offers are not durable: cancellation, timeout, disconnect, or restart drops
//! them. Authorization is delivered only after the local user accepts.
//! Settled results are cached briefly so lost-response replays stay idempotent.

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::json;
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use crate::{api::PendingTargetedOffer, control_plane::IdentityCooldown, event_hub::EventHub};

#[derive(Debug, Clone)]
pub(crate) struct PendingTargetedOfferRecord {
    pub(crate) offer: PendingTargetedOffer,
}

struct PendingWaiter {
    decision: watch::Sender<Option<bool>>,
}

struct AuthWaiter {
    auth: watch::Sender<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettledOfferResult {
    Accepted { authorization: Option<String> },
    Declined { reason: String },
}

#[derive(Clone)]
pub(crate) struct TargetedOfferInbox {
    event_hub: Arc<EventHub>,
    pending: Arc<Mutex<HashMap<String, PendingTargetedOfferRecord>>>,
    decisions: Arc<Mutex<HashMap<String, PendingWaiter>>>,
    auths: Arc<Mutex<HashMap<String, AuthWaiter>>>,
    settled: Arc<Mutex<HashMap<String, SettledOfferResult>>>,
    cooldown: IdentityCooldown,
    max_pending: usize,
    offer_timeout: Duration,
}

impl TargetedOfferInbox {
    pub(crate) fn new(
        event_hub: Arc<EventHub>,
        max_pending: usize,
        cooldown: IdentityCooldown,
        offer_timeout_ms: u64,
    ) -> Self {
        Self {
            event_hub,
            pending: Arc::new(Mutex::new(HashMap::new())),
            decisions: Arc::new(Mutex::new(HashMap::new())),
            auths: Arc::new(Mutex::new(HashMap::new())),
            settled: Arc::new(Mutex::new(HashMap::new())),
            cooldown,
            max_pending,
            offer_timeout: Duration::from_millis(offer_timeout_ms),
        }
    }

    pub(crate) fn cooldown(&self) -> &IdentityCooldown {
        &self.cooldown
    }

    /// Surface a validated offer and block until the local user decides.
    ///
    /// Replaying the same transfer identity returns the settled result or joins
    /// the existing pending wait — never a second prompt.
    pub(crate) async fn submit(&self, offer: PendingTargetedOffer) -> TargetedOfferDecision {
        let transfer_id = offer.transfer_id.clone();
        if self.cooldown.is_cooling(&offer.sender_endpoint_id) {
            return TargetedOfferDecision::Refused {
                reason: "identity-cooldown".to_string(),
            };
        }
        if let Some(settled) = self.settled.lock().await.get(&transfer_id).cloned() {
            return settled_to_decision(settled);
        }

        {
            let pending = self.pending.lock().await;
            if let Some(existing) = pending.get(&transfer_id) {
                if offers_equivalent(&existing.offer, &offer) {
                    drop(pending);
                    return self.wait_existing_decision(&transfer_id).await;
                }
                return TargetedOfferDecision::Refused {
                    reason: "immutable-transfer-mismatch".to_string(),
                };
            }
            // Prefer the more specific per-sender refusal before the global bound.
            if pending
                .values()
                .any(|entry| entry.offer.sender_endpoint_id == offer.sender_endpoint_id)
            {
                return TargetedOfferDecision::Refused {
                    reason: "offer-already-pending".to_string(),
                };
            }
            if pending.len() >= self.max_pending {
                return TargetedOfferDecision::Refused {
                    reason: "too-many-pending-offers".to_string(),
                };
            }
        }

        let (decision_tx, _decision_rx) = watch::channel(None);
        {
            let mut pending = self.pending.lock().await;
            if pending.contains_key(&transfer_id) {
                // Lost the race with another submit of the same id.
                drop(pending);
                return self.wait_existing_decision(&transfer_id).await;
            }
            if pending
                .values()
                .any(|entry| entry.offer.sender_endpoint_id == offer.sender_endpoint_id)
            {
                return TargetedOfferDecision::Refused {
                    reason: "offer-already-pending".to_string(),
                };
            }
            if pending.len() >= self.max_pending {
                return TargetedOfferDecision::Refused {
                    reason: "too-many-pending-offers".to_string(),
                };
            }
            pending.insert(
                transfer_id.clone(),
                PendingTargetedOfferRecord {
                    offer: offer.clone(),
                },
            );
        }
        self.decisions.lock().await.insert(
            transfer_id.clone(),
            PendingWaiter {
                decision: decision_tx,
            },
        );
        self.cooldown.clear_strikes(&offer.sender_endpoint_id);

        self.event_hub.emit_endpoint(
            "targeted_transfer",
            "offer-received",
            json!({
                "targeted_transfer_id": offer.transfer_id,
                "sender_endpoint_id": offer.sender_endpoint_id,
                "file_count": offer.file_count,
                "total_size": offer.total_size,
                "manifest_id": offer.manifest_id,
            }),
        );

        self.wait_existing_decision(&transfer_id).await
    }

    async fn wait_existing_decision(&self, transfer_id: &str) -> TargetedOfferDecision {
        let mut rx = {
            let decisions = self.decisions.lock().await;
            let Some(waiter) = decisions.get(transfer_id) else {
                if let Some(settled) = self.settled.lock().await.get(transfer_id).cloned() {
                    return settled_to_decision(settled);
                }
                return TargetedOfferDecision::Declined {
                    reason: "no-response".to_string(),
                };
            };
            waiter.decision.subscribe()
        };

        let wait = async {
            loop {
                if let Some(accepted) = *rx.borrow_and_update() {
                    return accepted;
                }
                if rx.changed().await.is_err() {
                    return false;
                }
            }
        };

        match tokio::time::timeout(self.offer_timeout, wait).await {
            Ok(true) => TargetedOfferDecision::Accepted,
            Ok(false) => {
                self.discard(transfer_id).await;
                TargetedOfferDecision::Declined {
                    reason: "receiver-declined".to_string(),
                }
            }
            Err(_) => {
                self.discard(transfer_id).await;
                TargetedOfferDecision::Declined {
                    reason: "no-response".to_string(),
                }
            }
        }
    }

    pub(crate) async fn list(&self) -> Vec<PendingTargetedOffer> {
        self.pending
            .lock()
            .await
            .values()
            .map(|entry| entry.offer.clone())
            .collect()
    }

    pub(crate) async fn get_pending(&self, transfer_id: &str) -> Option<PendingTargetedOffer> {
        self.pending
            .lock()
            .await
            .get(transfer_id)
            .map(|entry| entry.offer.clone())
    }

    #[cfg(test)]
    pub(crate) async fn authorization_matches_pending(
        &self,
        auth: &crate::targeted_transfer::TargetedAuthorization,
    ) -> bool {
        self.pending
            .lock()
            .await
            .get(&auth.transfer_id)
            .is_some_and(|entry| {
                let offer = &entry.offer;
                offer.sender_endpoint_id == auth.sender_endpoint_id
                    && offer.receiver_endpoint_id == auth.receiver_endpoint_id
                    && offer.manifest_id == auth.manifest_id
                    && offer.content_hash == auth.content_hash
                    && offer.transfer_name == auth.transfer_name
                    && offer.file_count == auth.file_count
                    && offer.total_size == auth.total_size
                    && offer.protocol_version == auth.protocol_version
            })
    }

    pub(crate) async fn settled_authorization(&self, transfer_id: &str) -> Option<String> {
        match self.settled.lock().await.get(transfer_id) {
            Some(SettledOfferResult::Accepted {
                authorization: Some(auth),
            }) => Some(auth.clone()),
            _ => None,
        }
    }

    pub(crate) async fn is_settled(&self, transfer_id: &str) -> bool {
        self.settled.lock().await.contains_key(transfer_id)
    }

    /// Record the local decision. On accept, wait for sender-issued authorization.
    pub(crate) async fn respond(
        &self,
        transfer_id: &str,
        accepted: bool,
    ) -> Result<Option<String>, RespondError> {
        if let Some(auth) = self.settled_authorization(transfer_id).await {
            return Ok(Some(auth));
        }

        let sender_endpoint_id = {
            let pending = self.pending.lock().await;
            pending
                .get(transfer_id)
                .map(|entry| entry.offer.sender_endpoint_id.clone())
        };
        let Some(sender_endpoint_id) = sender_endpoint_id else {
            return Err(RespondError::Unknown);
        };
        if !accepted {
            let decision_tx = {
                let decisions = self.decisions.lock().await;
                decisions
                    .get(transfer_id)
                    .map(|entry| entry.decision.clone())
                    .ok_or(RespondError::Unknown)?
            };
            let _ = decision_tx.send(Some(false));
            self.discard(transfer_id).await;
            self.cooldown.record_decline(&sender_endpoint_id);
            self.settled.lock().await.insert(
                transfer_id.to_string(),
                SettledOfferResult::Declined {
                    reason: "receiver-declined".to_string(),
                },
            );
            self.event_hub.emit_endpoint(
                "targeted_transfer",
                "offer-declined",
                json!({ "targeted_transfer_id": transfer_id }),
            );
            return Ok(None);
        }

        self.accept_live(transfer_id).await?;
        self.wait_for_authorization(transfer_id).await
    }

    pub(crate) async fn pending_for_acceptance(
        &self,
        transfer_id: &str,
    ) -> Option<PendingTargetedOffer> {
        self.get_pending(transfer_id).await
    }

    pub(crate) async fn accept_live(&self, transfer_id: &str) -> Result<(), RespondError> {
        let decision_tx = {
            let decisions = self.decisions.lock().await;
            decisions
                .get(transfer_id)
                .map(|entry| entry.decision.clone())
                .ok_or(RespondError::Unknown)?
        };
        let (auth_tx, auth_rx) = watch::channel(None);
        self.auths
            .lock()
            .await
            .insert(transfer_id.to_string(), AuthWaiter { auth: auth_tx });
        if decision_tx.send(Some(true)).is_err() {
            self.auths.lock().await.remove(transfer_id);
            return Err(RespondError::SenderGone);
        }
        drop(auth_rx);
        Ok(())
    }

    pub(crate) async fn wait_for_authorization(
        &self,
        transfer_id: &str,
    ) -> Result<Option<String>, RespondError> {
        if let Some(auth) = self.settled_authorization(transfer_id).await {
            return Ok(Some(auth));
        }
        let mut auth_rx = {
            let auths = self.auths.lock().await;
            auths
                .get(transfer_id)
                .map(|entry| entry.auth.subscribe())
                .ok_or(RespondError::Unknown)?
        };
        let wait_auth = async {
            loop {
                if let Some(auth) = auth_rx.borrow_and_update().clone() {
                    return Ok(auth);
                }
                if auth_rx.changed().await.is_err() {
                    return Err(());
                }
            }
        };

        match tokio::time::timeout(self.offer_timeout, wait_auth).await {
            Ok(Ok(auth)) => {
                self.pending.lock().await.remove(transfer_id);
                self.auths.lock().await.remove(transfer_id);
                self.settled.lock().await.insert(
                    transfer_id.to_string(),
                    SettledOfferResult::Accepted {
                        authorization: Some(auth.clone()),
                    },
                );
                Ok(Some(auth))
            }
            Ok(Err(())) | Err(_) => {
                self.auths.lock().await.remove(transfer_id);
                Err(RespondError::AuthorizationTimeout)
            }
        }
    }

    pub(crate) async fn deliver_authorization(
        &self,
        transfer_id: &str,
        authorization: String,
    ) -> bool {
        if let Some(SettledOfferResult::Accepted {
            authorization: Some(existing),
        }) = self.settled.lock().await.get(transfer_id)
        {
            return existing == &authorization;
        }
        if let Some(waiter) = self.auths.lock().await.get(transfer_id) {
            waiter.auth.send(Some(authorization)).is_ok()
        } else {
            false
        }
    }

    pub(crate) async fn discard_from(&self, endpoint_id: &str) {
        let ids: Vec<String> = {
            let pending = self.pending.lock().await;
            pending
                .values()
                .filter(|entry| entry.offer.sender_endpoint_id == endpoint_id)
                .map(|entry| entry.offer.transfer_id.clone())
                .collect()
        };
        for id in ids {
            self.discard(&id).await;
        }
    }

    pub(crate) async fn discard(&self, transfer_id: &str) {
        self.pending.lock().await.remove(transfer_id);
        if let Some(waiter) = self.decisions.lock().await.remove(transfer_id) {
            let _ = waiter.decision.send(Some(false));
        }
        self.auths.lock().await.remove(transfer_id);
    }
}

fn offers_equivalent(left: &PendingTargetedOffer, right: &PendingTargetedOffer) -> bool {
    left.transfer_id == right.transfer_id
        && left.sender_endpoint_id == right.sender_endpoint_id
        && left.receiver_endpoint_id == right.receiver_endpoint_id
        && left.manifest_id == right.manifest_id
        && left.content_hash == right.content_hash
        && left.transfer_name == right.transfer_name
        && left.file_count == right.file_count
        && left.total_size == right.total_size
        && left.protocol_version == right.protocol_version
}

fn settled_to_decision(settled: SettledOfferResult) -> TargetedOfferDecision {
    match settled {
        SettledOfferResult::Accepted { .. } => TargetedOfferDecision::Accepted,
        SettledOfferResult::Declined { reason } => TargetedOfferDecision::Declined { reason },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetedOfferDecision {
    Accepted,
    Declined { reason: String },
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RespondError {
    Unknown,
    SenderGone,
    AuthorizationTimeout,
}

#[allow(dead_code)]
pub(crate) fn new_offer_id() -> String {
    Uuid::new_v4().to_string()
}
