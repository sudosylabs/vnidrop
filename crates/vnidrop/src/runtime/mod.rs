//! Core runtime orchestration for the VniDrop Iroh backend.
//!
//! Split by concern so send/receive/provider/lifecycle changes stay reviewable:
//! - [`facade`] — UniFFI `VnidropCore` surface and runtime entry
//! - [`share`] — import/share pipeline
//! - [`receive`] — ticket receive, download, export
//! - [`lifecycle`] — cancel/delete/shutdown/status/access
//! - [`provider`] — blob provider events and per-connection send progress
//! - [`saved_devices`] — experimental saved-device pairing, forget, block
//! - [`targeted`] — saved-device targeted transfers

mod delivery;
mod facade;
mod lifecycle;
mod provider;
mod receive;
mod saved_devices;
mod share;
mod storage;
mod targeted;

pub use facade::VnidropCore;
#[cfg(test)]
pub(crate) use provider::{consume_request_updates, RequestStreamOutcome};

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use anyhow::Result;
use futures_lite::StreamExt as _;
use iroh::{
    endpoint::presets, protocol::Router, tls::CaTlsConfig, Endpoint, EndpointAddr, RelayConfig,
    RelayMap, RelayMode, RelayUrl,
};
use iroh_blobs::{
    format::collection::Collection,
    provider::events::{EventMask, EventSender},
    store::{
        fs::{options::Options as FsStoreOptions, FsStore},
        GcConfig,
    },
    BlobsProtocol, Hash,
};
use serde_json::json;
use tokio::{
    sync::{oneshot, Mutex as TokioMutex, Notify, Semaphore},
    task::JoinHandle,
};

use crate::{
    access_policy::{mode_from_storage, AccessPolicy},
    api::{CoreEvent, CoreEventSink, CoreLimits, CoreRelayMode},
    approval::ApprovalService,
    device_relationship::{DeviceRelationshipService, RelationshipProtocol},
    event_hub::EventHub,
    handshake::HandshakeService,
    invitation::Repository,
    logging::init_logging,
    pairing_eligibility::PairingEligibilityService,
    secure_secret::{start_endpoint_identity, ProfileLock, SecureSecretStore},
    targeted_transfer::{TargetedOfferInbox, TargetedTransferProtocol},
    ticket::ticket_matches_relay_profile,
    transfer_state::{TransferDirection, TransferStatus},
};

const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelayStatus {
    Disabled,
    Connected,
    Unreachable,
}

impl RelayStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Connected => "connected",
            Self::Unreachable => "unreachable",
        }
    }
}

/// Owns the Iroh endpoint, blob store, transfer history, and byte streaming.
/// Kotlin owns app lifecycle and platform file picking.
pub(super) struct CoreInner {
    pub(super) app_data_dir: PathBuf,
    pub(super) endpoint: Endpoint,
    pub(super) router: Router,
    pub(super) store: FsStore,
    pub(super) repository: Repository,
    pub(super) targeted_transfers: crate::targeted_transfer::TargetedTransferStore,
    pub(super) blocked_devices: crate::blocked_devices::BlockStore,
    _profile_lock: Option<ProfileLock>,
    pub(super) secret_custody: Option<Arc<crate::secure_secret::SecretCustody>>,
    pub(super) event_hub: Arc<EventHub>,
    pub(super) approval: ApprovalService,
    pub(super) pairing_eligibility: PairingEligibilityService,
    pub(super) device_relationships: Arc<DeviceRelationshipService>,
    pub(super) targeted_offers: TargetedOfferInbox,
    pub(super) limits: CoreLimits,
    pub(super) relay_mode: CoreRelayMode,
    pub(super) custom_relay_urls: Vec<RelayUrl>,
    pub(super) transfer_slots: Semaphore,
    pub(super) access_policy: Arc<AccessPolicy>,
    /// Sync mutex so cancel can remove + signal without awaiting (and without
    /// holding a Tokio lock across repository I/O).
    pub(super) active_transfers: Arc<std::sync::Mutex<HashMap<u64, ActiveTransfer>>>,
    pub(super) active_targeted_transfers: Arc<std::sync::Mutex<HashMap<String, ActiveTransfer>>>,
    // Active shares are protected by persistent Iroh tags.
    pub(super) active_shares: TokioMutex<HashMap<u64, ()>>,
    /// Content hash → active share transfer ids (root and collection members).
    /// Multiple transfers can share the same content-addressed hash.
    pub(super) hash_to_transfer: Arc<TokioMutex<HashMap<String, HashSet<u64>>>>,
    pub(super) connection_endpoints: TokioMutex<HashMap<u64, String>>,
    pub(super) provider_task: TokioMutex<Option<JoinHandle<()>>>,
    pub(super) delivery_receipt_notify: Notify,
    pub(super) delivery_receipt_task: TokioMutex<Option<JoinHandle<()>>>,
    pub(super) targeted_completion_task: TokioMutex<Option<JoinHandle<()>>>,
    pub(super) shutdown_started: AtomicBool,
    /// Test-only log of peers passed to [`Self::cancel_targeted_transfers_for_peer`].
    #[cfg(test)]
    targeted_cancel_log: std::sync::Mutex<Vec<String>>,
    #[cfg(test)]
    suppress_targeted_completion: AtomicBool,
}

