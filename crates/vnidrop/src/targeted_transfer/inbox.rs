//! Live-session inbox for unapproved targeted-transfer offers.
//!
//! Offers are not durable: cancellation, timeout, disconnect, or restart drops
//! them. Authorization is delivered only after the local user accepts.
//! The most recent decisions are cached for lost-response replays.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use serde_json::json;
use tokio::sync::{watch, Mutex};

use crate::{api::PendingTargetedOffer, control_plane::IdentityCooldown, event_hub::EventHub};

const MAX_SETTLED_OFFERS: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct PendingTargetedOfferRecord {
    pub(crate) offer: PendingTargetedOffer,
}

struct PendingWaiter {
    decision: watch::Sender<Option<bool>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettledOfferResult {
    Accepted,
    Declined,
}

type SettledOffers = VecDeque<(String, SettledOfferResult)>;

#[derive(Clone)]
pub(crate) struct TargetedOfferInbox {
    event_hub: Arc<EventHub>,
    pending: Arc<Mutex<HashMap<String, PendingTargetedOfferRecord>>>,
    decisions: Arc<Mutex<HashMap<String, PendingWaiter>>>,
    auths: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    settled: Arc<Mutex<SettledOffers>>,
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
            settled: Arc::new(Mutex::new(VecDeque::new())),
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
    /// Replays join the existing pending wait or reuse a recent decision.
    pub(crate) async fn submit(&self, offer: PendingTargetedOffer) -> TargetedOfferDecision {
        let transfer_id = offer.transfer_id.clone();
        if self.cooldown.is_cooling(&offer.sender_endpoint_id) {
            return TargetedOfferDecision::Refused {
                reason: "identity-cooldown".to_string(),
            };
        }
        if let Some(settled) = self.settled_result(&transfer_id).await {
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
                "sender_endpoint_id": offer.sender_endpoint_id,
                "file_count": offer.file_count,
                "total_size": offer.total_size,
                "manifest_id": offer.manifest_id,
            }),
        );

        self.wait_existing_decision(&transfer_id).await
    }

    async fn wait_existing_decision(&self, transfer_id: &str) -> TargetedOfferDecision {
        let receiver = {
            let decisions = self.decisions.lock().await;
            decisions
                .get(transfer_id)
                .map(|waiter| waiter.decision.subscribe())
        };
        let Some(mut rx) = receiver else {
            if let Some(settled) = self.settled_result(transfer_id).await {
                return settled_to_decision(settled);
            }
            return TargetedOfferDecision::Declined {
                reason: "no-response".to_string(),
            };
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

    async fn settled_result(&self, transfer_id: &str) -> Option<SettledOfferResult> {
        self.settled
            .lock()
            .await
            .iter()
            .find_map(|(id, result)| (id == transfer_id).then_some(*result))
    }

    async fn remember_settled(&self, transfer_id: &str, result: SettledOfferResult) {
        let mut settled = self.settled.lock().await;
        settled.retain(|(id, _)| id != transfer_id);
        if settled.len() == MAX_SETTLED_OFFERS {
            settled.pop_front();
        }
        settled.push_back((transfer_id.to_owned(), result));
    }

    pub(crate) async fn is_settled(&self, transfer_id: &str) -> bool {
        self.settled_result(transfer_id).await.is_some()
    }

    /// Record the local decision. On accept, wait for sender-issued authorization.
    pub(crate) async fn respond(
        &self,
        transfer_id: &str,
        accepted: bool,
    ) -> Result<bool, RespondError> {
        if let Some(result) = self.settled_result(transfer_id).await {
            return Ok(result == SettledOfferResult::Accepted);
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
            self.remember_settled(transfer_id, SettledOfferResult::Declined)
                .await;
            let _ = decision_tx.send(Some(false));
            self.discard(transfer_id).await;
            self.cooldown.record_decline(&sender_endpoint_id);
            self.event_hub
                .emit_endpoint("targeted_transfer", "offer-declined", json!({}));
            return Ok(false);
        }

        self.accept_live(transfer_id).await?;
        self.wait_for_authorization(transfer_id).await?;
        Ok(true)
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
        let (auth_tx, _) = watch::channel(false);
        self.auths
            .lock()
            .await
            .insert(transfer_id.to_string(), auth_tx);
        if decision_tx.send(Some(true)).is_err() {
            self.auths.lock().await.remove(transfer_id);
            return Err(RespondError::SenderGone);
        }
        Ok(())
    }

    pub(crate) async fn wait_for_authorization(
        &self,
        transfer_id: &str,
    ) -> Result<(), RespondError> {
        let receiver = {
            let auths = self.auths.lock().await;
            auths.get(transfer_id).map(watch::Sender::subscribe)
        };
        let Some(mut auth_rx) = receiver else {
            return if self.settled_result(transfer_id).await == Some(SettledOfferResult::Accepted) {
                Ok(())
            } else {
                Err(RespondError::Unknown)
            };
        };
        let wait_auth = async {
            loop {
                if *auth_rx.borrow_and_update() {
                    return Ok(());
                }
                if auth_rx.changed().await.is_err() {
                    return Err(());
                }
            }
        };

        match tokio::time::timeout(self.offer_timeout, wait_auth).await {
            Ok(Ok(())) => {
                self.remember_settled(transfer_id, SettledOfferResult::Accepted)
                    .await;
                self.pending.lock().await.remove(transfer_id);
                self.decisions.lock().await.remove(transfer_id);
                self.auths.lock().await.remove(transfer_id);
                Ok(())
            }
            Ok(Err(())) | Err(_) => {
                self.auths.lock().await.remove(transfer_id);
                Err(RespondError::AuthorizationTimeout)
            }
        }
    }

    pub(crate) async fn authorization_stored(&self, transfer_id: &str) {
        if let Some(waiter) = self.auths.lock().await.get(transfer_id) {
            waiter.send_replace(true);
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
        SettledOfferResult::Accepted => TargetedOfferDecision::Accepted,
        SettledOfferResult::Declined => TargetedOfferDecision::Declined {
            reason: "receiver-declined".to_owned(),
        },
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
