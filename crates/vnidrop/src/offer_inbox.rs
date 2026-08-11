//! Incoming transfer offers from paired devices.
//!
//! An offer is only a delivery mechanism for a ticket: it replaces the QR code,
//! not the transfer. Accepting hands the ticket to the platform layer, which
//! runs the ordinary receive with its own destination rules.
//!
//! Nothing in this inbox is persisted: a prompt belongs to a live connection,
//! so a restart correctly loses it rather than resurrecting one whose sender is
//! long gone. Offers the *sender* could not deliver are a different thing and
//! do persist — see `held_offers` in [`crate::contacts`].

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::json;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::{event_hub::EventHub, offer::OfferResponse, util::now_ms};

/// How long the sender waits for the receiving user to decide.
const OFFER_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub(crate) struct PendingOffer {
    pub(crate) offer_id: String,
    pub(crate) from_endpoint_id: String,
    pub(crate) sender_display_name: Option<String>,
    pub(crate) transfer_name: String,
    pub(crate) file_count: u64,
    pub(crate) total_bytes: u64,
    pub(crate) received_at: i64,
    /// Released to the caller only once the local user accepts.
    ticket: String,
}

struct Waiter {
    endpoint_id: String,
    responder: oneshot::Sender<bool>,
}

#[derive(Clone)]
pub(crate) struct OfferInbox {
    event_hub: Arc<EventHub>,
    pending: Arc<Mutex<HashMap<String, PendingOffer>>>,
    waiters: Arc<Mutex<HashMap<String, Waiter>>>,
    /// Endpoint → time before which new offers are refused.
    cooldowns: Arc<Mutex<HashMap<String, i64>>>,
    max_pending: usize,
    decline_cooldown_ms: i64,
}

impl OfferInbox {
    pub(crate) fn new(
        event_hub: Arc<EventHub>,
        max_pending: usize,
        identity_cooldown_ms: u64,
    ) -> Self {
        Self {
            event_hub,
            pending: Arc::new(Mutex::new(HashMap::new())),
            waiters: Arc::new(Mutex::new(HashMap::new())),
            cooldowns: Arc::new(Mutex::new(HashMap::new())),
            max_pending,
            decline_cooldown_ms: identity_cooldown_ms as i64,
        }
    }

    /// Surface an offer and block until the local user decides.
    ///
    /// The caller has already proven a live grant, so this is a known device;
    /// the limits here bound nuisance rather than attack.
    pub(crate) async fn submit(
        &self,
        from_endpoint_id: String,
        transfer_name: String,
        sender_display_name: Option<String>,
        file_count: u64,
        total_bytes: u64,
        ticket: String,
    ) -> OfferResponse {
        let now = now_ms();
        {
            let mut cooldowns = self.cooldowns.lock().await;
            cooldowns.retain(|_, until| *until > now);
            if cooldowns.contains_key(&from_endpoint_id) {
                return OfferResponse::Declined {
                    reason: "declined-recently".to_string(),
                };
            }
        }

        let offer_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= self.max_pending {
                return OfferResponse::Declined {
                    reason: "too-many-pending-offers".to_string(),
                };
            }
            // One prompt per device at a time: a second offer would stack
            // notifications for the same sender.
            if pending
                .values()
                .any(|offer| offer.from_endpoint_id == from_endpoint_id)
            {
                return OfferResponse::Declined {
                    reason: "offer-already-pending".to_string(),
                };
            }
            pending.insert(
                offer_id.clone(),
                PendingOffer {
                    offer_id: offer_id.clone(),
                    from_endpoint_id: from_endpoint_id.clone(),
                    sender_display_name: sender_display_name.clone(),
                    transfer_name: transfer_name.clone(),
                    file_count,
                    total_bytes,
                    received_at: now,
                    ticket,
                },
            );
        }
        self.waiters.lock().await.insert(
            offer_id.clone(),
            Waiter {
                endpoint_id: from_endpoint_id.clone(),
                responder: tx,
            },
        );

