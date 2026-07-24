use anyhow::Context;
use iroh::RelayUrl;
use iroh_blobs::Hash;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, net::IpAddr, path::PathBuf, str::FromStr};

use crate::error::VnidropError;
use crate::util::{non_empty, now_ms};

pub(crate) const MAX_CUSTOM_RELAYS: usize = 8;
pub(crate) const MAX_RELAY_URL_BYTES: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum CoreRelayMode {
    Automatic,
    StrictCustom,
    CustomWithDirectFallback,
    LocalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct CoreNetworkConfig {
    pub mode: CoreRelayMode,
    pub relay_urls: Vec<String>,
}

impl Default for CoreNetworkConfig {
    fn default() -> Self {
        Self {
            mode: CoreRelayMode::Automatic,
            relay_urls: Vec::new(),
        }
    }
}

impl CoreNetworkConfig {
    pub(crate) fn validated_relay_urls(&self) -> anyhow::Result<Vec<RelayUrl>> {
        match self.mode {
            CoreRelayMode::Automatic | CoreRelayMode::LocalOnly => {
                if !self.relay_urls.is_empty() {
                    anyhow::bail!(
                        "{} relay mode must not include custom relay URLs",
                        match self.mode {
                            CoreRelayMode::Automatic => "automatic",
                            CoreRelayMode::LocalOnly => "local-only",
                            CoreRelayMode::StrictCustom
                            | CoreRelayMode::CustomWithDirectFallback => unreachable!(),
                        }
                    );
                }
                Ok(Vec::new())
            }
            CoreRelayMode::StrictCustom | CoreRelayMode::CustomWithDirectFallback => {
                if self.relay_urls.is_empty() {
                    anyhow::bail!("custom relay mode requires at least one relay URL");
                }
                if self.relay_urls.len() > MAX_CUSTOM_RELAYS {
                    anyhow::bail!(
                        "custom relay mode supports at most {MAX_CUSTOM_RELAYS} relay URLs"
                    );
                }

                let mut seen = BTreeSet::new();
                let mut validated = Vec::with_capacity(self.relay_urls.len());
                for (index, value) in self.relay_urls.iter().enumerate() {
                    if value.is_empty()
                        || value
                            .chars()
                            .any(|character| character.is_whitespace() || character.is_control())
                    {
                        anyhow::bail!(
                            "relay URL must be non-empty and contain no whitespace or control characters"
                        );
                    }
                    if value.len() > MAX_RELAY_URL_BYTES {
                        anyhow::bail!(
                            "relay URL is {} bytes, limit is {MAX_RELAY_URL_BYTES}",
                            value.len()
                        );
                    }
                    let url = RelayUrl::from_str(value)
                        .with_context(|| format!("invalid relay URL at position {}", index + 1))?;
                    let secure = url.scheme() == "https";
                    let loopback_http = url.scheme() == "http"
                        && url.host_str().is_some_and(|host| {
                            host.eq_ignore_ascii_case("localhost")
                                || host
                                    .trim_start_matches('[')
                                    .trim_end_matches(']')
                                    .parse::<IpAddr>()
                                    .is_ok_and(|address| address.is_loopback())
                        });
                    if !secure && !loopback_http {
                        anyhow::bail!(
                            "relay URL must use HTTPS; HTTP is allowed only for loopback development relays"
                        );
                    }
                    if url.host_str().is_none() {
                        anyhow::bail!("relay URL must include a host");
                    }
                    if url.port() == Some(0) {
                        anyhow::bail!("relay URL port must be between 1 and 65535");
                    }
                    if value.contains('@') || !url.username().is_empty() || url.password().is_some()
                    {
                        anyhow::bail!("relay URL must not contain credentials");
                    }
                    if url.query().is_some() || url.fragment().is_some() {
                        anyhow::bail!("relay URL must not contain a query or fragment");
                    }
                    if url.path() != "/" {
                        anyhow::bail!("relay URL must not contain a path");
                    }
                    if !seen.insert(url.clone()) {
                        anyhow::bail!("custom relay URLs must not contain duplicates");
                    }
                    validated.push(url);
                }
                Ok(validated)
            }
        }
    }
}

#[uniffi::export]
pub fn default_core_network_config() -> CoreNetworkConfig {
    CoreNetworkConfig::default()
}

/// Removes the blob store after its owning core has shut down.
///
/// Callers must verify that no transfer or share is active before shutdown.
#[uniffi::export]
pub fn clear_inactive_transfer_cache(app_data_dir: String) -> Result<u64, VnidropError> {
    let result = (|| -> anyhow::Result<u64> {
        let app_data_dir = PathBuf::from(app_data_dir);
        anyhow::ensure!(
            app_data_dir.is_absolute(),
            "app data directory must be absolute"
        );
        anyhow::ensure!(
            app_data_dir.file_name().is_some(),
            "app data directory must not be a filesystem root"
        );
        let blobs = app_data_dir.join("blobs");
        let metadata = match std::fs::symlink_metadata(&blobs) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "blob store path is not a directory"
        );
        let bytes = walkdir::WalkDir::new(&blobs)
            .follow_links(false)
            .into_iter()
            .try_fold(0u64, |total, entry| {
                let entry = entry?;
                let metadata = entry.metadata()?;
                Ok::<_, walkdir::Error>(if metadata.is_file() {
                    total.saturating_add(metadata.len())
                } else {
                    total
                })
            })?;
        std::fs::remove_dir_all(blobs)?;
        Ok(bytes)
    })();
    result.map_err(VnidropError::filesystem)
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct CoreLimits {
    pub max_sources: u64,
    pub max_collection_files: u64,
    pub max_total_bytes: u64,
    pub max_path_bytes: u64,
    pub max_ticket_bytes: u64,
    pub max_metadata_bytes: u64,
    pub max_events: u64,
    pub max_pending_approvals: u64,
    pub max_concurrent_transfers: u64,
    pub event_queue_capacity: u64,
}

impl Default for CoreLimits {
    fn default() -> Self {
        Self {
            max_sources: 128,
            max_collection_files: 10_000,
            // Cap extreme disk fill while still allowing multi-GB folders.
            max_total_bytes: 256 * 1024 * 1024 * 1024,
            max_path_bytes: 4_096,
            // vnd1 tickets are small JSON+base64; 256 KiB is a generous ceiling.
            max_ticket_bytes: 256 * 1024,
            max_metadata_bytes: 16 * 1024,
            max_events: 500,
            // Bound handshake spam / notification pressure on the sender.
            max_pending_approvals: 64,
            max_concurrent_transfers: 8,
            event_queue_capacity: 1_024,
        }
    }
}

impl CoreLimits {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        let positive = [
            ("max_sources", self.max_sources),
            ("max_collection_files", self.max_collection_files),
            ("max_total_bytes", self.max_total_bytes),
            ("max_path_bytes", self.max_path_bytes),
            ("max_ticket_bytes", self.max_ticket_bytes),
            ("max_metadata_bytes", self.max_metadata_bytes),
            ("max_events", self.max_events),
            ("max_pending_approvals", self.max_pending_approvals),
            ("max_concurrent_transfers", self.max_concurrent_transfers),
            ("event_queue_capacity", self.event_queue_capacity),
        ];
        for (name, value) in positive {
            if value == 0 {
                anyhow::bail!("core limit {name} must be greater than zero");
            }
        }
        for (name, value) in [
            ("max_pending_approvals", self.max_pending_approvals),
            ("max_concurrent_transfers", self.max_concurrent_transfers),
            ("event_queue_capacity", self.event_queue_capacity),
        ] {
            usize::try_from(value)
                .with_context(|| format!("core limit {name} exceeds platform capacity"))?;
        }
        if self.max_total_bytes > i64::MAX as u64 || self.max_events > i64::MAX as u64 {
            anyhow::bail!("SQLite-backed limits must fit in a signed 64-bit integer");
        }
        Ok(())
    }

    pub(crate) fn validate_metadata_text(
        &self,
        field: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        if let Some(value) = value {
            if value.len() as u64 > self.max_metadata_bytes {
                anyhow::bail!(
                    "{field} is {} bytes, limit is {}",
                    value.len(),
                    self.max_metadata_bytes
                );
            }
        }
        Ok(())
    }
}

