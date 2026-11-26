//! Pairing orchestration, grant custody, and Saved-device listing.
//!
//! Transport lives in [`super::protocol`]; durable rows in [`super::store`].

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::Duration,
};

use anyhow::Context;
use data_encoding::HEXLOWER;
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl};
use serde_json::json;
use tokio::sync::Mutex as TokioMutex;

use crate::{
    api::{
        saved_device_capabilities, CoreRelayMode, DeviceRelationship, DeviceRelationshipState,
        SavedDevice,
    },
    blocked_devices::BlockStore,
    error::VnidropError,
    event_hub::EventHub,
    grant::{Challenge, GrantId, GrantProof, GrantSecret},
    pairing_eligibility::PairingEligibilityService,
    secure_secret::{SecretCustody, SecretHandle, SecretKind, SecretMaterial},
    ticket::{encode_persisted_sender_address, filter_peer_addr_for_relay_mode},
    util::now_ms,
};

use super::{
    crypto::{
        encode_relationship_grant_secret, prove_relationship_grant, secret_from_material,
        verify_relationship_grant,
    },
    protocol::{
        PairingAck, PairingAckResponse, PairingConsent, PairingConsentResponse, PairingRequest,
        PairingRequestResponse, RelationshipClient, RevokeNotice, WireGrant, WireProof,
    },
    store::{state_as_str, DeviceRelationshipStore, RelationshipRow, RelationshipUpsert},
};

#[cfg(test)]
#[path = "../tests/device_relationship_service.rs"]
mod tests;

mod pairing;

const PENDING_TTL_MS: i64 = 30 * 60 * 1_000;

#[derive(Clone)]
pub(crate) struct DeviceRelationshipService {
    pub(super) store: DeviceRelationshipStore,
    pub(super) blocked: BlockStore,
    pub(super) custody: Option<Arc<SecretCustody>>,
    pub(super) eligibility: PairingEligibilityService,
    pub(super) event_hub: Arc<EventHub>,
    local_endpoint_id: String,
    endpoint: Endpoint,
    relay_mode: CoreRelayMode,
    custom_relay_urls: Vec<RelayUrl>,
    max_saved_devices: u64,
    pairing_timeout: Duration,
    peer_locks: Arc<TokioMutex<HashMap<String, Weak<TokioMutex<()>>>>>,
}

