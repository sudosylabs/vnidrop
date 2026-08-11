//! Live-session inbox for unapproved targeted-transfer offers.
//!
//! Offers are not durable: cancellation, timeout, disconnect, or restart drops
//! them. Authorization is delivered only after the local user accepts.

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::json;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::{api::PendingTargetedOffer, event_hub::EventHub};

const OFFER_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub(crate) struct PendingTargetedOfferRecord {
    pub(crate) offer: PendingTargetedOffer,
}

struct DecisionWaiter {
    decision: oneshot::Sender<bool>,
}

struct AuthWaiter {
    auth: oneshot::Sender<String>,
}

#[derive(Clone)]
pub(crate) struct TargetedOfferInbox {
    event_hub: Arc<EventHub>,
    pending: Arc<Mutex<HashMap<String, PendingTargetedOfferRecord>>>,
    decisions: Arc<Mutex<HashMap<String, DecisionWaiter>>>,
    auths: Arc<Mutex<HashMap<String, AuthWaiter>>>,
    max_pending: usize,
}

impl TargetedOfferInbox {
    pub(crate) fn new(event_hub: Arc<EventHub>, max_pending: usize) -> Self {
        Self {
            event_hub,
            pending: Arc::new(Mutex::new(HashMap::new())),
            decisions: Arc::new(Mutex::new(HashMap::new())),
            auths: Arc::new(Mutex::new(HashMap::new())),
            max_pending,
        }
    }

    /// Surface a validated offer and block until the local user decides.
    pub(crate) async fn submit(&self, offer: PendingTargetedOffer) -> TargetedOfferDecision {
        let transfer_id = offer.transfer_id.clone();
        let (decision_tx, decision_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= self.max_pending {
                return TargetedOfferDecision::Refused {
                    reason: "too-many-pending-offers".to_string(),
                };
            }
            if pending
                .values()
                .any(|entry| entry.offer.sender_endpoint_id == offer.sender_endpoint_id)
            {
                return TargetedOfferDecision::Refused {
                    reason: "offer-already-pending".to_string(),
                };
            }
            if pending.contains_key(&transfer_id) {
                return TargetedOfferDecision::Refused {
                    reason: "duplicate-transfer".to_string(),
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
            DecisionWaiter {
                decision: decision_tx,
            },
        );

        self.event_hub.emit_endpoint(
            "targeted_transfer",
            "offer-received",
            json!({
                "transfer_id": offer.transfer_id,
                "sender_endpoint_id": offer.sender_endpoint_id,
                "file_count": offer.file_count,
                "total_size": offer.total_size,
                "manifest_id": offer.manifest_id,
            }),
        );

        match tokio::time::timeout(OFFER_WAIT_TIMEOUT, decision_rx).await {
            Ok(Ok(true)) => TargetedOfferDecision::Accepted,
            Ok(Ok(false)) => {
                self.discard(&transfer_id).await;
                TargetedOfferDecision::Declined {
                    reason: "receiver-declined".to_string(),
                }
            }
            Ok(Err(_)) | Err(_) => {
                self.discard(&transfer_id).await;
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

    /// Record the local decision. On accept, wait for sender-issued authorization.
    pub(crate) async fn respond(
        &self,
        transfer_id: &str,
        accepted: bool,
    ) -> Result<Option<String>, RespondError> {
        let exists = self.pending.lock().await.contains_key(transfer_id);
        if !exists {
            return Err(RespondError::Unknown);
        }
        let waiter = self.decisions.lock().await.remove(transfer_id);
        let Some(waiter) = waiter else {
            return Err(RespondError::Unknown);
        };
        if !accepted {
            let _ = waiter.decision.send(false);
            self.discard(transfer_id).await;
            self.event_hub.emit_endpoint(
                "targeted_transfer",
                "offer-declined",
                json!({ "transfer_id": transfer_id }),
            );
            return Ok(None);
        }

        let (auth_tx, auth_rx) = oneshot::channel();
        self.auths
            .lock()
            .await
            .insert(transfer_id.to_string(), AuthWaiter { auth: auth_tx });
        if waiter.decision.send(true).is_err() {
            self.auths.lock().await.remove(transfer_id);
            self.discard(transfer_id).await;
            return Err(RespondError::SenderGone);
        }

        match tokio::time::timeout(OFFER_WAIT_TIMEOUT, auth_rx).await {
            Ok(Ok(auth)) => {
                self.pending.lock().await.remove(transfer_id);
                self.event_hub.emit_endpoint(
                    "targeted_transfer",
                    "offer-accepted",
                    json!({ "transfer_id": transfer_id }),
                );
                Ok(Some(auth))
            }
            Ok(Err(_)) | Err(_) => {
                self.discard(transfer_id).await;
                Err(RespondError::AuthorizationTimeout)
            }
        }
    }

    pub(crate) async fn deliver_authorization(
        &self,
        transfer_id: &str,
        authorization: String,
    ) -> bool {
        if let Some(waiter) = self.auths.lock().await.remove(transfer_id) {
            waiter.auth.send(authorization).is_ok()
        } else {
            false
        }
    }

    #[allow(
        dead_code,
        reason = "called via cancel_targeted_transfers_for_peer for ticket 09"
    )]
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

    async fn discard(&self, transfer_id: &str) {
        self.pending.lock().await.remove(transfer_id);
        if let Some(waiter) = self.decisions.lock().await.remove(transfer_id) {
            let _ = waiter.decision.send(false);
        }
        self.auths.lock().await.remove(transfer_id);
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
