//! Runtime operations for device history: pairing, forgetting, and blocking.
//!
//! The protocol side lives in [`crate::offer`] and the decisions in
//! [`crate::pairing`]; this is where those meet the endpoint and the UniFFI
//! surface.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use iroh::{EndpointAddr, EndpointId};
use serde_json::json;

use super::{CoreInner, POLL_MIN_INTERVAL_MS};
use crate::{
    api::{
        ContactSendResult, ContactSummary, GrantLifetimeSetting, HeldOfferSummary, IncomingOffer,
        PendingPairing, ShareMetadataInput, ShareResult, ShareSource, TransferAccessMode,
    },
    contacts::HeldOffer,
    error::VnidropError,
    grant::{GrantId, HeldGrant},
    offer::{
        DeliverGrant, GrantDeliveryResponse, OfferResponse, OfferService, RevokeGrant, SubmitOffer,
    },
    ticket::{encode_persisted_sender_address, parse_persisted_sender_address},
    transfer_state::{TransferDirection, TransferStatus},
    util::now_ms,
};

/// How long to wait for a device to answer before treating it as not running.
///
/// Without this an offline peer never fails, it just keeps being retried, and
/// the offer is never handed to the hold-for-later path.
const OFFER_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Whether a device may be polled again yet.
///
/// Split out because the surrounding call needs two live nodes to exercise,
/// while the window itself is worth asserting on its own.
pub(crate) fn should_poll(last_polled_ms: Option<i64>, now_ms: i64) -> bool {
    last_polled_ms.is_none_or(|last| now_ms - last >= POLL_MIN_INTERVAL_MS)
}

impl CoreInner {
    pub(super) async fn list_pairing_eligibilities(
        &self,
    ) -> Result<Vec<crate::api::PairingEligibilitySummary>, crate::error::VnidropError> {
        self.pairing_eligibility.list().await
    }

    pub(super) async fn decline_pairing_eligibility(
        &self,
        peer_endpoint_id: String,
    ) -> Result<(), crate::error::VnidropError> {
        self.pairing_eligibility.decline(&peer_endpoint_id).await
    }

    pub(super) async fn request_saved_device_pairing(
        &self,
        peer_endpoint_id: String,
    ) -> Result<bool, crate::error::VnidropError> {
        self.pairing_eligibility
            .request_pairing(&peer_endpoint_id)
            .await
    }

    #[cfg(test)]
    pub(super) async fn submit_pairing_eligibility_for_test(
        &self,
        peer_endpoint_id: String,
        session_id: String,
        capability: Vec<u8>,
    ) -> Result<bool, crate::error::VnidropError> {
        let material = crate::secure_secret::SecretMaterial::new(capability)?;
        self.pairing_eligibility
            .accept_presented_eligibility(&peer_endpoint_id, &session_id, &material)
            .await
    }

    pub(super) async fn list_contacts(&self) -> Result<Vec<ContactSummary>> {
        let contacts = self
            .repository
            .contacts()
            .list_contacts()
            .await
            .map_err(VnidropError::repository)?;
        let store = self.repository.contacts();
        let mut summaries = Vec::with_capacity(contacts.len());
        for contact in contacts {
            // "Can I reach them" is exactly "do I hold a live grant", so the two
            // never drift apart in the UI.
            let can_send = store
                .held_grant_for(&contact.endpoint_id)
                .await
                .map_err(VnidropError::repository)?
                .is_some();
            summaries.push(ContactSummary {
                endpoint_id: contact.endpoint_id,
                local_label: contact.local_label,
                remote_display_name: contact.remote_display_name,
                last_transfer_at: contact.last_transfer_at,
                created_at: contact.created_at,
                can_send,
            });
        }
        Ok(summaries)
    }