pub(super) struct ActiveTransfer {
    pub(super) direction: TransferDirection,
    pub(super) cancel: oneshot::Sender<()>,
}

pub(super) enum IdentityMode {
    Protected {
        store: Arc<dyn SecureSecretStore>,
        profile_lock: ProfileLock,
    },
}

impl CoreInner {
    pub(super) async fn start(
        app_data_dir: PathBuf,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
        relay_mode: CoreRelayMode,
        relay_urls: Vec<RelayUrl>,
        identity_mode: IdentityMode,
    ) -> Result<Arc<Self>> {
        tokio::fs::create_dir_all(&app_data_dir).await?;
        init_logging(&app_data_dir)?;
        let stores = crate::persistence::open_all(&app_data_dir).await?;
        let repository = stores.invitation.clone();
        let targeted_transfers = stores.targeted.clone();
        let blocked_devices = stores.blocked.clone();
        let (secret_key, secret_custody, profile_lock) = match identity_mode {
            IdentityMode::Protected {
                store,
                profile_lock,
            } => {
                let (secret_key, custody) = start_endpoint_identity(
                    stores.secrets.clone(),
                    store,
                    &app_data_dir.join("iroh.secret"),
                )
                .await?;
                (secret_key, Some(Arc::new(custody)), Some(profile_lock))
            }
        };
        let store_root = app_data_dir.join("blobs");
        let mut store_options = FsStoreOptions::new(&store_root);
        store_options.gc = Some(GcConfig {
            interval: Duration::from_secs(30 * 60),
            add_protected: None,
        });
        let store = FsStore::load_with_opts(store_root.join("blobs.db"), store_options).await?;
        let endpoint = match relay_mode {
            CoreRelayMode::Automatic => {
                Endpoint::builder(presets::N0)
                    .secret_key(secret_key)
                    .bind()
                    .await?
            }
            CoreRelayMode::StrictCustom | CoreRelayMode::CustomWithDirectFallback => {
                let relay_map = RelayMap::from_iter(relay_urls.iter().cloned().map(|url| {
                    // Loopback HTTP is a development escape hatch. Without TLS the
                    // relay cannot serve Iroh's QUIC address-discovery endpoint.
                    if url.scheme() == "http" {
                        RelayConfig::new(url, None)
                    } else {
                        RelayConfig::from(url)
                    }
                }));
                // Minimal leaves address lookup empty, so strict custom mode
                // cannot silently publish or resolve addresses through N0.
                Endpoint::builder(presets::Minimal)
                    .relay_mode(RelayMode::Custom(relay_map))
                    .ca_tls_config(CaTlsConfig::embedded())
                    .secret_key(secret_key)
                    .bind()
                    .await?
            }
            CoreRelayMode::LocalOnly => {
                Endpoint::builder(presets::Minimal)
                    .relay_mode(RelayMode::Disabled)
                    .secret_key(secret_key)
                    .bind()
                    .await?
            }
        };
        let relay_status =
            match wait_for_relay(&endpoint, relay_mode, &relay_urls, RELAY_CONNECT_TIMEOUT).await {
                Ok(status) => status,
                Err(error) => {
                    endpoint.close().await;
                    return Err(error);
                }
            };

        // Provider events are where the sender sees remote readers.  The core
        // uses them for send progress and for the current approval gate.
        let (events, event_rx) = EventSender::channel(128, EventMask::ALL_READONLY);
        let blobs = BlobsProtocol::new(&store, Some(events));
        let recovered_transfers = repository.recover_interrupted_transfers().await?;
        let event_hub = Arc::new(EventHub::start(
            repository.clone(),
            event_sink,
            limits.event_queue_capacity as usize,
            limits.max_events,
        ));
        event_hub.emit_endpoint(
            "network",
            "relay-status",
            json!({
                "mode": relay_mode_label(relay_mode),
                "status": relay_status.as_str(),
            }),
        );
        for recovered in recovered_transfers {
            event_hub.emit_transfer(
                recovered.transfer_id,
                recovered.direction.as_str(),
                "recovery",
                "interrupted-transfer-failed",
                json!({ "previous_status": recovered.previous_status.as_str() }),
            );
        }
        let expired_requests = repository
            .expire_pending_receiver_requests("application restarted before approval")
            .await?;
        if expired_requests > 0 {
            event_hub.emit_endpoint(
                "recovery",
                "pending-approvals-expired",
                json!({ "count": expired_requests }),
            );
        }
        let access_policy = AccessPolicy::new();
        // Restore share ownership and access mode before the router can serve
        // any request. Unknown persisted modes fail closed in mode_from_storage.
        // Register root + every collection member so child gets stay under ACL.
        let mut restored_hashes: HashMap<String, HashSet<u64>> = HashMap::new();
        let mut restored_active_shares = HashMap::new();
        let mut active_tag_names = HashSet::new();
        for share in repository.list_active_shares().await? {
            let transfer_id = share.transfer_id;
            let Ok(root_hash) = Hash::from_str(&share.content_hash) else {
                repository
                    .transition_transfer_status(
                        transfer_id,
                        TransferStatus::Sharing,
                        TransferStatus::Failed,
                    )
                    .await?;
                event_hub.emit_transfer(
                    transfer_id,
                    TransferDirection::Send.as_str(),
                    "recovery",
                    "share-root-missing-or-corrupt",
                    json!({ "content_hash": share.content_hash }),
                );
                continue;
            };
            let collection = if store.blobs().has(root_hash).await.unwrap_or(false) {
                Collection::load(root_hash, store.as_ref()).await.ok()
            } else {
                None
            };
            let Some(collection) = collection else {
                repository
                    .transition_transfer_status(
                        transfer_id,
                        TransferStatus::Sharing,
                        TransferStatus::Failed,
                    )
                    .await?;
                event_hub.emit_transfer(
                    transfer_id,
                    TransferDirection::Send.as_str(),
                    "recovery",
                    "share-root-missing-or-corrupt",
                    json!({ "content_hash": share.content_hash }),
                );
                continue;
            };
            let relay_profile_matches = share.ticket.as_deref().is_some_and(|ticket| {
                ticket_matches_relay_profile(ticket, &limits, relay_mode, &relay_urls)
                    .unwrap_or(false)
            });
            if !relay_profile_matches {
                repository
                    .transition_transfer_status(
                        transfer_id,
                        TransferStatus::Sharing,
                        TransferStatus::Stopped,
                    )
                    .await?;
                event_hub.emit_transfer(
                    transfer_id,
                    TransferDirection::Send.as_str(),
                    "recovery",
                    "share-stopped-network-profile-changed",
                    json!({ "reason": "saved ticket does not match the active relay profile" }),
                );
                continue;
            }
            let tag_name = share_tag_name(&share.local_id);
            store
                .tags()
                .set(&tag_name, (root_hash, iroh_blobs::BlobFormat::HashSeq))
                .await?;
            active_tag_names.insert(tag_name);
            restored_hashes
                .entry(root_hash.to_string())
                .or_default()
                .insert(transfer_id);
            for (_, member_hash) in collection.iter() {
                restored_hashes
                    .entry(member_hash.to_string())
                    .or_default()
                    .insert(transfer_id);
            }
            restored_active_shares.insert(transfer_id, ());
            access_policy
                .set_mode(transfer_id, mode_from_storage(&share.access_mode))
                .await;
        }
        let mut share_tags = store.tags().list_prefix("vnidrop/share/").await?;
        while let Some(tag) = share_tags.next().await {
            let tag = tag?;
            let name = String::from_utf8_lossy(tag.name.as_ref()).to_string();
            if !active_tag_names.contains(&name) {
                store.tags().delete(name).await?;
            }
        }
        let pairing_eligibility = PairingEligibilityService::new(
            stores.eligibility.clone(),
            stores.relationships.clone(),
            secret_custody.clone(),
            event_hub.clone(),
            endpoint.id().to_string(),
        );
        let approval = ApprovalService::new(
            repository.clone(),
            blocked_devices.clone(),
            event_hub.clone(),
            access_policy.clone(),
            limits.max_pending_approvals as usize,
            limits.max_metadata_bytes,
            Some(pairing_eligibility.clone()),
        );
        let handshake = HandshakeService::new(approval.clone());
        let identity_cooldown = crate::control_plane::IdentityCooldown::new(
            limits.identity_cooldown_ms,
            limits.malformed_strike_limit,
        );
        let targeted_offers = TargetedOfferInbox::new(
            event_hub.clone(),
            limits.max_pending_offers as usize,
            identity_cooldown,
            limits.offer_timeout_ms,
        );
        let device_relationships = Arc::new(DeviceRelationshipService::new(
            stores.relationships.clone(),
            stores.blocked.clone(),
            secret_custody.clone(),
            pairing_eligibility.clone(),
            event_hub.clone(),
            endpoint.id().to_string(),
            endpoint.clone(),
            relay_mode,
            relay_urls.clone(),
            limits.max_saved_devices,
            limits.pairing_timeout_ms,
        ));
        let active_transfers =
            Arc::new(std::sync::Mutex::new(HashMap::<u64, ActiveTransfer>::new()));
        let active_targeted_transfers = Arc::new(std::sync::Mutex::new(HashMap::<
            String,
            ActiveTransfer,
        >::new()));
        let hash_to_transfer = Arc::new(TokioMutex::new(restored_hashes));
        let cleanup_targeted = active_targeted_transfers.clone();
        let cleanup_hashes = hash_to_transfer.clone();
        let cleanup_store = store.clone();
        let cleanup_custody = secret_custody.clone();
        let cleanup_targeted_store = targeted_transfers.clone();
        let targeted_cleanup =
            Arc::new(move |row: crate::targeted_transfer::TargetedTransferRow| {
                let targeted = cleanup_targeted.clone();
                let hashes = cleanup_hashes.clone();
                let blobs = cleanup_store.clone();
                let custody = cleanup_custody.clone();
                let transfers = cleanup_targeted_store.clone();
                Box::pin(async move {
                    let active_targeted = targeted
                        .lock()
                        .expect("active_targeted_transfers")
                        .remove(&row.id);
                    if let Some(active_targeted) = active_targeted {
                        let _ = active_targeted.cancel.send(());
                    }
                    {
                        let mut hashes = hashes.lock().await;
                        hashes.retain(|_, owners| {
                            owners.remove(&row.protocol_transfer_id);
                            !owners.is_empty()
                        });
                    }
                    if row.role == crate::targeted_transfer::TargetedTransferRole::Sender {
                        blobs
                            .tags()
                            .delete(targeted_tag_name(&row.id))
                            .await
                            .map_err(crate::error::VnidropError::transfer)?;
                    } else if let Some(handle) = row.authorization_secret_handle {
                        if let Some(custody) = custody {
                            custody
                                .remove(&crate::secure_secret::SecretHandle::from_stored(handle))
                                .await?;
                        }
                        transfers.clear_authorization(&row.id).await?;
                    }
                    Ok(())
                }) as crate::targeted_transfer::protocol::TargetedCleanupFuture
            });
        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs)
            .accept(HandshakeService::ALPN, handshake)
            .accept(
                RelationshipProtocol::ALPN,
                RelationshipProtocol::new(device_relationships.clone()),
            )
            .accept(
                TargetedTransferProtocol::ALPN,
                TargetedTransferProtocol::new(
                    device_relationships.clone(),
                    targeted_offers.clone(),
                    targeted_transfers.clone(),
                    limits.clone(),
                    endpoint.id().to_string(),
                    relay_mode,
                    relay_urls.clone(),
                    event_hub.clone(),
                    access_policy.clone(),
                    targeted_cleanup,
                ),
            )
            .spawn();

