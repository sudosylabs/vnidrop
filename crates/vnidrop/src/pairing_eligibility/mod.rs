//! Pairing eligibility after completed authenticated invitation transfers.
//!
//! The capability is derived from the shared approval session token and becomes
//! usable only after the transfer reaches a durable completed state. Public APIs
//! expose eligibility state, never the capability bytes.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;

mod store;

pub(crate) use store::PairingEligibilityStore;

use crate::{
    api::{experimental_saved_device_capabilities, PairingEligibilitySummary},
    error::VnidropError,
    event_hub::EventHub,
    secure_secret::{SecretCustody, SecretHandle, SecretKind, SecretMaterial},
    util::now_ms,
};

const ELIGIBILITY_TTL_MS: i64 = 24 * 60 * 60 * 1_000;
const CAPABILITY_CONTEXT: &str = "vnidrop-pairing-eligibility-v1";

#[derive(Clone)]
pub(crate) struct PairingEligibilityService {
    store: PairingEligibilityStore,
    custody: Option<Arc<SecretCustody>>,
    event_hub: Arc<EventHub>,
    local_endpoint_id: String,
}

impl PairingEligibilityService {
    pub(crate) fn new(
        store: PairingEligibilityStore,
        custody: Option<Arc<SecretCustody>>,
        event_hub: Arc<EventHub>,
        local_endpoint_id: String,
    ) -> Self {
        Self {
            store,
            custody,
            event_hub,
            local_endpoint_id,
        }
    }

    /// Removes orphaned eligibility secrets and rows whose secrets are missing.
    pub(crate) async fn reconcile(&self) -> Result<(), VnidropError> {
        let records = self.store.list_records().await?;
        let mut referenced = HashSet::new();
        for entry in records {
            referenced.insert(entry.secret_handle.clone());
            let Some(custody) = &self.custody else {
                continue;
            };
            let handle = SecretHandle::from_stored(entry.secret_handle.clone());
            if custody.load(&handle).await.is_err() {
                self.delete_entry_silent(&entry).await?;
            }
        }
        if let Some(custody) = &self.custody {
            for handle in custody
                .list_active_handles(SecretKind::PairingEligibility)
                .await?
            {
                if !referenced.contains(handle.as_str()) {
                    let _ = custody.remove(&handle).await;
                }
            }
        }
        self.expire_due(true).await
    }

    pub(crate) async fn list(&self) -> Result<Vec<PairingEligibilitySummary>, VnidropError> {
        self.expire_due(true).await?;
        self.store.list_summaries().await
    }

    /// Activates eligibility after a durable completed authenticated transfer.
    pub(crate) async fn activate_after_completed_transfer(
        &self,
        peer_endpoint_id: &str,
        session_id: &str,
        approval_token: &str,
    ) -> Result<(), VnidropError> {
        let Some(custody) = &self.custody else {
            return Ok(());
        };
        if peer_endpoint_id.is_empty() || session_id.is_empty() || approval_token.is_empty() {
            return Ok(());
        }
        if self.store.find_by_session(session_id).await?.is_some() {
            return Ok(());
        }

        let protocol_version =
            experimental_saved_device_capabilities().relationship_protocol_version;
        let capability = derive_capability(
            approval_token,
            &self.local_endpoint_id,
            peer_endpoint_id,
            session_id,
            protocol_version,
        )?;
        // Credential custody already stages then activates the secret. Domain
        // metadata is written only after that verify; a crash leaves an orphan
        // secret that reconcile() removes on the next start.
        let handle = custody
            .protect(SecretKind::PairingEligibility, capability, None)
            .await?;
        let created_at = now_ms();
        let expires_at = created_at + ELIGIBILITY_TTL_MS;
        if let Err(error) = self
            .store
            .insert(PairingEligibilityInsert {
                peer_endpoint_id,
                session_id,
                protocol_version,
                secret_handle: handle.as_str(),
                created_at,
                expires_at,
            })
            .await
        {
            let _ = custody.remove(&handle).await;
            return Err(error);
        }

        self.event_hub.emit_endpoint(
            "pairing",
            "eligibility-available",
            json!({
                "peer_endpoint_id": peer_endpoint_id,
                "session_id": session_id,
                "protocol_version": protocol_version,
                "expires_at": expires_at,
            }),
        );
        Ok(())
    }

