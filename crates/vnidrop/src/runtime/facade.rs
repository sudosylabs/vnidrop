use std::{future::Future, path::PathBuf, sync::Arc};

use anyhow::Context;
use serde_json::json;

use super::CoreInner;
use crate::{
    api::{
        CoreEvent, CoreEventSink, CoreLimits, CoreNetworkConfig, CoreStorageUsage,
        ReceiveOutputSink, ReceiveOutputSinkV2, ReceivedArtifact, ReceiverRequest, RuntimeStatus,
        ShareMetadataInput, ShareResult, ShareSource, StoredTransfer, TicketInspection,
        TransferAccessMode,
    },
    error::VnidropError,
    filesystem::platform_path,
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
        limits.validate().map_err(VnidropError::initialization)?;
        let relay_urls = network_config
            .validated_relay_urls()
            .map_err(VnidropError::initialization)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vnidrop")
            .build()?;
        let app_data_dir = PathBuf::from(app_data_dir);
        let inner = runtime
            .block_on(CoreInner::start(
                app_data_dir,
                event_sink,
                limits,
                network_config.mode,
                relay_urls,
            ))
            .map_err(VnidropError::initialization)?;
        Ok(Arc::new(Self { runtime, inner }))
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
