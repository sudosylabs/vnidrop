use std::{future::Future, path::PathBuf, sync::Arc};

use anyhow::Context;
use serde_json::json;

use super::{CoreInner, IdentityMode};
use crate::{
    api::{
        ContactSendResult, ContactSummary, CoreEvent, CoreEventSink, CoreLimits, CoreNetworkConfig,
        CoreStorageUsage, GrantLifetimeSetting, HeldOfferSummary, IncomingOffer, PendingPairing,
        ReceiveOutputSink, ReceiveOutputSinkV2, ReceivedArtifact, ReceiverRequest, RuntimeStatus,
        ShareMetadataInput, ShareResult, ShareSource, StoredTransfer, TicketInspection,
        TransferAccessMode,
    },
    error::VnidropError,
    filesystem::platform_path,
    secure_secret::{lock_profile, platform_secret_store},
    ticket::parse_transfer_ticket_with_limits,
    transfer_state::{TransferDirection, TransferStatus},
};

#[derive(uniffi::Object)]
pub struct VnidropCore {
    runtime: tokio::runtime::Runtime,
    inner: Arc<CoreInner>,
}

impl VnidropCore {
    /// Drive work on this core's multi-thread runtime from a sync API boundary.
    ///
    /// Uses [`tokio::runtime::Handle::block_on`] rather than exclusive
    /// [`Runtime::block_on`] so a concurrent call (for example cancel while
    /// another thread is blocked inside `receive`) cannot deadlock the runtime
    /// driver. UniFFI and tests both rely on that: receive runs on a worker
    /// thread while cancel/approve arrive from the UI or test harness thread.
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.handle().block_on(future)
    }

    fn initialize_with_identity_mode(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
        network_config: CoreNetworkConfig,
        identity_mode: IdentityMode,
    ) -> Result<Arc<Self>, VnidropError> {
        limits.validate().map_err(VnidropError::initialization)?;
        let relay_urls = network_config
            .validated_relay_urls()
            .map_err(VnidropError::initialization)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vnidrop")
            .build()?;
        let inner = runtime
            .block_on(CoreInner::start(
                PathBuf::from(app_data_dir),
                event_sink,
                limits,
                network_config.mode,
                relay_urls,
                identity_mode,
            ))
            .map_err(VnidropError::initialization)?;
        Ok(Arc::new(Self { runtime, inner }))
    }
}

#[uniffi::export]
impl VnidropCore {
    #[uniffi::constructor]
    pub fn initialize(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
    ) -> Result<Arc<Self>, VnidropError> {
        Self::initialize_with_limits_and_network_config(
            app_data_dir,
            event_sink,
            CoreLimits::default(),
            CoreNetworkConfig::default(),
        )
    }

    #[uniffi::constructor]
    pub fn initialize_with_network_config(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        network_config: CoreNetworkConfig,
    ) -> Result<Arc<Self>, VnidropError> {
        Self::initialize_with_limits_and_network_config(
            app_data_dir,
            event_sink,
            CoreLimits::default(),
            network_config,
        )
    }

    #[uniffi::constructor]
    pub fn initialize_with_limits(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
    ) -> Result<Arc<Self>, VnidropError> {
        Self::initialize_with_limits_and_network_config(
            app_data_dir,
            event_sink,
            limits,
            CoreNetworkConfig::default(),
        )
    }

    #[uniffi::constructor]
    pub fn initialize_with_limits_and_network_config(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
        network_config: CoreNetworkConfig,
    ) -> Result<Arc<Self>, VnidropError> {
        Self::initialize_with_identity_mode(
            app_data_dir,
            event_sink,
            limits,
            network_config,
            IdentityMode::Legacy,
        )
    }

    /// Starts the experimental saved-device core with a platform-protected identity.
    #[uniffi::constructor]
    pub fn initialize_with_experimental_saved_devices(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
        network_config: CoreNetworkConfig,
    ) -> Result<Arc<Self>, VnidropError> {
        let app_data_path = PathBuf::from(app_data_dir);
        std::fs::create_dir_all(&app_data_path).map_err(VnidropError::filesystem)?;
        let app_data_path =
            std::fs::canonicalize(app_data_path).map_err(VnidropError::filesystem)?;
        let profile_lock = lock_profile(&app_data_path)?;
        let store = platform_secret_store(&app_data_path)?;
        Self::initialize_with_identity_mode(
            app_data_path.to_string_lossy().into_owned(),
            event_sink,
            limits,
            network_config,
            IdentityMode::Protected {
                store,
                profile_lock,
            },
        )
    }