    /// Starts a local pairing attempt when eligibility exists.
    ///
    /// Returns `false` when eligibility is missing/expired (silent reject). A
    /// successful start consumes the single-use eligibility for that session.
    #[allow(
        dead_code,
        reason = "retained for eligibility-only callers; mutual consent uses take_eligibility"
    )]
    pub(crate) async fn request_pairing(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<bool, VnidropError> {
        Ok(self.take_eligibility(peer_endpoint_id).await?.is_some())
    }

    /// Takes and consumes eligibility for a peer, returning the capability material.
    pub(crate) async fn take_eligibility(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Option<TakenEligibility>, VnidropError> {
        self.expire_due(true).await?;
        let entries = self.store.list_for_peer(peer_endpoint_id).await?;
        let Some(entry) = entries.into_iter().next() else {
            return Ok(None);
        };
        if entry.expires_at <= now_ms() {
            self.delete_entry_silent(&entry).await?;
            return Ok(None);
        }
        let Some(custody) = &self.custody else {
            self.delete_entry_silent(&entry).await?;
            return Ok(None);
        };
        let capability = match custody
            .load(&SecretHandle::from_stored(entry.secret_handle.clone()))
            .await
        {
            Ok(material) => material,
            Err(_) => {
                self.delete_entry_silent(&entry).await?;
                return Ok(None);
            }
        };
        self.delete_entry(&entry).await?;
        Ok(Some(TakenEligibility {
            session_id: entry.session_id,
            protocol_version: entry.protocol_version,
            capability,
        }))
    }

    /// Validates an inbound eligibility presentation without prompts or events on failure.
    #[allow(
        dead_code,
        reason = "inbound pairing wire acceptance lands with mutual-consent ticket 08"
    )]
    pub(crate) async fn accept_presented_eligibility(
        &self,
        peer_endpoint_id: &str,
        session_id: &str,
        capability: &SecretMaterial,
    ) -> Result<bool, VnidropError> {
        let Some(entry) = self
            .validate_presented_capability(peer_endpoint_id, session_id, capability)
            .await?
        else {
            return Ok(false);
        };
        self.delete_entry(&entry).await?;
        Ok(true)
    }

    /// Consumes eligibility for one session without requiring the capability bytes.
    pub(crate) async fn consume_session(
        &self,
        peer_endpoint_id: &str,
        session_id: &str,
    ) -> Result<(), VnidropError> {
        if let Some(entry) = self.store.find_by_session(session_id).await? {
            if entry.peer_endpoint_id == peer_endpoint_id {
                self.delete_entry(&entry).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn decline(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
        self.remove_for_peer(peer_endpoint_id).await
    }

    pub(crate) async fn remove_for_peer(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
        let entries = self.store.list_for_peer(peer_endpoint_id).await?;
        for entry in entries {
            self.delete_entry(&entry).await?;
        }
        Ok(())
    }

    /// Returns the matching record when the capability is valid; otherwise `None`
    /// without emitting prompts or eligibility-removed events for the reject path.
    #[allow(
        dead_code,
        reason = "inbound pairing wire acceptance lands with mutual-consent ticket 08"
    )]
    pub(crate) async fn validate_presented_capability(
        &self,
        peer_endpoint_id: &str,
        session_id: &str,
        capability: &SecretMaterial,
    ) -> Result<Option<PairingEligibilityRecord>, VnidropError> {
        self.expire_due(false).await?;
        let Some(custody) = &self.custody else {
            return Ok(None);
        };
        let Some(entry) = self.store.find_by_session(session_id).await? else {
            return Ok(None);
        };
        if entry.peer_endpoint_id != peer_endpoint_id || entry.expires_at <= now_ms() {
            return Ok(None);
        }
        let stored = match custody
            .load(&SecretHandle::from_stored(entry.secret_handle.clone()))
            .await
        {
            Ok(material) => material,
            Err(_) => return Ok(None),
        };
        if stored == *capability {
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    async fn expire_due(&self, emit_events: bool) -> Result<(), VnidropError> {
        let now = now_ms();
        let expired = self.store.list_expired(now).await?;
        for entry in expired {
            if emit_events {
                self.delete_entry(&entry).await?;
            } else {
                self.delete_entry_silent(&entry).await?;
            }
        }
        Ok(())
    }

    async fn delete_entry(&self, entry: &PairingEligibilityRecord) -> Result<(), VnidropError> {
        self.delete_entry_silent(entry).await?;
        self.event_hub.emit_endpoint(
            "pairing",
            "eligibility-removed",
            json!({
                "peer_endpoint_id": entry.peer_endpoint_id,
                "session_id": entry.session_id,
            }),
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn force_expiry_for_test(
        &self,
        session_id: &str,
        expires_at: i64,
    ) -> Result<(), VnidropError> {
        self.store
            .force_expiry_for_test(session_id, expires_at)
            .await
    }

    async fn delete_entry_silent(
        &self,
        entry: &PairingEligibilityRecord,
    ) -> Result<(), VnidropError> {
        if let Some(custody) = &self.custody {
            let handle = SecretHandle::from_stored(entry.secret_handle.clone());
            let _ = custody.remove(&handle).await;
        }
        self.store.delete(&entry.session_id).await
    }
}

pub(crate) struct PairingEligibilityInsert<'a> {
    pub(crate) peer_endpoint_id: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) protocol_version: u16,
    pub(crate) secret_handle: &'a str,
    pub(crate) created_at: i64,
    pub(crate) expires_at: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct TakenEligibility {
    pub(crate) session_id: String,
    pub(crate) protocol_version: u16,
    pub(crate) capability: SecretMaterial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairingEligibilityRecord {
    pub(crate) peer_endpoint_id: String,
    pub(crate) session_id: String,
    pub(crate) protocol_version: u16,
    pub(crate) secret_handle: String,
    pub(crate) created_at: i64,
    pub(crate) expires_at: i64,
}

fn derive_capability(
    approval_token: &str,
    local_endpoint_id: &str,
    peer_endpoint_id: &str,
    session_id: &str,
    protocol_version: u16,
) -> Result<SecretMaterial, VnidropError> {
    let mut endpoints = [local_endpoint_id, peer_endpoint_id];
    endpoints.sort_unstable();
    let mut hasher = blake3::Hasher::new_derive_key(CAPABILITY_CONTEXT);
    hasher.update(approval_token.as_bytes());
    hasher.update(&[0]);
    hasher.update(endpoints[0].as_bytes());
    hasher.update(&[0]);
    hasher.update(endpoints[1].as_bytes());
    hasher.update(&[0]);
    hasher.update(session_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(&protocol_version.to_le_bytes());
    let bytes = *hasher.finalize().as_bytes();
    SecretMaterial::new(bytes.to_vec())
}
