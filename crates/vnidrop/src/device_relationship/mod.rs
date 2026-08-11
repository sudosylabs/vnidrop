//! Experimental saved-device mutual-consent relationships.
//!
//! Implements design §6/§7: pending outgoing/incoming states, directional grants
//! bound to relationship generation, and Saved only after mutual acknowledgement.

use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use anyhow::Context;
use data_encoding::HEXLOWER;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr, EndpointId, RelayUrl,
};
use irpc::{channel::oneshot, rpc_requests, Client, WithChannels};
use irpc_iroh::{read_request, IrohLazyRemoteConnection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use tokio::sync::Mutex as TokioMutex;

use crate::{
    api::{
        experimental_saved_device_capabilities, CoreRelayMode, DeviceRelationship,
        DeviceRelationshipState, SavedDevice,
    },
    error::VnidropError,
    event_hub::EventHub,
    grant::{Challenge, GrantId, GrantProof, GrantSecret},
    pairing_eligibility::PairingEligibilityService,
    secure_secret::{SecretCustody, SecretHandle, SecretKind, SecretMaterial},
    ticket::{encode_persisted_sender_address, filter_peer_addr_for_relay_mode},
    util::now_ms,
};

mod crypto;
mod lifecycle;

#[cfg(test)]
pub(crate) use lifecycle::GenerationTombstone;

use crate::blocked_devices::BlockStore;
use crypto::{
    encode_relationship_grant_secret, prove_relationship_grant, secret_from_material,
    verify_relationship_grant,
};

const PENDING_TTL_MS: i64 = 30 * 60 * 1_000;

#[derive(Clone)]
pub(crate) struct DeviceRelationshipService {
    pool: SqlitePool,
    custody: Option<Arc<SecretCustody>>,
    eligibility: PairingEligibilityService,
    event_hub: Arc<EventHub>,
    local_endpoint_id: String,
    endpoint: Endpoint,
    relay_mode: CoreRelayMode,
    custom_relay_urls: Vec<RelayUrl>,
    max_saved_devices: u64,
    pairing_timeout: Duration,
    peer_locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>,
}

impl DeviceRelationshipService {
    #[allow(
        clippy::too_many_arguments,
        reason = "constructor wires custody, eligibility, endpoint, and network profile once"
    )]
    pub(crate) fn new(
        pool: SqlitePool,
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
            pool,
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
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS n FROM device_relationships
            WHERE state IN ('saved', 'pending_outgoing', 'pending_incoming')
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(row.get::<i64, _>("n") as u64)
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
        locks
            .entry(peer_endpoint_id.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    pub(crate) async fn ensure_schema(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_relationships (
                remote_endpoint_id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                generation INTEGER NOT NULL,
                minimum_protocol_version INTEGER NOT NULL,
                session_id TEXT,
                issued_grant_handle TEXT,
                held_grant_handle TEXT,
                issued_grant_id TEXT,
                held_grant_id TEXT,
                peer_ack INTEGER NOT NULL DEFAULT 0,
                local_ack INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            "#,
        )
        .execute(pool)
        .await?;
        let columns = sqlx::query("PRAGMA table_info(device_relationships)")
            .fetch_all(pool)
            .await?;
        let has = |name: &str| columns.iter().any(|row| row.get::<String, _>(1) == name);
        if !has("issued_grant_id") {
            sqlx::query("ALTER TABLE device_relationships ADD COLUMN issued_grant_id TEXT")
                .execute(pool)
                .await?;
        }
        if !has("held_grant_id") {
            sqlx::query("ALTER TABLE device_relationships ADD COLUMN held_grant_id TEXT")
                .execute(pool)
                .await?;
        }
        if !has("local_label") {
            sqlx::query("ALTER TABLE device_relationships ADD COLUMN local_label TEXT")
                .execute(pool)
                .await?;
        }
        Self::ensure_lifecycle_schema(pool).await?;
        Ok(())
    }

    pub(super) fn blocked_devices(&self) -> BlockStore {
        BlockStore::new(self.pool.clone())
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
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, issued_grant_handle, held_grant_handle, state
            FROM device_relationships
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;

        let mut live_handles = std::collections::HashSet::new();
        for row in rows {
            let peer: String = row.get("remote_endpoint_id");
            let state = parse_state(&row.get::<String, _>("state"))?;
            let issued: Option<String> = row.get("issued_grant_handle");
            let held: Option<String> = row.get("held_grant_handle");
            let mut issued_missing = issued.is_none();
            if let Some(handle) = &issued {
                live_handles.insert(handle.clone());
                if custody
                    .load(&SecretHandle::from_stored(handle.clone()))
                    .await
                    .is_err()
                {
                    issued_missing = true;
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
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, state, generation, minimum_protocol_version, created_at, updated_at
            FROM device_relationships
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_relationship).collect()
    }

    pub(crate) async fn list_saved_devices(&self) -> Result<Vec<SavedDevice>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, local_label, created_at, updated_at
            FROM device_relationships
            WHERE state = 'saved'
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows
            .into_iter()
            .map(|row| SavedDevice {
                endpoint_id: row.get("remote_endpoint_id"),
                local_label: row.get("local_label"),
                remote_display_name: None,
                created_at: row.get("created_at"),
                last_authenticated_at: Some(row.get("updated_at")),
            })
            .collect())
    }

    /// Sets the user-owned local label for a Saved device. Labels are never
    /// overwritten by remote display names.
    pub(crate) async fn set_saved_device_label(
        &self,
        peer_endpoint_id: String,
        label: Option<String>,
    ) -> Result<(), VnidropError> {
        let result = sqlx::query(
            r#"
            UPDATE device_relationships
            SET local_label = ?2, updated_at = ?3
            WHERE remote_endpoint_id = ?1 AND state = 'saved'
            "#,
        )
        .bind(&peer_endpoint_id)
        .bind(label)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "peer is not a saved device"
            )));
        }
        self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::Saved);
        Ok(())
    }

    pub(crate) async fn request_pairing(
        self: &Arc<Self>,
        peer_endpoint_id: String,
    ) -> Result<bool, VnidropError> {
        if self.is_blocked(&peer_endpoint_id).await {
            return Ok(false);
        }
        let peer_lock = self.lock_peer(&peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;

        if let Some(existing) = self.find_row(&peer_endpoint_id).await? {
            if existing.state == DeviceRelationshipState::Saved {
                return Ok(true);
            }
        }

        if !self.can_create_new_relationship(&peer_endpoint_id).await? {
            return Ok(false);
        }

        let Some(taken) = self.eligibility.take_eligibility(&peer_endpoint_id).await? else {
            return Ok(false);
        };
        let generation = 1_u64;
        let now = now_ms();

        // Peer already prompted us for the same qualifying session: merge into
        // one relationship without a second consent prompt.
        if let Some(existing) = self.find_row(&peer_endpoint_id).await? {
            if existing.state == DeviceRelationshipState::PendingIncoming
                && existing.session_id.as_deref() == Some(taken.session_id.as_str())
            {
                self.upsert_relationship(RelationshipUpsert {
                    remote_endpoint_id: &peer_endpoint_id,
                    state: DeviceRelationshipState::PendingOutgoing,
                    generation,
                    minimum_protocol_version: taken.protocol_version,
                    session_id: Some(&taken.session_id),
                    issued_grant_handle: None,
                    held_grant_handle: None,
                    issued_grant_id: None,
                    held_grant_id: None,
                    peer_ack: false,
                    local_ack: false,
                    created_at: existing.created_at,
                    updated_at: now,
                })
                .await?;
                self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::PendingOutgoing);
                drop(_guard);
                self.complete_simultaneous_merge(
                    &peer_endpoint_id,
                    generation,
                    taken.protocol_version,
                )
                .await?;
                return Ok(true);
            }
        }

        self.upsert_relationship(RelationshipUpsert {
            remote_endpoint_id: &peer_endpoint_id,
            state: DeviceRelationshipState::PendingOutgoing,
            generation,
            minimum_protocol_version: taken.protocol_version,
            session_id: Some(&taken.session_id),
            issued_grant_handle: None,
            held_grant_handle: None,
            issued_grant_id: None,
            held_grant_id: None,
            peer_ack: false,
            local_ack: false,
            created_at: now,
            updated_at: now,
        })
        .await?;
        self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::PendingOutgoing);
        drop(_guard);

        let addr = self.peer_addr(&peer_endpoint_id).await?;
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let response = match tokio::time::timeout(
            self.pairing_timeout,
            client.pairing_request(PairingRequest {
                session_id: taken.session_id.clone(),
                capability: taken.capability.to_vec(),
                protocol_version: taken.protocol_version,
                generation,
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            // Timeout/network failure keeps bounded PendingOutgoing for recovery.
            Ok(Err(_)) | Err(_) => return Ok(true),
        };

        match response {
            PairingRequestResponse::AwaitingConsent => Ok(true),
            PairingRequestResponse::Merged => {
                self.complete_simultaneous_merge(
                    &peer_endpoint_id,
                    generation,
                    taken.protocol_version,
                )
                .await?;
                Ok(true)
            }
            PairingRequestResponse::AlreadySaved => {
                self.activate_saved(&peer_endpoint_id).await?;
                Ok(true)
            }
            PairingRequestResponse::Rejected => {
                // Keep pending: a transient reject must not erase recoverable state.
                // Explicit decline is handled on the peer via consent=false.
                Ok(true)
            }
        }
    }

    pub(crate) async fn respond_to_pairing(
        self: &Arc<Self>,
        peer_endpoint_id: String,
        accepted: bool,
    ) -> Result<bool, VnidropError> {
        if self.is_blocked(&peer_endpoint_id).await {
            return Ok(false);
        }
        let Some(row) = self.find_row(&peer_endpoint_id).await? else {
            return Ok(false);
        };
        if row.state == DeviceRelationshipState::Saved {
            return Ok(true);
        }
        if row.state != DeviceRelationshipState::PendingIncoming {
            return Ok(false);
        }
        if !accepted {
            if let Some(session_id) = &row.session_id {
                let _ = self
                    .eligibility
                    .consume_session(&peer_endpoint_id, session_id)
                    .await;
            }
            self.delete_relationship(&peer_endpoint_id).await?;
            let addr = self.peer_addr(&peer_endpoint_id).await?;
            let client = RelationshipClient::connect(self.endpoint.clone(), addr);
            let _ = tokio::time::timeout(
                self.pairing_timeout,
                client.pairing_consent(PairingConsent {
                    accepted: false,
                    grant: None,
                    challenge: None,
                    generation: row.generation,
                    protocol_version: row.minimum_protocol_version,
                }),
            )
            .await;
            return Ok(true);
        }

        if let Some(session_id) = &row.session_id {
            let _ = self
                .eligibility
                .consume_session(&peer_endpoint_id, session_id)
                .await;
        }

        let grant = self
            .mint_and_store_issued_grant(
                &peer_endpoint_id,
                row.generation,
                row.minimum_protocol_version,
            )
            .await?;
        let challenge = Challenge::generate();
        let addr = self.peer_addr(&peer_endpoint_id).await?;
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let response = match tokio::time::timeout(
            self.pairing_timeout,
            client.pairing_consent(PairingConsent {
                accepted: true,
                grant: Some(grant.clone()),
                challenge: Some(challenge.encode()),
                generation: row.generation,
                protocol_version: row.minimum_protocol_version,
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            // Leave PendingIncoming so the operator can retry consent.
            Ok(Err(_)) | Err(_) => return Ok(true),
        };

        match response {
            PairingConsentResponse::Completed {
                grant: peer_grant,
                possession_proof,
                ack_challenge,
            } => {
                self.verify_issued_possession(
                    &peer_endpoint_id,
                    &challenge,
                    &possession_proof,
                    row.generation,
                    row.minimum_protocol_version,
                )
                .await?;
                self.store_held_grant(&peer_endpoint_id, &peer_grant)
                    .await?;
                let ack_challenge =
                    Challenge::decode(&ack_challenge).map_err(VnidropError::invalid_input)?;
                let proof = self
                    .prove_held_possession(
                        &peer_endpoint_id,
                        &ack_challenge,
                        peer_grant.generation,
                        peer_grant.protocol_version,
                    )
                    .await?;
                match tokio::time::timeout(
                    self.pairing_timeout,
                    client.pairing_ack(PairingAck {
                        possession_proof: proof,
                        challenge: ack_challenge.encode(),
                        generation: row.generation,
                        protocol_version: row.minimum_protocol_version,
                    }),
                )
                .await
                {
                    Ok(Ok(PairingAckResponse::Acknowledged | PairingAckResponse::AlreadySaved)) => {
                        self.set_acks(&peer_endpoint_id, true, true).await?;
                        self.activate_saved(&peer_endpoint_id).await?;
                        Ok(true)
                    }
                    // Ack lost/failed: grants are stored; pending remains recoverable.
                    _ => Ok(true),
                }
            }
            PairingConsentResponse::AlreadySaved => {
                self.activate_saved(&peer_endpoint_id).await?;
                Ok(true)
            }
            PairingConsentResponse::Rejected => Ok(false),
        }
    }

    async fn handle_pairing_request(
        self: &Arc<Self>,
        remote_endpoint_id: String,
        request: PairingRequest,
    ) -> PairingRequestResponse {
        if self.is_blocked(&remote_endpoint_id).await {
            // Indistinguishable rejection: do not expose block state.
            return PairingRequestResponse::Rejected;
        }
        let peer_lock = self.lock_peer(&remote_endpoint_id).await;
        let _guard = peer_lock.lock().await;

        if let Ok(Some(existing)) = self.find_row(&remote_endpoint_id).await {
            if existing.state == DeviceRelationshipState::Saved {
                return PairingRequestResponse::AlreadySaved;
            }
            // Simultaneous initiation: both sides proved eligibility for the same session.
            if existing.state == DeviceRelationshipState::PendingOutgoing
                && existing.session_id.as_deref() == Some(request.session_id.as_str())
            {
                let generation = existing.generation;
                let protocol_version = existing.minimum_protocol_version;
                let should_lead = self.local_endpoint_id.as_str() < remote_endpoint_id.as_str();
                drop(_guard);
                if should_lead {
                    let _ = self
                        .complete_simultaneous_merge(
                            &remote_endpoint_id,
                            generation,
                            protocol_version,
                        )
                        .await;
                }
                return PairingRequestResponse::Merged;
            }
        }

        let capability = match SecretMaterial::new(request.capability) {
            Ok(capability) => capability,
            Err(_) => return PairingRequestResponse::Rejected,
        };
        let local_protocol = experimental_saved_device_capabilities().relationship_protocol_version;
        // Peers without a compatible saved-device protocol cannot pair; they
        // retain ordinary invitation flow outside this ALPN.
        if request.protocol_version != local_protocol {
            return PairingRequestResponse::Rejected;
        }
        let accepted = match self
            .eligibility
            .validate_presented_capability(&remote_endpoint_id, &request.session_id, &capability)
            .await
        {
            Ok(Some(_)) => true,
            Ok(None) | Err(_) => false,
        };
        if !accepted {
            return PairingRequestResponse::Rejected;
        }

        match self.can_create_new_relationship(&remote_endpoint_id).await {
            Ok(true) => {}
            Ok(false) | Err(_) => return PairingRequestResponse::Rejected,
        }

        let now = now_ms();
        if self
            .upsert_relationship(RelationshipUpsert {
                remote_endpoint_id: &remote_endpoint_id,
                state: DeviceRelationshipState::PendingIncoming,
                generation: request.generation,
                minimum_protocol_version: request.protocol_version,
                session_id: Some(&request.session_id),
                issued_grant_handle: None,
                held_grant_handle: None,
                issued_grant_id: None,
                held_grant_id: None,
                peer_ack: false,
                local_ack: false,
                created_at: now,
                updated_at: now,
            })
            .await
            .is_err()
        {
            return PairingRequestResponse::Rejected;
        }
        self.emit_changed(
            &remote_endpoint_id,
            DeviceRelationshipState::PendingIncoming,
        );
        PairingRequestResponse::AwaitingConsent
    }

    async fn handle_pairing_consent(
        self: &Arc<Self>,
        remote_endpoint_id: String,
        consent: PairingConsent,
    ) -> PairingConsentResponse {
        if self.is_blocked(&remote_endpoint_id).await {
            return PairingConsentResponse::Rejected;
        }
        let Ok(Some(row)) = self.find_row(&remote_endpoint_id).await else {
            return PairingConsentResponse::Rejected;
        };
        if row.state == DeviceRelationshipState::Saved {
            return PairingConsentResponse::AlreadySaved;
        }
        if row.state != DeviceRelationshipState::PendingOutgoing {
            return PairingConsentResponse::Rejected;
        }
        if consent.protocol_version < row.minimum_protocol_version
            || consent.generation != row.generation
        {
            return PairingConsentResponse::Rejected;
        }
        if !consent.accepted {
            // Peer declined: clear local pending; eligibility was already consumed by requester.
            let _ = self.delete_relationship(&remote_endpoint_id).await;
            return PairingConsentResponse::Rejected;
        }
        let (Some(peer_grant), Some(challenge_hex)) = (consent.grant, consent.challenge) else {
            return PairingConsentResponse::Rejected;
        };
        let Ok(challenge) = Challenge::decode(&challenge_hex) else {
            return PairingConsentResponse::Rejected;
        };
        if self
            .store_held_grant(&remote_endpoint_id, &peer_grant)
            .await
            .is_err()
        {
            return PairingConsentResponse::Rejected;
        }
        let Ok(local_grant) = self
            .mint_and_store_issued_grant(
                &remote_endpoint_id,
                consent.generation,
                consent.protocol_version,
            )
            .await
        else {
            return PairingConsentResponse::Rejected;
        };
        let Ok(possession_proof) = self
            .prove_held_possession(
                &remote_endpoint_id,
                &challenge,
                peer_grant.generation,
                peer_grant.protocol_version,
            )
            .await
        else {
            return PairingConsentResponse::Rejected;
        };
        let ack_challenge = Challenge::generate();
        if self
            .set_acks(&remote_endpoint_id, false, false)
            .await
            .is_err()
        {
            return PairingConsentResponse::Rejected;
        }
        PairingConsentResponse::Completed {
            grant: Box::new(local_grant),
            possession_proof,
            ack_challenge: ack_challenge.encode(),
        }
    }

    async fn handle_pairing_ack(
        &self,
        remote_endpoint_id: String,
        ack: PairingAck,
    ) -> PairingAckResponse {
        if self.is_blocked(&remote_endpoint_id).await {
            return PairingAckResponse::Rejected;
        }
        let Ok(Some(row)) = self.find_row(&remote_endpoint_id).await else {
            return PairingAckResponse::Rejected;
        };
        if row.state == DeviceRelationshipState::Saved {
            return PairingAckResponse::AlreadySaved;
        }
        if row.state != DeviceRelationshipState::PendingOutgoing
            && row.state != DeviceRelationshipState::PendingIncoming
        {
            return PairingAckResponse::Rejected;
        }
        // Peer proves possession of our issued grant over the ack challenge we
        // sent in Completed; the challenge travels again on the ack message.
        let Ok(challenge) = Challenge::decode(&ack.challenge) else {
            return PairingAckResponse::Rejected;
        };
        if self
            .verify_issued_possession(
                &remote_endpoint_id,
                &challenge,
                &ack.possession_proof,
                ack.generation,
                ack.protocol_version,
            )
            .await
            .is_err()
        {
            return PairingAckResponse::Rejected;
        }
        if self
            .set_acks(&remote_endpoint_id, true, true)
            .await
            .is_err()
        {
            return PairingAckResponse::Rejected;
        }
        if self.activate_saved(&remote_endpoint_id).await.is_err() {
            return PairingAckResponse::Rejected;
        }
        PairingAckResponse::Acknowledged
    }

    async fn complete_simultaneous_merge(
        self: &Arc<Self>,
        peer_endpoint_id: &str,
        generation: u64,
        protocol_version: u16,
    ) -> Result<(), VnidropError> {
        let peer_lock = self.lock_peer(peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;
        if let Some(row) = self.find_row(peer_endpoint_id).await? {
            if row.state == DeviceRelationshipState::Saved {
                return Ok(());
            }
            // Another merge attempt already minted; do not rotate the issued grant.
            if row.issued_grant_handle.is_some() {
                return Ok(());
            }
        }
        // Deterministic role: smaller endpoint ID leads the consent+ack exchange.
        if self.local_endpoint_id.as_str() >= peer_endpoint_id {
            return Ok(());
        }
        let grant = self
            .mint_and_store_issued_grant(peer_endpoint_id, generation, protocol_version)
            .await?;
        let challenge = Challenge::generate();
        drop(_guard);
        let addr = self.peer_addr(peer_endpoint_id).await?;
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let response = match tokio::time::timeout(
            self.pairing_timeout,
            client.pairing_consent(PairingConsent {
                accepted: true,
                grant: Some(grant),
                challenge: Some(challenge.encode()),
                generation,
                protocol_version,
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) | Err(_) => return Ok(()),
        };
        match response {
            PairingConsentResponse::Completed {
                grant: peer_grant,
                possession_proof,
                ack_challenge,
            } => {
                self.verify_issued_possession(
                    peer_endpoint_id,
                    &challenge,
                    &possession_proof,
                    generation,
                    protocol_version,
                )
                .await?;
                self.store_held_grant(peer_endpoint_id, &peer_grant).await?;
                let ack_challenge =
                    Challenge::decode(&ack_challenge).map_err(VnidropError::invalid_input)?;
                let proof = self
                    .prove_held_possession(
                        peer_endpoint_id,
                        &ack_challenge,
                        peer_grant.generation,
                        peer_grant.protocol_version,
                    )
                    .await?;
                if let Ok(Ok(PairingAckResponse::Acknowledged | PairingAckResponse::AlreadySaved)) =
                    tokio::time::timeout(
                        self.pairing_timeout,
                        client.pairing_ack(PairingAck {
                            possession_proof: proof,
                            challenge: ack_challenge.encode(),
                            generation,
                            protocol_version,
                        }),
                    )
                    .await
                {
                    self.set_acks(peer_endpoint_id, true, true).await?;
                    self.activate_saved(peer_endpoint_id).await?;
                }
            }
            PairingConsentResponse::AlreadySaved => {
                self.activate_saved(peer_endpoint_id).await?;
            }
            PairingConsentResponse::Rejected => {}
        }
        Ok(())
    }

    async fn mint_and_store_issued_grant(
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
        sqlx::query(
            r#"
            UPDATE device_relationships
            SET issued_grant_handle = ?2, issued_grant_id = ?3, updated_at = ?4
            WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(handle.as_str())
        .bind(grant_id.encode())
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
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
        sqlx::query(
            r#"
            UPDATE device_relationships
            SET held_grant_handle = ?2, held_grant_id = ?3, updated_at = ?4
            WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(handle.as_str())
        .bind(grant_id.encode())
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
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
        // downgrade attempts (design §7 / §15).
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
        sqlx::query(
            "UPDATE device_relationships SET minimum_protocol_version = ?2, updated_at = ?3 WHERE remote_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .bind(i64::from(minimum_protocol_version))
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
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
        sqlx::query(
            "UPDATE device_relationships SET local_ack = ?2, peer_ack = ?3, updated_at = ?4 WHERE remote_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .bind(i64::from(local_ack))
        .bind(i64::from(peer_ack))
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    async fn activate_saved(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE device_relationships SET state = ?2, updated_at = ?3 WHERE remote_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .bind(state_as_str(DeviceRelationshipState::Saved))
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        self.emit_changed(peer_endpoint_id, DeviceRelationshipState::Saved);
        Ok(())
    }

    async fn expire_pending(&self) -> Result<(), VnidropError> {
        let cutoff = now_ms() - PENDING_TTL_MS;
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id FROM device_relationships
            WHERE state IN ('pending_outgoing', 'pending_incoming') AND updated_at < ?1
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        for row in rows {
            let peer: String = row.get(0);
            self.delete_relationship(&peer).await?;
        }
        Ok(())
    }

    async fn delete_relationship(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
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
        sqlx::query("DELETE FROM device_relationships WHERE remote_endpoint_id = ?1")
            .bind(peer_endpoint_id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    async fn find_row(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Option<RelationshipRow>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT remote_endpoint_id, state, generation, minimum_protocol_version, session_id,
                   issued_grant_handle, held_grant_handle, issued_grant_id, held_grant_id,
                   peer_ack, local_ack, created_at, updated_at
            FROM device_relationships WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(relationship_row_from_sql).transpose()
    }

    async fn upsert_relationship(&self, entry: RelationshipUpsert<'_>) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            INSERT INTO device_relationships (
                remote_endpoint_id, state, generation, minimum_protocol_version, session_id,
                issued_grant_handle, held_grant_handle, issued_grant_id, held_grant_id,
                peer_ack, local_ack, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(remote_endpoint_id) DO UPDATE SET
                state = excluded.state,
                generation = excluded.generation,
                minimum_protocol_version = excluded.minimum_protocol_version,
                session_id = excluded.session_id,
                issued_grant_handle = COALESCE(excluded.issued_grant_handle, device_relationships.issued_grant_handle),
                held_grant_handle = COALESCE(excluded.held_grant_handle, device_relationships.held_grant_handle),
                issued_grant_id = COALESCE(excluded.issued_grant_id, device_relationships.issued_grant_id),
                held_grant_id = COALESCE(excluded.held_grant_id, device_relationships.held_grant_id),
                peer_ack = excluded.peer_ack,
                local_ack = excluded.local_ack,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(entry.remote_endpoint_id)
        .bind(state_as_str(entry.state))
        .bind(entry.generation as i64)
        .bind(i64::from(entry.minimum_protocol_version))
        .bind(entry.session_id)
        .bind(entry.issued_grant_handle)
        .bind(entry.held_grant_handle)
        .bind(entry.issued_grant_id)
        .bind(entry.held_grant_id)
        .bind(i64::from(entry.peer_ack))
        .bind(i64::from(entry.local_ack))
        .bind(entry.created_at)
        .bind(entry.updated_at)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn peer_addr(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<EndpointAddr, VnidropError> {
        let parsed: EndpointId = peer_endpoint_id
            .parse()
            .context("unusable peer endpoint id")
            .map_err(VnidropError::invalid_input)?;
        let raw = if let Some(info) = self.endpoint.remote_info(parsed).await {
            let mut addr = EndpointAddr::from(parsed);
            addr.addrs = info.addrs().map(|entry| entry.addr().clone()).collect();
            let _ = encode_persisted_sender_address(&addr);
            addr
        } else {
            EndpointAddr::from(parsed)
        };
        filter_peer_addr_for_relay_mode(&raw, self.relay_mode, &self.custom_relay_urls).map_err(
            |error| {
                VnidropError::relay_policy_incompatible(anyhow::anyhow!(
                    "peer address is unusable under the active network profile: {error}"
                ))
            },
        )
    }

    fn emit_changed(&self, peer_endpoint_id: &str, state: DeviceRelationshipState) {
        self.event_hub.emit_endpoint(
            "pairing",
            "relationship-changed",
            json!({
                "peer_endpoint_id": peer_endpoint_id,
                "state": state_as_str(state),
            }),
        );
    }
}

#[derive(Clone)]
pub(crate) struct RelationshipProtocol {
    relationships: Arc<DeviceRelationshipService>,
}

impl RelationshipProtocol {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/relationship/1";

    pub(crate) fn new(relationships: Arc<DeviceRelationshipService>) -> Self {
        Self { relationships }
    }
}

impl fmt::Debug for RelationshipProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RelationshipProtocol")
    }
}

impl ProtocolHandler for RelationshipProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        while let Some(message) = read_request::<RelationshipMessages>(&connection).await? {
            match message {
                RelationshipMessage::PairingRequest(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .relationships
                        .handle_pairing_request(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                RelationshipMessage::PairingConsent(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .relationships
                        .handle_pairing_consent(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                RelationshipMessage::PairingAck(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .relationships
                        .handle_pairing_ack(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                RelationshipMessage::RevokeNotice(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let acknowledged = self
                        .relationships
                        .handle_remote_revoke(remote_endpoint_id.clone(), inner.generation)
                        .await;
                    let response = if acknowledged {
                        RevokeNoticeResponse::Acknowledged
                    } else {
                        RevokeNoticeResponse::Rejected
                    };
                    let _ = tx.send(response).await;
                }
            }
        }
        connection.closed().await;
        Ok(())
    }
}

struct RelationshipClient {
    inner: Client<RelationshipMessages>,
}

impl RelationshipClient {
    fn connect(endpoint: Endpoint, addr: EndpointAddr) -> Self {
        Self {
            inner: Client::boxed(IrohLazyRemoteConnection::new(
                endpoint,
                addr,
                RelationshipProtocol::ALPN.to_vec(),
            )),
        }
    }

    async fn pairing_request(
        &self,
        request: PairingRequest,
    ) -> Result<PairingRequestResponse, irpc::Error> {
        self.inner.rpc(request).await
    }

    async fn pairing_consent(
        &self,
        consent: PairingConsent,
    ) -> Result<PairingConsentResponse, irpc::Error> {
        self.inner.rpc(consent).await
    }

    async fn pairing_ack(&self, ack: PairingAck) -> Result<PairingAckResponse, irpc::Error> {
        self.inner.rpc(ack).await
    }

    async fn revoke_notice(
        &self,
        notice: RevokeNotice,
    ) -> Result<RevokeNoticeResponse, irpc::Error> {
        self.inner.rpc(notice).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairingRequest {
    session_id: String,
    capability: Vec<u8>,
    protocol_version: u16,
    generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum PairingRequestResponse {
    AwaitingConsent,
    Merged,
    AlreadySaved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairingConsent {
    accepted: bool,
    grant: Option<WireGrant>,
    challenge: Option<String>,
    generation: u64,
    protocol_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PairingConsentResponse {
    Completed {
        grant: Box<WireGrant>,
        possession_proof: WireProof,
        ack_challenge: String,
    },
    AlreadySaved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairingAck {
    possession_proof: WireProof,
    challenge: String,
    generation: u64,
    protocol_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum PairingAckResponse {
    Acknowledged,
    AlreadySaved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireGrant {
    grant_id: String,
    secret: String,
    issuer_endpoint_id: String,
    holder_endpoint_id: String,
    generation: u64,
    protocol_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WireProof {
    pub(crate) grant_id: String,
    pub(crate) mac: String,
    pub(crate) challenge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RevokeNotice {
    generation: u64,
    issued_grant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum RevokeNoticeResponse {
    Acknowledged,
    Rejected,
}

#[rpc_requests(message = RelationshipMessage)]
#[derive(Debug, Serialize, Deserialize)]
#[allow(
    clippy::enum_variant_names,
    reason = "Pairing* names mirror the wire RPC surface"
)]
enum RelationshipMessages {
    #[rpc(tx = oneshot::Sender<PairingRequestResponse>)]
    PairingRequest(PairingRequest),
    #[rpc(tx = oneshot::Sender<PairingConsentResponse>)]
    PairingConsent(PairingConsent),
    #[rpc(tx = oneshot::Sender<PairingAckResponse>)]
    PairingAck(PairingAck),
    #[rpc(tx = oneshot::Sender<RevokeNoticeResponse>)]
    RevokeNotice(RevokeNotice),
}

struct RelationshipUpsert<'a> {
    remote_endpoint_id: &'a str,
    state: DeviceRelationshipState,
    generation: u64,
    minimum_protocol_version: u16,
    session_id: Option<&'a str>,
    issued_grant_handle: Option<&'a str>,
    held_grant_handle: Option<&'a str>,
    issued_grant_id: Option<&'a str>,
    held_grant_id: Option<&'a str>,
    peer_ack: bool,
    local_ack: bool,
    created_at: i64,
    updated_at: i64,
}

struct RelationshipRow {
    state: DeviceRelationshipState,
    generation: u64,
    minimum_protocol_version: u16,
    session_id: Option<String>,
    issued_grant_handle: Option<String>,
    held_grant_handle: Option<String>,
    issued_grant_id: Option<String>,
    held_grant_id: Option<String>,
    created_at: i64,
}

impl DeviceRelationshipService {
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

fn state_as_str(state: DeviceRelationshipState) -> &'static str {
    match state {
        DeviceRelationshipState::PendingOutgoing => "pending_outgoing",
        DeviceRelationshipState::PendingIncoming => "pending_incoming",
        DeviceRelationshipState::Saved => "saved",
        DeviceRelationshipState::Revoked => "revoked",
        DeviceRelationshipState::Blocked => "blocked",
    }
}

fn parse_state(value: &str) -> Result<DeviceRelationshipState, VnidropError> {
    match value {
        "pending_outgoing" => Ok(DeviceRelationshipState::PendingOutgoing),
        "pending_incoming" => Ok(DeviceRelationshipState::PendingIncoming),
        "saved" => Ok(DeviceRelationshipState::Saved),
        "revoked" => Ok(DeviceRelationshipState::Revoked),
        "blocked" => Ok(DeviceRelationshipState::Blocked),
        _ => Err(VnidropError::Internal {
            reason: "unknown device relationship state".to_string(),
        }),
    }
}

fn row_to_relationship(row: sqlx::sqlite::SqliteRow) -> Result<DeviceRelationship, VnidropError> {
    Ok(DeviceRelationship {
        remote_endpoint_id: row.get("remote_endpoint_id"),
        state: parse_state(&row.get::<String, _>("state"))?,
        generation: row.get::<i64, _>("generation") as u64,
        minimum_protocol_version: row.get::<i64, _>("minimum_protocol_version") as u16,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn relationship_row_from_sql(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RelationshipRow, VnidropError> {
    Ok(RelationshipRow {
        state: parse_state(&row.get::<String, _>("state"))?,
        generation: row.get::<i64, _>("generation") as u64,
        minimum_protocol_version: row.get::<i64, _>("minimum_protocol_version") as u16,
        session_id: row.get("session_id"),
        issued_grant_handle: row.get("issued_grant_handle"),
        held_grant_handle: row.get("held_grant_handle"),
        issued_grant_id: row.get("issued_grant_id"),
        held_grant_id: row.get("held_grant_id"),
        created_at: row.get("created_at"),
    })
}