        // The ticket is deliberately absent: an event is a log record, and a
        // ticket is a capability.
        self.event_hub.emit_endpoint(
            "offer",
            "offer-received",
            json!({
                "offer_id": offer_id,
                "from_endpoint_id": from_endpoint_id,
                "sender_display_name": sender_display_name,
                "transfer_name": transfer_name,
                "file_count": file_count,
                "total_bytes": total_bytes,
            }),
        );

        match tokio::time::timeout(OFFER_WAIT_TIMEOUT, rx).await {
            Ok(Ok(true)) => OfferResponse::Accepted,
            Ok(Ok(false)) => OfferResponse::Declined {
                reason: "receiver-declined".to_string(),
            },
            // Dropped responder or timeout: clear the prompt so it cannot
            // linger after the sender has given up.
            Ok(Err(_)) | Err(_) => {
                self.discard(&offer_id).await;
                OfferResponse::Declined {
                    reason: "no-response".to_string(),
                }
            }
        }
    }

    /// Add an offer collected by polling.
    ///
    /// Unlike [`Self::submit`] there is no remote waiting on the answer: the
    /// sender handed the ticket over and moved on, so this returns immediately.
    pub(crate) async fn enqueue(
        &self,
        from_endpoint_id: String,
        transfer_name: String,
        sender_display_name: Option<String>,
        file_count: u64,
        total_bytes: u64,
        ticket: String,
    ) -> bool {
        let offer_id = uuid::Uuid::new_v4().to_string();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= self.max_pending {
                return false;
            }
            if pending
                .values()
                .any(|offer| offer.from_endpoint_id == from_endpoint_id)
            {
                return false;
            }
            pending.insert(
                offer_id.clone(),
                PendingOffer {
                    offer_id: offer_id.clone(),
                    from_endpoint_id: from_endpoint_id.clone(),
                    sender_display_name: sender_display_name.clone(),
                    transfer_name: transfer_name.clone(),
                    file_count,
                    total_bytes,
                    received_at: now_ms(),
                    ticket,
                },
            );
        }
        self.event_hub.emit_endpoint(
            "offer",
            "offer-collected",
            json!({
                "offer_id": offer_id,
                "from_endpoint_id": from_endpoint_id,
                "sender_display_name": sender_display_name,
                "transfer_name": transfer_name,
                "file_count": file_count,
                "total_bytes": total_bytes,
            }),
        );
        true
    }

    pub(crate) async fn list(&self) -> Vec<PendingOffer> {
        self.pending.lock().await.values().cloned().collect()
    }

    /// Record the local user's decision.
    ///
    /// Returns the ticket on acceptance: it leaves the core at the moment of
    /// consent and not before, so a declined offer never hands over a
    /// capability. The caller then runs the ordinary receive with it.
    pub(crate) async fn respond(&self, offer_id: &str, accepted: bool) -> Option<String> {
        let offer = self.pending.lock().await.remove(offer_id)?;
        let waiter = self.waiters.lock().await.remove(offer_id);

        if !accepted {
            self.cooldowns.lock().await.insert(
                offer.from_endpoint_id.clone(),
                now_ms() + self.decline_cooldown_ms,
            );
        }
        if let Some(waiter) = waiter {
            let _ = waiter.responder.send(accepted);
        }
        self.event_hub.emit_endpoint(
            "offer",
            if accepted {
                "offer-accepted"
            } else {
                "offer-declined"
            },
            json!({
                "offer_id": offer_id,
                "from_endpoint_id": offer.from_endpoint_id,
            }),
        );

        accepted.then_some(offer.ticket)
    }

    /// Drop every prompt from a device, used when it is forgotten or blocked
    /// while an offer is on screen.
    pub(crate) async fn discard_from(&self, endpoint_id: &str) {
        let ids: Vec<String> = {
            let pending = self.pending.lock().await;
            pending
                .values()
                .filter(|offer| offer.from_endpoint_id == endpoint_id)
                .map(|offer| offer.offer_id.clone())
                .collect()
        };
        for offer_id in ids {
            self.discard(&offer_id).await;
        }
    }

    async fn discard(&self, offer_id: &str) {
        self.pending.lock().await.remove(offer_id);
        if let Some(waiter) = self.waiters.lock().await.remove(offer_id) {
            let _ = waiter.responder.send(false);
            let _ = waiter.endpoint_id;
        }
    }
}