#[uniffi::export]
pub fn default_core_limits() -> CoreLimits {
    CoreLimits::default()
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct CoreEvent {
    pub id: String,
    pub timestamp: i64,
    pub scope: String,
    pub transfer_id: Option<u64>,
    pub direction: Option<String>,
    pub phase: String,
    pub kind: String,
    pub data_json: String,
}

#[uniffi::export(with_foreign)]
pub trait CoreEventSink: Send + Sync {
    fn on_event(&self, event: CoreEvent);
}

#[uniffi::export(with_foreign)]
pub trait ReceiveOutputSink: Send + Sync {
    fn start_file(&self, relative_path: String) -> Result<(), crate::error::VnidropError>;
    fn write_chunk(
        &self,
        relative_path: String,
        bytes: Vec<u8>,
    ) -> Result<(), crate::error::VnidropError>;
    fn finish_file(&self, relative_path: String) -> Result<(), crate::error::VnidropError>;
    fn abort_file(
        &self,
        relative_path: String,
        reason: String,
    ) -> Result<(), crate::error::VnidropError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum ReceivedLocatorKind {
    FilesystemPath,
    AndroidMediaStore,
    AndroidDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct PublishedOutput {
    pub locator_kind: ReceivedLocatorKind,
    pub locator: String,
}

/// Versioned receive sink that reports the durable locator created at publish time.
#[uniffi::export(with_foreign)]
pub trait ReceiveOutputSinkV2: Send + Sync {
    fn start_file(&self, relative_path: String) -> Result<(), crate::error::VnidropError>;
    fn write_chunk(
        &self,
        relative_path: String,
        bytes: Vec<u8>,
    ) -> Result<(), crate::error::VnidropError>;
    fn finish_file(
        &self,
        relative_path: String,
    ) -> Result<PublishedOutput, crate::error::VnidropError>;
    fn abort_file(
        &self,
        relative_path: String,
        reason: String,
    ) -> Result<(), crate::error::VnidropError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct ReceivedArtifact {
    pub id: String,
    pub transfer_local_id: String,
    pub protocol_transfer_id: u64,
    pub relative_path: String,
    pub locator_kind: ReceivedLocatorKind,
    pub locator: String,
    pub logical_size: u64,
    pub published_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct CoreStorageUsage {
    pub blob_store_bytes: u64,
    pub database_bytes: u64,
    pub logs_bytes: u64,
    pub previews_bytes: u64,
    pub other_core_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct RuntimeStatus {
    pub endpoint_id: String,
    pub addr: String,
    pub active_transfers: u64,
    pub active_shares: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum SourceKind {
    Path,
    FileDescriptor,
    AndroidContentUri,
    IosSecurityScopedUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ShareSource {
    pub kind: SourceKind,
    pub value: String,
    pub display_name: Option<String>,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ShareMetadataInput {
    pub transfer_id: u64,
    pub transfer_name: Option<String>,
    pub sender_name: Option<String>,
    pub access_mode: TransferAccessMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum TransferAccessMode {
    Public,
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct StoredTransfer {
    pub local_id: String,
    pub transfer_id: u64,
    pub peer_id: Option<String>,
    pub direction: String,
    pub status: String,
    pub transfer_name: Option<String>,
    pub content_hash: Option<String>,
    pub ticket: Option<String>,
    pub file_count: u64,
    pub total_size: u64,
    pub access_mode: TransferAccessMode,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ShareResult {
    pub transfer_id: u64,
    pub ticket: String,
    pub hash: String,
    pub transfer_name: String,
    pub file_count: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct TransferMetadata {
    pub version: u8,
    pub transfer_id: u64,
    pub transfer_name: String,
    pub sender_name: Option<String>,
    pub created_at: i64,
    pub content_hash: String,
    pub file_count: u64,
    pub total_size: u64,
}

impl TransferMetadata {
    pub(crate) fn new(
        transfer_id: u64,
        transfer_name: impl Into<String>,
        sender_name: Option<String>,
        content_hash: Hash,
        file_count: u64,
        total_size: u64,
    ) -> Self {
        Self {
            version: 1,
            transfer_id,
            transfer_name: transfer_name.into(),
            sender_name: sender_name.and_then(non_empty),
            created_at: now_ms(),
            content_hash: content_hash.to_string(),
            file_count,
            total_size,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct TicketInspection {
    pub kind: String,
    pub metadata: TransferMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ReceiverRequest {
    pub id: String,
    pub transfer_id: u64,
    pub remote_endpoint_id: String,
    pub transfer_name: String,
    pub receiver_name: Option<String>,
    pub receiver_device_name: Option<String>,
    pub app_version: String,
    pub status: String,
    pub reason: Option<String>,
    pub requested_at: i64,
    pub responded_at: Option<i64>,
    pub completed_at: Option<i64>,
}