        let inner = Arc::new(Self {
            app_data_dir,
            endpoint,
            router,
            store,
            repository,
            targeted_transfers,
            blocked_devices,
            _profile_lock: profile_lock,
            secret_custody: secret_custody.clone(),
            event_hub,
            approval,
            pairing_eligibility,
            device_relationships,
            targeted_offers,
            relay_mode,
            custom_relay_urls: relay_urls,
            transfer_slots: Semaphore::new(limits.max_concurrent_transfers as usize),
            limits,
            access_policy,
            active_transfers,
            active_targeted_transfers,
            active_shares: TokioMutex::new(restored_active_shares),
            hash_to_transfer,
            connection_endpoints: TokioMutex::new(HashMap::new()),
            provider_task: TokioMutex::new(None),
            delivery_receipt_notify: Notify::new(),
            delivery_receipt_task: TokioMutex::new(None),
            targeted_completion_task: TokioMutex::new(None),
            shutdown_started: AtomicBool::new(false),
            #[cfg(test)]
            targeted_cancel_log: std::sync::Mutex::new(Vec::new()),
            #[cfg(test)]
            suppress_targeted_completion: AtomicBool::new(false),
        });

        // In-flight connecting/transferring transfers become Interrupted across restart.
        match inner.targeted_store().mark_interrupted_in_flight().await {
            Ok(ids) => {
                for id in ids {
                    inner.emit_targeted_lifecycle(&id, "interrupted");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to mark in-flight targeted transfers interrupted");
            }
        }
        // Restore permanent receiver ACLs for approved targeted shares.
        if let Err(error) = inner.restore_targeted_transfer_access().await {
            tracing::warn!(%error, "failed to restore targeted transfer access");
        }

        inner.emit_endpoint(
            "startup",
            "endpoint-online",
            json!({
                "endpoint_id": inner.endpoint.id().to_string(),
                "addr": format!("{:?}", inner.endpoint.addr()),
                "store_root": store_root.to_string_lossy(),
            }),
        );
        inner.spawn_provider_event_task(event_rx).await;
        inner.spawn_delivery_receipt_task().await;
        inner.spawn_targeted_completion_task().await;
        if let Err(error) = inner.pairing_eligibility.reconcile().await {
            tracing::warn!(%error, "failed to reconcile pairing eligibility");
        }
        if let Err(error) = inner.device_relationships.reconcile().await {
            tracing::warn!(%error, "failed to reconcile device relationships");
        }
        Ok(inner)
    }