    pub fn status(&self) -> RuntimeStatus {
        self.block_on(self.inner.status())
    }

    pub fn share_files(
        &self,
        sources: Vec<ShareSource>,
        metadata: ShareMetadataInput,
    ) -> Result<ShareResult, VnidropError> {
        self.block_on(self.inner.share_files(sources, metadata))
            .map_err(VnidropError::transfer)
    }

    pub fn receive(
        &self,
        ticket: String,
        output_dir: String,
        receiver_name: Option<String>,
    ) -> Result<(), VnidropError> {
        if let Err(error) = parse_transfer_ticket_with_limits(&ticket, &self.inner.limits)
            .context("failed to parse transfer ticket")
        {
            self.block_on(async {
                self.inner.emit_endpoint(
                    "error",
                    "invalid-ticket",
                    json!({ "reason": error.to_string() }),
                );
                self.inner.event_hub.flush().await;
            });
            return Err(VnidropError::ticket(error));
        }
        let output_dir = platform_path(&output_dir).map_err(VnidropError::filesystem)?;
        self.block_on(self.inner.receive(ticket, output_dir, receiver_name))
            .map_err(VnidropError::transfer)
    }

    pub fn receive_with_output_sink(
        &self,
        ticket: String,
        output_sink: Arc<dyn ReceiveOutputSink>,
        receiver_name: Option<String>,
    ) -> Result<(), VnidropError> {
        if let Err(error) = parse_transfer_ticket_with_limits(&ticket, &self.inner.limits)
            .context("failed to parse transfer ticket")
        {
            self.block_on(async {
                self.inner.emit_endpoint(
                    "error",
                    "invalid-ticket",
                    json!({ "reason": error.to_string() }),
                );
                self.inner.event_hub.flush().await;
            });
            return Err(VnidropError::ticket(error));
        }
        self.block_on(
            self.inner
                .receive_with_output_sink(ticket, output_sink, receiver_name),
        )
        .map_err(VnidropError::transfer)
    }

    pub fn receive_with_output_sink_v2(
        &self,
        ticket: String,
        output_sink: Arc<dyn ReceiveOutputSinkV2>,
        receiver_name: Option<String>,
    ) -> Result<(), VnidropError> {
        if let Err(error) = parse_transfer_ticket_with_limits(&ticket, &self.inner.limits)
            .context("failed to parse transfer ticket")
        {
            return Err(VnidropError::ticket(error));
        }
        self.block_on(
            self.inner
                .receive_with_output_sink_v2(ticket, output_sink, receiver_name),
        )
        .map_err(VnidropError::transfer)
    }

    pub fn cancel_transfer(&self, transfer_id: u64) -> Result<(), VnidropError> {
        // Fire the oneshot on this thread before entering the runtime so a
        // blocked export write cannot prevent the cancel signal from being
        // delivered (the receive `select!` observes it on the next yield).
        if let Some(direction) = self.inner.take_active_transfer(transfer_id) {
            let expected = match direction {
                TransferDirection::Send => TransferStatus::Importing,
                TransferDirection::Receive => TransferStatus::Receiving,
            };
            return self
                .block_on(async {
                    self.inner
                        .repository
                        .transition_transfer_status(
                            transfer_id,
                            expected,
                            TransferStatus::Cancelled,
                        )
                        .await?;
                    self.inner.emit_transfer(
                        transfer_id,
                        direction.as_str(),
                        "lifecycle",
                        "cancel-requested",
                        json!({}),
                    );
                    Ok::<(), anyhow::Error>(())
                })
                .map_err(VnidropError::transfer);
        }
        self.block_on(self.inner.cancel_idle_or_share(transfer_id))
            .map_err(VnidropError::transfer)
    }