impl DeviceRelationshipService {
    #[allow(
        clippy::too_many_arguments,
        reason = "constructor wires custody, eligibility, endpoint, and network profile once"
    )]
    pub(crate) fn new(
        store: DeviceRelationshipStore,
        blocked: BlockStore,
        custody: Option<Arc<SecretCustody>>,
        eligibility: PairingEligibilityService,
        event_hub: Arc<EventHub>,
        local_endpoint_id: String,
        endpoint: Endpoint,
        relay_mode: CoreRelayMode,
        custom_relay_urls: Vec<RelayUrl>,
        max_saved_devices: u64,
        pairing_timeout_ms: u64,
    ) -> Self {
        Self {
            store,
            blocked,
            custody,
            eligibility,
            event_hub,
            local_endpoint_id,
            endpoint,
            relay_mode,
            custom_relay_urls,
            max_saved_devices,
            pairing_timeout: Duration::from_millis(pairing_timeout_ms),
            peer_locks: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    /// Slots consumed by Saved or in-flight mutual-consent relationships.
    async fn relationship_slots_used(&self) -> Result<u64, VnidropError> {
        self.store.count_active_slots().await
    }

    async fn can_create_new_relationship(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<bool, VnidropError> {
        if self.find_row(peer_endpoint_id).await?.is_some() {
            return Ok(true);
        }
        Ok(self.relationship_slots_used().await? < self.max_saved_devices)
    }

    pub(super) async fn lock_peer(&self, peer_endpoint_id: &str) -> Arc<TokioMutex<()>> {
        let mut locks = self.peer_locks.lock().await;
        if let Some(lock) = locks.get(peer_endpoint_id).and_then(Weak::upgrade) {
            return lock;
        }
        if locks.len() == locks.capacity() {
            locks.retain(|_, lock| lock.strong_count() > 0);
        }
        let lock = Arc::new(TokioMutex::new(()));
        locks.insert(peer_endpoint_id.to_owned(), Arc::downgrade(&lock));
        lock
    }

    pub(super) fn blocked_devices(&self) -> BlockStore {
        self.blocked.clone()
    }

    pub(super) async fn is_blocked(&self, endpoint_id: &str) -> bool {
        // Fail closed: a store error must not admit blocked traffic.
        self.blocked_devices()
            .is_blocked(endpoint_id)
            .await
            .unwrap_or(true)
    }

    /// Drop orphaned relationship grant secrets and disable rows whose secrets are gone.
    pub(crate) async fn reconcile(&self) -> Result<(), VnidropError> {
        let Some(custody) = &self.custody else {
            return Ok(());
        };
        let rows = self.store.list_reconcile_rows().await?;

        let mut live_handles = std::collections::HashSet::new();
        for row in rows {
            let peer = row.remote_endpoint_id;
            let state = row.state;
            let issued = row.issued_grant_handle;
            let held = row.held_grant_handle;
            let mut issued_missing = issued.is_none();
            if let Some(handle) = &issued {
                live_handles.insert(handle.clone());
                match custody
                    .load(&SecretHandle::from_stored(handle.clone()))
                    .await
                {
                    Ok(_) => {}
                    Err(VnidropError::SecureStorageMissing { .. }) => issued_missing = true,
                    Err(error) => return Err(error),
                }
            }
            if let Some(handle) = &held {
                live_handles.insert(handle.clone());
                // Held gaps after rotation are recoverable; orphaned handles are
                // still tracked so reconcile does not delete live custody rows.
                let _ = custody
                    .load(&SecretHandle::from_stored(handle.clone()))
                    .await;
            }
            // Issued grants authorize the peer. A missing held grant after
            // rotation is recoverable while the peer is offline.
            if state == DeviceRelationshipState::Saved && issued_missing {
                self.delete_relationship(&peer).await?;
            }
        }

        for handle in custody
            .list_active_handles(SecretKind::RelationshipGrant)
            .await?
        {
            if !live_handles.contains(handle.as_str()) {
                let _ = custody.remove(&handle).await;
            }
        }
        Ok(())
    }

    pub(crate) async fn list(&self) -> Result<Vec<DeviceRelationship>, VnidropError> {
        self.expire_pending().await?;
        self.store.list().await
    }

    pub(crate) async fn list_saved_devices(&self) -> Result<Vec<SavedDevice>, VnidropError> {
        self.store.list_saved_devices().await
    }

    /// Sets the user-owned local label for a Saved device. Labels are never
    /// overwritten by remote display names.
    pub(crate) async fn set_saved_device_label(
        &self,
        peer_endpoint_id: String,
        label: Option<String>,
    ) -> Result<(), VnidropError> {
        if !self
            .store
            .set_saved_device_label(&peer_endpoint_id, label)
            .await?
        {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "peer is not a saved device"
            )));
        }
        self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::Saved);
        Ok(())
    }

    pub(super) async fn mint_and_store_issued_grant(
        &self,
        peer_endpoint_id: &str,
        generation: u64,
        protocol_version: u16,
    ) -> Result<WireGrant, VnidropError> {
        let custody =
            self.custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "relationship grants require protected custody".to_string(),
                })?;
        let secret = GrantSecret::generate();
        let grant_id = GrantId::generate();
        let material = encode_relationship_grant_secret(&secret)?;
        let handle = custody
            .protect(SecretKind::RelationshipGrant, material, None)
            .await?;
        self.store
            .set_issued_grant(peer_endpoint_id, handle.as_str(), &grant_id.encode())
            .await?;
        Ok(WireGrant {
            grant_id: grant_id.encode(),
            secret: secret.encode(),
            issuer_endpoint_id: self.local_endpoint_id.clone(),
            holder_endpoint_id: peer_endpoint_id.to_string(),
            generation,
            protocol_version,
        })
    }

    async fn store_held_grant(
        &self,
        peer_endpoint_id: &str,
        grant: &WireGrant,
    ) -> Result<(), VnidropError> {
        if grant.holder_endpoint_id != self.local_endpoint_id
            || grant.issuer_endpoint_id != peer_endpoint_id
        {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "relationship grant endpoint binding mismatch"
            )));
        }
        let custody =
            self.custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "relationship grants require protected custody".to_string(),
                })?;
        let grant_id = GrantId::decode(&grant.grant_id).map_err(VnidropError::invalid_input)?;
        let secret = GrantSecret::decode(&grant.secret).map_err(VnidropError::invalid_input)?;
        let material = encode_relationship_grant_secret(&secret)?;
        let handle = custody
            .protect(SecretKind::RelationshipGrant, material, None)
            .await?;
        self.store
            .set_held_grant(peer_endpoint_id, handle.as_str(), &grant_id.encode())
            .await?;
        Ok(())
    }

    /// Prove possession of the held grant for a Saved peer (targeted offers).
    pub(crate) async fn prove_saved_possession(
        &self,
        peer_endpoint_id: &str,
        challenge: &Challenge,
    ) -> Result<(WireProof, u64, u16), VnidropError> {
        let row = self.find_row(peer_endpoint_id).await?.ok_or_else(|| {
            VnidropError::permission(anyhow::anyhow!("no saved relationship with peer"))
        })?;
        if row.state != DeviceRelationshipState::Saved {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "peer is not a saved device"
            )));
        }
        let proof = self
            .prove_held_possession(
                peer_endpoint_id,
                challenge,
                row.generation,
                row.minimum_protocol_version,
            )
            .await?;
        Ok((proof, row.generation, row.minimum_protocol_version))
    }

    /// Verify a Saved peer's held-grant proof against our issued grant.
    pub(crate) async fn verify_saved_possession(
        &self,
        peer_endpoint_id: &str,
        challenge: &Challenge,
        proof: &WireProof,
        generation: u64,
        protocol_version: u16,
    ) -> Result<(), VnidropError> {
        let row = self.find_row(peer_endpoint_id).await?.ok_or_else(|| {
            VnidropError::permission(anyhow::anyhow!("no saved relationship with peer"))
        })?;
        if row.state != DeviceRelationshipState::Saved {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "peer is not a saved device"
            )));
        }
        if row.generation != generation {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "relationship generation mismatch"
            )));
        }
        // Established relationships record a protocol floor and reject silent
        // downgrade attempts.
        if protocol_version < row.minimum_protocol_version {
            return Err(VnidropError::protocol_incompatible(anyhow::anyhow!(
                "relationship protocol downgrade is forbidden"
            )));
        }
        self.verify_issued_possession(
            peer_endpoint_id,
            challenge,
            proof,
            generation,
            protocol_version,
        )
        .await
    }

    pub(crate) async fn require_saved(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
        let row = self.find_row(peer_endpoint_id).await?.ok_or_else(|| {
            VnidropError::permission(anyhow::anyhow!("no saved relationship with peer"))
        })?;
        if row.state != DeviceRelationshipState::Saved {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "peer is not a saved device"
            )));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn force_minimum_protocol_version_for_test(
        &self,
        peer_endpoint_id: &str,
        minimum_protocol_version: u16,
    ) -> Result<(), VnidropError> {
        self.store
            .set_minimum_protocol_version(peer_endpoint_id, minimum_protocol_version)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn insert_tombstone_for_test(
        &self,
        peer_endpoint_id: &str,
        generation: u64,
    ) -> Result<(), VnidropError> {
        self.store
            .insert_tombstone_for_test(peer_endpoint_id, generation)
            .await
    }

    async fn prove_held_possession(
        &self,
        peer_endpoint_id: &str,
        challenge: &Challenge,
        generation: u64,
        protocol_version: u16,
    ) -> Result<WireProof, VnidropError> {
        let custody =
            self.custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "relationship grants require protected custody".to_string(),
                })?;
        let row = self.find_row(peer_endpoint_id).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("missing relationship for proof"))
        })?;
        let handle = row.held_grant_handle.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("missing held grant for proof"))
        })?;
        let grant_id = GrantId::decode(row.held_grant_id.as_deref().ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("missing held grant id"))
        })?)
        .map_err(VnidropError::invalid_input)?;
        let material = custody.load(&SecretHandle::from_stored(handle)).await?;
        let secret = secret_from_material(&material)?;
        let proof = prove_relationship_grant(
            grant_id,
            &secret,
            challenge,
            peer_endpoint_id,
            &self.local_endpoint_id,
            generation,
            protocol_version,
        );
        Ok(WireProof {
            grant_id: proof.grant_id.encode(),
            mac: HEXLOWER.encode(proof.mac()),
            challenge: challenge.encode(),
        })
    }

    async fn verify_issued_possession(
        &self,
        peer_endpoint_id: &str,
        challenge: &Challenge,
        proof: &WireProof,
        generation: u64,
        protocol_version: u16,
    ) -> Result<(), VnidropError> {
        if let Err(rejection) = self
            .reject_replayed_generation(peer_endpoint_id, generation, Some(proof.grant_id.as_str()))
            .await
        {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "relationship grant {}",
                rejection.as_str()
            )));
        }
        let custody =
            self.custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "relationship grants require protected custody".to_string(),
                })?;
        let row = self.find_row(peer_endpoint_id).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("missing relationship for verify"))
        })?;
        let handle = row.issued_grant_handle.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("missing issued grant for verify"))
        })?;
        let expected_id = row.issued_grant_id.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("missing issued grant id"))
        })?;
        if proof.grant_id != expected_id {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "relationship grant id mismatch"
            )));
        }
        let material = custody.load(&SecretHandle::from_stored(handle)).await?;
        let secret = secret_from_material(&material)?;
        let grant_id = GrantId::decode(&proof.grant_id).map_err(VnidropError::invalid_input)?;
        let mac_bytes = HEXLOWER
            .decode(proof.mac.as_bytes())
            .context("invalid proof mac")
            .map_err(VnidropError::invalid_input)?;
        let mac: [u8; 32] = mac_bytes.try_into().map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid proof mac length"))
        })?;
        let presented = GrantProof::from_parts(grant_id, mac);
        verify_relationship_grant(
            &secret,
            &presented,
            challenge,
            &self.local_endpoint_id,
            peer_endpoint_id,
            generation,
            protocol_version,
        )
        .map_err(|error| VnidropError::invalid_input(anyhow::anyhow!(error)))?;
        Ok(())
    }

    async fn set_acks(
        &self,
        peer_endpoint_id: &str,
        local_ack: bool,
        peer_ack: bool,
    ) -> Result<(), VnidropError> {
        self.store
            .set_acks(peer_endpoint_id, local_ack, peer_ack)
            .await
    }

    async fn activate_saved(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
        if !self.store.activate_saved(peer_endpoint_id).await? {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "both pairing grants are required before saving a device"
            )));
        }
        self.emit_changed(peer_endpoint_id, DeviceRelationshipState::Saved);
        Ok(())
    }

    async fn expire_pending(&self) -> Result<(), VnidropError> {
        let peers = self
            .store
            .list_expired_pending_peers(now_ms() - PENDING_TTL_MS)
            .await?;
        for peer in peers {
            self.delete_relationship(&peer).await?;
        }
        Ok(())
    }

    pub(super) async fn delete_relationship(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<(), VnidropError> {
        if let Ok(Some(row)) = self.find_row(peer_endpoint_id).await {
            if let Some(custody) = &self.custody {
                for handle in [row.issued_grant_handle, row.held_grant_handle]
                    .into_iter()
                    .flatten()
                {
                    let _ = custody.remove(&SecretHandle::from_stored(handle)).await;
                }
            }
        }
        self.store.delete(peer_endpoint_id).await
    }

    pub(super) async fn find_row(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Option<RelationshipRow>, VnidropError> {
        self.store.find_row(peer_endpoint_id).await
    }

    pub(crate) async fn peer_addr(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<EndpointAddr, VnidropError> {
        let parsed: EndpointId = peer_endpoint_id
            .parse()
            .context("unusable peer endpoint id")
            .map_err(VnidropError::invalid_input)?;
        let mut raw = EndpointAddr::from(parsed);
        if let Some(info) = self.endpoint.remote_info(parsed).await {
            raw.addrs = info.addrs().map(|entry| entry.addr().clone()).collect();
        }
        if raw.is_empty() {
            if let Some(stored) = self.store.saved_address(peer_endpoint_id).await? {
                let addr = crate::ticket::parse_persisted_sender_address(&stored)
                    .map_err(VnidropError::repository)?;
                if addr.id == parsed {
                    raw = addr;
                }
            }
        } else {
            let stored = encode_persisted_sender_address(&raw).map_err(VnidropError::repository)?;
            self.store
                .remember_address(peer_endpoint_id, &stored)
                .await?;
        }
        filter_peer_addr_for_relay_mode(&raw, self.relay_mode, &self.custom_relay_urls).map_err(
            |error| {
                VnidropError::relay_policy_incompatible(anyhow::anyhow!(
                    "peer address is unusable under the active network profile: {error}"
                ))
            },
        )
    }

    pub(super) fn emit_changed(&self, peer_endpoint_id: &str, state: DeviceRelationshipState) {
        self.event_hub.emit_endpoint(
            "pairing",
            "relationship-changed",
            json!({
                "peer_endpoint_id": peer_endpoint_id,
                "state": state_as_str(state),
            }),
        );
    }

    /// Best-effort signed/bound revocation notice; correctness never depends on delivery.
    pub(crate) async fn notify_remote_revoke(
        &self,
        peer_endpoint_id: &str,
        generation: u64,
        issued_grant_id: Option<String>,
    ) {
        let Ok(addr) = self.peer_addr(peer_endpoint_id).await else {
            return;
        };
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let _ = tokio::time::timeout(
            self.pairing_timeout,
            client.revoke_notice(RevokeNotice {
                generation,
                issued_grant_id,
            }),
        )
        .await;
    }
}