    pub(super) fn emit_endpoint(&self, phase: &str, kind: &str, data: serde_json::Value) {
        self.event_hub.emit_endpoint(phase, kind, data);
    }

    pub(super) fn emit_transfer(
        &self,
        transfer_id: u64,
        direction: &str,
        phase: &str,
        kind: &str,
        data: serde_json::Value,
    ) {
        self.event_hub
            .emit_transfer(transfer_id, direction, phase, kind, data);
    }

    pub(super) async fn register_share_hashes(
        &self,
        transfer_id: u64,
        hashes: impl IntoIterator<Item = Hash>,
    ) {
        let mut map = self.hash_to_transfer.lock().await;
        for hash in hashes {
            map.entry(hash.to_string()).or_default().insert(transfer_id);
        }
    }

    pub(super) async fn unregister_transfer_hashes(&self, transfer_id: u64) {
        let mut map = self.hash_to_transfer.lock().await;
        map.retain(|_, transfers| {
            transfers.remove(&transfer_id);
            !transfers.is_empty()
        });
    }

    pub(super) async fn list_events(&self, transfer_id: Option<u64>) -> Result<Vec<CoreEvent>> {
        self.event_hub.flush().await;
        self.repository
            .list_events(transfer_id, self.limits.max_events)
            .await
    }
}