    pub fn delete_transfer(&self, transfer_id: u64) -> Result<(), VnidropError> {
        self.block_on(self.inner.delete_transfer(transfer_id))
            .map_err(VnidropError::transfer)
    }

    pub fn delete_receive_history(&self) -> Result<u64, VnidropError> {
        self.block_on(self.inner.delete_receive_history())
            .map_err(VnidropError::repository)
    }

    pub fn set_transfer_access_mode(
        &self,
        transfer_id: u64,
        mode: TransferAccessMode,
    ) -> Result<(), VnidropError> {
        self.block_on(self.inner.set_transfer_access_mode(transfer_id, mode))
            .map_err(VnidropError::permission)
    }

    pub fn approve_endpoint_for_transfer(
        &self,
        transfer_id: u64,
        endpoint_id: String,
    ) -> Result<(), VnidropError> {
        self.block_on(
            self.inner
                .approve_endpoint_for_transfer(transfer_id, endpoint_id),
        )
        .map_err(VnidropError::permission)
    }

    pub fn list_receiver_requests(
        &self,
        transfer_id: u64,
    ) -> Result<Vec<ReceiverRequest>, VnidropError> {
        self.block_on(self.inner.repository.list_receiver_requests(transfer_id))
            .map_err(VnidropError::repository)
    }

    pub fn respond_receiver_request(
        &self,
        request_id: String,
        accepted: bool,
        reason: Option<String>,
    ) -> Result<(), VnidropError> {
        self.block_on(self.inner.approval.respond(request_id, accepted, reason))
            .map_err(VnidropError::permission)
    }

    /// Devices the user has chosen to remember.
    pub fn list_contacts(&self) -> Result<Vec<ContactSummary>, VnidropError> {
        self.block_on(self.inner.list_contacts())
            .map_err(VnidropError::repository)
    }

    /// Share content and push it straight to a paired device.
    ///
    /// Only the receiving user is prompted: this device authorised the target
    /// when it created the offer.
    pub fn send_to_contact(
        &self,
        endpoint_id: String,
        sources: Vec<ShareSource>,
        metadata: ShareMetadataInput,
    ) -> Result<ContactSendResult, VnidropError> {
        self.block_on(self.inner.send_to_contact(endpoint_id, sources, metadata))
            .map_err(VnidropError::transfer)
    }

    /// Ask remembered devices whether they are holding transfers for this one.
    ///
    /// Call only from a foreground transition or an explicit user action: it
    /// tells every contact that this device is awake. Returns how many offers
    /// were collected.
    pub fn poll_contacts_for_offers(&self) -> Result<u64, VnidropError> {
        self.block_on(self.inner.poll_contacts_for_offers())
            .map_err(VnidropError::transfer)
    }

    /// Offer an existing share to a remembered device.
    ///
    /// Another way to deliver the invitation already created for a transfer,
    /// alongside the QR code — not a second share of the same files.
    pub fn offer_transfer_to_contact(
        &self,
        transfer_id: u64,
        endpoint_id: String,
    ) -> Result<ContactSendResult, VnidropError> {
        self.block_on(
            self.inner
                .offer_transfer_to_contact(transfer_id, endpoint_id),
        )
        .map_err(VnidropError::transfer)
    }

    /// Transfers this device is holding for contacts that were not running.
    pub fn list_held_offers(&self) -> Result<Vec<HeldOfferSummary>, VnidropError> {
        self.block_on(self.inner.list_held_offers())
            .map_err(VnidropError::repository)
    }

    /// Transfers paired devices are offering, awaiting this user's decision.
    pub fn list_pending_offers(&self) -> Vec<IncomingOffer> {
        self.block_on(self.inner.list_pending_offers())
    }

    /// Accept or decline an incoming offer.
    ///
    /// Returns the ticket when accepted, which the caller passes to `receive`
    /// with its own destination. Declining returns none: a refused offer never
    /// yields a capability.
    pub fn respond_to_offer(&self, offer_id: String, accepted: bool) -> Option<String> {
        self.block_on(self.inner.respond_to_offer(offer_id, accepted))
    }