    pub(super) async fn list_pending_pairings(&self) -> Vec<PendingPairing> {
        self.pairing
            .list_pending_grants()
            .await
            .into_iter()
            .map(|pending| PendingPairing {
                endpoint_id: pending.peer_endpoint_id,
                display_name: pending.display_name,
                received_at: pending.received_at,
            })
            .collect()
    }

    /// Agree to be remembered by a peer, and hand them the capability to reach
    /// us.
    ///
    /// The grant is persisted before delivery: a grant that may already have
    /// arrived must never be one we have forgotten issuing, or the peer would
    /// hold a capability we cannot validate or revoke.
    pub(super) async fn allow_device_to_reach_me(
        self: &Arc<Self>,
        endpoint_id: String,
        display_name: Option<String>,
    ) -> Result<()> {
        self.limits
            .validate_metadata_text("display name", display_name.as_deref())
            .map_err(VnidropError::invalid_input)?;
        if self
            .repository
            .contacts()
            .is_blocked(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?
        {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "endpoint is blocked; unblock it before pairing"
            ))
            .into());
        }

        let grant = self
            .pairing
            .issue_grant(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?;

        let addr = self.contact_addr(&endpoint_id).await?;
        let client = OfferService::client(self.endpoint.clone(), addr);
        let response = client
            .deliver_grant(DeliverGrant {
                grant_id: grant.grant_id,
                secret: grant.secret.encode(),
                expires_at: grant.expires_at,
                display_name,
            })
            .await
            .context("failed to deliver grant")
            .map_err(VnidropError::transfer)?;

        match response {
            GrantDeliveryResponse::AwaitingConsent | GrantDeliveryResponse::Stored => {
                self.remember_addr(&endpoint_id).await;
                self.emit_endpoint(
                    "contacts",
                    "grant-delivered",
                    json!({ "peer_endpoint_id": endpoint_id }),
                );
                Ok(())
            }
            GrantDeliveryResponse::Rejected { reason } => {
                // The peer would not take it, so the grant we just minted can
                // never be used. Retire it rather than leaving a live
                // capability nobody holds.
                let _ = self
                    .repository
                    .contacts()
                    .revoke_issued_grant(grant.grant_id, now_ms())
                    .await;
                Err(
                    VnidropError::transfer(anyhow::anyhow!("peer refused the pairing: {reason}"))
                        .into(),
                )
            }
        }
    }

    /// Share content and push the ticket straight to a paired device.
    ///
    /// Two things make this one prompt rather than two: the share is created
    /// with the ticket never leaving this device except over the authenticated
    /// offer connection, and the target endpoint is pre-authorised so the
    /// handshake it runs next does not ask us to approve a transfer we started.
    pub(super) async fn send_to_contact(
        self: &Arc<Self>,
        endpoint_id: String,
        sources: Vec<ShareSource>,
        mut metadata: ShareMetadataInput,
    ) -> Result<ContactSendResult> {
        let store = self.repository.contacts();
        let grant = store
            .held_grant_for(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?
            .ok_or_else(|| {
                VnidropError::permission(anyhow::anyhow!(
                    "no live grant for this device; pair with it again"
                ))
            })?;

        // Invariant: an offer-created share is never public. The recipient is a
        // specific device, so serving it to anyone holding the ticket would
        // widen access beyond what the user asked for.
        metadata.access_mode = TransferAccessMode::ApprovalRequired;
        let sender_name = metadata.sender_name.clone();
        let share = self.share_files(sources, metadata).await?;
        self.offer_share(endpoint_id, grant, share, sender_name.as_deref())
            .await
    }

    /// Offer a share that already exists, so a transfer created for an
    /// invitation can also be pushed to a remembered device.
    ///
    /// The ticket is the one already stored for the transfer: this adds another
    /// way to deliver it, it does not create a second share of the same files.
    pub(super) async fn offer_transfer_to_contact(
        self: &Arc<Self>,
        transfer_id: u64,
        endpoint_id: String,
    ) -> Result<ContactSendResult> {
        let grant = self
            .repository
            .contacts()
            .held_grant_for(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?
            .ok_or_else(|| {
                VnidropError::permission(anyhow::anyhow!(
                    "no live grant for this device; pair with it again"
                ))
            })?;

        let stored = self
            .repository
            .list_transfers()
            .await
            .map_err(VnidropError::repository)?
            .into_iter()
            .find(|transfer| transfer.transfer_id == transfer_id)
            .ok_or_else(|| {
                VnidropError::invalid_input(anyhow::anyhow!("unknown transfer {transfer_id}"))
            })?;

        // Only a live share can be offered: a stopped one no longer serves its
        // content, so handing out its ticket would promise nothing.
        if stored.direction != TransferDirection::Send.as_str()
            || stored.status != TransferStatus::Sharing.as_str()
        {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "transfer {transfer_id} is not an active share"
            ))
            .into());
        }
        let ticket = stored.ticket.clone().ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("transfer {transfer_id} has no invitation"))
        })?;

        let share = ShareResult {
            transfer_id,
            ticket,
            hash: stored.content_hash.unwrap_or_default(),
            transfer_name: stored.transfer_name.unwrap_or_default(),
            file_count: stored.file_count,
            total_size: stored.total_size,
        };
        self.offer_share(endpoint_id, grant, share, None).await
    }

    /// Deliver an offer for `share`, holding it when the device is not running.
    async fn offer_share(
        self: &Arc<Self>,
        endpoint_id: String,
        grant: HeldGrant,
        share: ShareResult,
        sender_name: Option<&str>,
    ) -> Result<ContactSendResult> {
        let store = self.repository.contacts();

        // An unreachable device is the common case on mobile, not an error: the
        // share stays here and the ticket waits for the peer to come and get it.
        let outcome = match self
            .deliver_offer(&endpoint_id, &grant, &share, sender_name)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.hold_offer(&endpoint_id, &share, sender_name).await?;
                tracing::debug!(%error, "offer held for later pickup");
                return Ok(ContactSendResult {
                    share,
                    delivered: false,
                });
            }
        };

        match outcome {
            OfferResponse::Accepted => {
                store
                    .touch_transfer(&endpoint_id, now_ms())
                    .await
                    .map_err(VnidropError::repository)?;
                self.remember_addr(&endpoint_id).await;
                self.emit_transfer(
                    share.transfer_id,
                    "send",
                    "offer",
                    "offer-accepted",
                    json!({ "peer_endpoint_id": endpoint_id }),
                );
                Ok(ContactSendResult {
                    share,
                    delivered: true,
                })
            }
            OfferResponse::Declined { reason } | OfferResponse::Refused { reason } => {
                let _ = self.cancel_idle_or_share(share.transfer_id).await;
                self.emit_transfer(
                    share.transfer_id,
                    "send",
                    "offer",
                    "offer-refused",
                    json!({ "peer_endpoint_id": endpoint_id, "reason": reason }),
                );
                // A refusal naming a dead grant is the peer telling us to stop
                // believing we can reach them.
                if matches!(reason.as_str(), "revoked" | "unknown" | "expired") {
                    let _ = store.delete_held_grant(grant.grant_id).await;
                }
                Err(VnidropError::permission(anyhow::anyhow!(
                    "device did not accept the transfer: {reason}"
                ))
                .into())
            }
        }
    }

    /// Keep an undeliverable offer on this device.
    ///
    /// The target is pre-authorised now rather than at pickup: it will dial
    /// straight back after collecting the ticket, and the session outlives the
    /// round trip.
    async fn hold_offer(
        self: &Arc<Self>,
        endpoint_id: &str,
        share: &ShareResult,
        sender_name: Option<&str>,
    ) -> Result<()> {
        self.access_policy
            .approve_endpoint(share.transfer_id, endpoint_id.to_string())
            .await;
        self.repository
            .contacts()
            .insert_held_offer(&HeldOffer {
                offer_id: uuid::Uuid::new_v4().to_string(),
                endpoint_id: endpoint_id.to_string(),
                transfer_id: share.transfer_id,
                ticket: share.ticket.clone(),
                transfer_name: share.transfer_name.clone(),
                sender_display_name: sender_name.map(ToOwned::to_owned),
                file_count: share.file_count,
                total_bytes: share.total_size,
                created_at: now_ms(),
            })
            .await
            .map_err(VnidropError::repository)?;
        self.emit_transfer(
            share.transfer_id,
            "send",
            "offer",
            "offer-held",
            json!({ "peer_endpoint_id": endpoint_id }),
        );
        Ok(())
    }

    /// Ask remembered devices whether they are holding anything for this one.
    ///
    /// Deliberately only ever called from a foreground transition or an explicit
    /// user action: polling reveals to every contact that the app was opened,
    /// which is why it is neither automatic nor backgrounded.
    pub(super) async fn poll_contacts_for_offers(self: &Arc<Self>) -> Result<u64> {
        let store = self.repository.contacts();
        let contacts = store
            .list_contacts()
            .await
            .map_err(VnidropError::repository)?;
        let now = now_ms();
        let mut collected = 0u64;

        for contact in contacts {
            if store
                .is_blocked(&contact.endpoint_id)
                .await
                .unwrap_or(false)
            {
                continue;
            }
            {
                // Rate limited per device so repeated app switching does not
                // turn into a presence beacon.
                let mut polled = self.last_polled.lock().await;
                if !should_poll(polled.get(&contact.endpoint_id).copied(), now) {
                    continue;
                }
                polled.insert(contact.endpoint_id.clone(), now);
            }

            let Ok(addr) = self.contact_addr(&contact.endpoint_id).await else {
                continue;
            };
            let client = OfferService::client(self.endpoint.clone(), addr);
            let Ok(polled) = client.poll_offers().await else {
                // Offline is the expected outcome, not a failure worth surfacing.
                continue;
            };
            for offer in polled.offers {
                let added = self
                    .offers
                    .enqueue(
                        contact.endpoint_id.clone(),
                        offer.transfer_name,
                        offer.sender_display_name,
                        offer.file_count,
                        offer.total_bytes,
                        offer.ticket,
                    )
                    .await;
                if added {
                    collected += 1;
                }
            }
            self.remember_addr(&contact.endpoint_id).await;
        }
        Ok(collected)
    }

    async fn deliver_offer(
        self: &Arc<Self>,
        endpoint_id: &str,
        grant: &HeldGrant,
        share: &ShareResult,
        sender_name: Option<&str>,
    ) -> Result<OfferResponse> {
        let addr = self.contact_addr(endpoint_id).await?;
        let client = OfferService::client(self.endpoint.clone(), addr);
        let challenge = tokio::time::timeout(OFFER_CONNECT_TIMEOUT, client.request_challenge())
            .await
            .map_err(|_| VnidropError::transfer(anyhow::anyhow!("device did not answer in time")))?
            .context("device is not reachable")
            .map_err(VnidropError::transfer)?;

        // Authorise before offering: the receiver may dial back the instant it
        // accepts, and an unauthorised endpoint would be refused by the
        // provider.
        self.access_policy
            .approve_endpoint(share.transfer_id, endpoint_id.to_string())
            .await;

        client
            .submit_offer(SubmitOffer {
                proof: grant.prove(&challenge, &self.endpoint.id().to_string()),
                ticket: share.ticket.clone(),
                transfer_name: share.transfer_name.clone(),
                sender_display_name: sender_name.map(ToOwned::to_owned),
                file_count: share.file_count,
                total_bytes: share.total_size,
            })
            .await
            .context("failed to deliver the offer")
            .map_err(VnidropError::transfer)
            .map_err(Into::into)
    }

    /// Transfers waiting for their target to come back online.
    pub(super) async fn list_held_offers(&self) -> Result<Vec<HeldOfferSummary>> {
        let held = self
            .repository
            .contacts()
            .list_held_offers()
            .await
            .map_err(VnidropError::repository)?;
        Ok(held
            .into_iter()
            .map(|offer| HeldOfferSummary {
                offer_id: offer.offer_id,
                endpoint_id: offer.endpoint_id,
                transfer_id: offer.transfer_id,
                transfer_name: offer.transfer_name,
                file_count: offer.file_count,
                total_bytes: offer.total_bytes,
                created_at: offer.created_at,
            })
            .collect())
    }

    pub(super) async fn list_pending_offers(&self) -> Vec<IncomingOffer> {
        self.offers
            .list()
            .await
            .into_iter()
            .map(|offer| IncomingOffer {
                offer_id: offer.offer_id,
                from_endpoint_id: offer.from_endpoint_id,
                sender_display_name: offer.sender_display_name,
                transfer_name: offer.transfer_name,
                file_count: offer.file_count,
                total_bytes: offer.total_bytes,
                received_at: offer.received_at,
            })
            .collect()
    }

    /// Answer an incoming offer. Returns the ticket when accepted, so the
    /// platform layer can run the ordinary receive with its own destination.
    pub(super) async fn respond_to_offer(
        &self,
        offer_id: String,
        accepted: bool,
    ) -> Option<String> {
        self.offers.respond(&offer_id, accepted).await
    }

    pub(super) async fn respond_to_pairing(
        &self,
        endpoint_id: String,
        accepted: bool,
    ) -> Result<bool> {
        if accepted {
            self.pairing
                .accept_pending_grant(&endpoint_id)
                .await
                .map_err(VnidropError::repository)
                .map_err(Into::into)
        } else {
            Ok(self.pairing.decline_pending_grant(&endpoint_id).await)
        }
    }

    /// Stop a peer from reaching us and drop the relationship locally.
    ///
    /// Revocation completes locally first: the notification is best effort and
    /// the peer losing access must not depend on being online to hear about it.
    pub(super) async fn forget_contact(self: &Arc<Self>, endpoint_id: String) -> Result<()> {
        let store = self.repository.contacts();
        let revoked = store
            .delete_contact(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?;
        // A prompt on screen from a device we just forgot would be actionable
        // with a grant that no longer exists.
        self.offers.discard_from(&endpoint_id).await;
        self.pairing_eligibility
            .remove_for_peer(&endpoint_id)
            .await
            .map_err(anyhow::Error::from)?;
        self.emit_endpoint(
            "contacts",
            "contact-forgotten",
            json!({ "peer_endpoint_id": endpoint_id, "revoked": revoked.len() }),
        );
        self.notify_revoked(endpoint_id, revoked).await;
        Ok(())
    }

    /// Forget every device at once, alongside the existing history-clearing
    /// actions. Every peer loses access; each is notified best effort.
    pub(super) async fn forget_all_contacts(self: &Arc<Self>) -> Result<u64> {
        let store = self.repository.contacts();
        let contacts = store
            .list_contacts()
            .await
            .map_err(VnidropError::repository)?;
        let revoked = store
            .delete_all_contacts()
            .await
            .map_err(VnidropError::repository)?;
        for contact in &contacts {
            self.offers.discard_from(&contact.endpoint_id).await;
        }
        self.pairing_eligibility
            .remove_all()
            .await
            .map_err(anyhow::Error::from)?;
        self.emit_endpoint(
            "contacts",
            "contacts-cleared",
            json!({ "contacts": contacts.len(), "revoked": revoked.len() }),
        );
        for contact in contacts.iter() {
            self.notify_revoked(contact.endpoint_id.clone(), revoked.clone())
                .await;
        }
        Ok(revoked.len() as u64)
    }

    pub(super) async fn block_contact(self: &Arc<Self>, endpoint_id: String) -> Result<()> {
        let store = self.repository.contacts();
        let revoked = store
            .revoke_issued_grants_for(&endpoint_id, now_ms())
            .await
            .map_err(VnidropError::repository)?;
        store
            .block_endpoint(&endpoint_id, now_ms())
            .await
            .map_err(VnidropError::repository)?;
        store
            .delete_contact(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?;
        self.offers.discard_from(&endpoint_id).await;
        self.pairing_eligibility
            .remove_for_peer(&endpoint_id)
            .await
            .map_err(anyhow::Error::from)?;
        self.emit_endpoint(
            "contacts",
            "contact-blocked",
            json!({ "peer_endpoint_id": endpoint_id }),
        );
        // A blocked peer is told nothing: silence here is what makes blocking
        // undetectable, unlike ordinary revocation.
        let _ = revoked;
        Ok(())
    }

    pub(super) async fn unblock_contact(&self, endpoint_id: String) -> Result<()> {
        self.repository
            .contacts()
            .unblock_endpoint(&endpoint_id)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn list_blocked_contacts(&self) -> Result<Vec<String>> {
        self.repository
            .contacts()
            .list_blocked()
            .await
            .map_err(VnidropError::repository)
            .map_err(Into::into)
    }

    pub(super) async fn set_contact_label(
        &self,
        endpoint_id: String,
        label: Option<String>,
    ) -> Result<()> {
        self.limits
            .validate_metadata_text("contact label", label.as_deref())
            .map_err(VnidropError::invalid_input)?;
        self.repository
            .contacts()
            .set_contact_label(&endpoint_id, label.as_deref())
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn set_grant_lifetime(&self, setting: GrantLifetimeSetting) {
        self.pairing.set_grant_lifetime(setting.into()).await;
    }

    /// Best-effort "your entry is dead" notification, so the peer's list clears
    /// promptly instead of at its next attempt.
    async fn notify_revoked(self: &Arc<Self>, endpoint_id: String, revoked: Vec<GrantId>) {
        if revoked.is_empty() {
            return;
        }
        let Ok(addr) = self.contact_addr(&endpoint_id).await else {
            return;
        };
        let client = OfferService::client(self.endpoint.clone(), addr);
        for grant_id in revoked {
            if let Err(error) = client.revoke_grant(RevokeGrant { grant_id }).await {
                tracing::debug!(%error, "revocation notice undeliverable; peer will learn on next attempt");
                return;
            }
        }
    }

    /// Where to dial a contact.
    ///
    /// Prefers the address cached from the last successful connection, which is
    /// what keeps contacts usable in relay profiles that do not resolve
    /// endpoint ids through public discovery.
    async fn contact_addr(&self, endpoint_id: &str) -> Result<EndpointAddr> {
        let cached = self
            .repository
            .contacts()
            .find_contact(endpoint_id)
            .await
            .ok()
            .flatten()
            .and_then(|contact| contact.last_known_addr)
            .and_then(|encoded| parse_persisted_sender_address(&encoded).ok());
        if let Some(addr) = cached {
            return Ok(addr);
        }
        let parsed: EndpointId = endpoint_id
            .parse()
            .context("contact has an unusable endpoint id")
            .map_err(VnidropError::invalid_input)?;
        Ok(EndpointAddr::from(parsed))
    }

    /// Refresh the cached address after a successful exchange.
    async fn remember_addr(&self, endpoint_id: &str) {
        let Ok(parsed) = endpoint_id.parse::<EndpointId>() else {
            return;
        };
        let Some(info) = self.endpoint.remote_info(parsed).await else {
            return;
        };
        let mut addr = EndpointAddr::from(parsed);
        addr.addrs = info.addrs().map(|entry| entry.addr().clone()).collect();
        if let Ok(encoded) = encode_persisted_sender_address(&addr) {
            let _ = self
                .repository
                .contacts()
                .set_last_known_addr(endpoint_id, &encoded)
                .await;
        }
    }
}