pub(crate) fn filter_peer_addr_for_relay_mode(
    addr: &EndpointAddr,
    relay_mode: CoreRelayMode,
    custom_relay_urls: &[RelayUrl],
) -> Result<EndpointAddr> {
    crate::ticket::filter_peer_addr_for_relay_mode(addr, relay_mode, custom_relay_urls)
}

pub(crate) async fn wait_for_relay(
    endpoint: &Endpoint,
    relay_mode: CoreRelayMode,
    relay_urls: &[RelayUrl],
    timeout: Duration,
) -> Result<RelayStatus> {
    if relay_mode == CoreRelayMode::LocalOnly {
        return Ok(RelayStatus::Disabled);
    }
    if tokio::time::timeout(timeout, endpoint.online())
        .await
        .is_err()
    {
        match relay_mode {
            CoreRelayMode::StrictCustom => {
                let configured_relays = relay_urls
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "timed out after {} seconds while connecting to custom relays [{configured_relays}]; verify the URLs, TLS certificates, network access, and relay availability",
                    timeout.as_secs_f32(),
                );
            }
            CoreRelayMode::Automatic | CoreRelayMode::CustomWithDirectFallback => {
                return Ok(RelayStatus::Unreachable);
            }
            CoreRelayMode::LocalOnly => unreachable!(),
        }
    }
    Ok(RelayStatus::Connected)
}

fn relay_mode_label(relay_mode: CoreRelayMode) -> &'static str {
    match relay_mode {
        CoreRelayMode::Automatic => "automatic",
        CoreRelayMode::StrictCustom => "strict-custom",
        CoreRelayMode::CustomWithDirectFallback => "custom-with-direct-fallback",
        CoreRelayMode::LocalOnly => "local-only",
    }
}

pub(super) fn share_tag_name(local_id: &str) -> String {
    format!("vnidrop/share/{local_id}")
}

pub(super) fn targeted_tag_name(transfer_id: &str) -> String {
    format!("vnidrop/targeted/{transfer_id}")
}