    /// Devices offering to be remembered, awaiting the local user's decision.
    pub fn list_pending_pairings(&self) -> Vec<PendingPairing> {
        self.block_on(self.inner.list_pending_pairings())
    }

    /// Agree to be reachable by a device, handing it a revocable capability.
    ///
    /// Independent of whether that device agrees to be reachable by us: each
    /// direction is a separate decision.
    pub fn allow_device_to_reach_me(
        &self,
        endpoint_id: String,
        display_name: Option<String>,
    ) -> Result<(), VnidropError> {
        self.block_on(
            self.inner
                .allow_device_to_reach_me(endpoint_id, display_name),
        )
        .map_err(VnidropError::transfer)
    }

    /// Accept or decline a device's offer to be remembered. Returns false when
    /// the offer already lapsed.
    pub fn respond_to_pairing(
        &self,
        endpoint_id: String,
        accepted: bool,
    ) -> Result<bool, VnidropError> {
        self.block_on(self.inner.respond_to_pairing(endpoint_id, accepted))
            .map_err(VnidropError::repository)
    }

    /// Forget a device and revoke its access. Takes effect locally at once; the
    /// peer is notified best effort.
    pub fn forget_contact(&self, endpoint_id: String) -> Result<(), VnidropError> {
        self.block_on(self.inner.forget_contact(endpoint_id))
            .map_err(VnidropError::repository)
    }

    /// Forget every device at once. Returns how many grants were revoked.
    pub fn forget_all_contacts(&self) -> Result<u64, VnidropError> {
        self.block_on(self.inner.forget_all_contacts())
            .map_err(VnidropError::repository)
    }

    /// Refuse a device outright. Unlike forgetting, the peer is told nothing.
    pub fn block_contact(&self, endpoint_id: String) -> Result<(), VnidropError> {
        self.block_on(self.inner.block_contact(endpoint_id))
            .map_err(VnidropError::repository)
    }

    pub fn unblock_contact(&self, endpoint_id: String) -> Result<(), VnidropError> {
        self.block_on(self.inner.unblock_contact(endpoint_id))
            .map_err(VnidropError::repository)
    }

    pub fn list_blocked_contacts(&self) -> Result<Vec<String>, VnidropError> {
        self.block_on(self.inner.list_blocked_contacts())
            .map_err(VnidropError::repository)
    }

    pub fn set_contact_label(
        &self,
        endpoint_id: String,
        label: Option<String>,
    ) -> Result<(), VnidropError> {
        self.block_on(self.inner.set_contact_label(endpoint_id, label))
            .map_err(VnidropError::repository)
    }

    /// Idle lifetime applied to grants issued from now on. Existing grants keep
    /// the lifetime they were issued with until they next renew.
    pub fn set_grant_lifetime(&self, lifetime: GrantLifetimeSetting) {
        self.block_on(self.inner.set_grant_lifetime(lifetime));
    }

    pub fn list_transfers(&self) -> Result<Vec<StoredTransfer>, VnidropError> {
        self.block_on(self.inner.repository.list_transfers())
            .map_err(VnidropError::repository)
    }

    pub fn list_received_artifacts(&self) -> Result<Vec<ReceivedArtifact>, VnidropError> {
        self.block_on(self.inner.repository.list_received_artifacts())
            .map_err(VnidropError::repository)
    }

    pub fn storage_usage(&self) -> Result<CoreStorageUsage, VnidropError> {
        self.block_on(self.inner.storage_usage())
            .map_err(VnidropError::filesystem)
    }

    pub fn list_events(&self, transfer_id: Option<u64>) -> Result<Vec<CoreEvent>, VnidropError> {
        self.block_on(self.inner.list_events(transfer_id))
            .map_err(VnidropError::repository)
    }

    pub fn inspect_ticket(&self, ticket: String) -> Result<TicketInspection, VnidropError> {
        let parsed = parse_transfer_ticket_with_limits(&ticket, &self.inner.limits)
            .context("failed to parse transfer ticket")
            .map_err(VnidropError::ticket)?;
        Ok(TicketInspection {
            kind: "vnidrop".to_string(),
            metadata: parsed.metadata,
        })
    }

    pub fn shutdown(&self) {
        self.block_on(self.inner.shutdown());
    }
}
