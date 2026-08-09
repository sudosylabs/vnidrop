mod access_policy;
mod api;
mod approval;
mod contacts;
mod error;
mod event_hub;
mod filesystem;
mod grant;
mod handshake;
mod logging;
mod offer;
mod offer_inbox;
mod pairing;
mod repository;
mod runtime;
mod secret;
#[allow(
    dead_code,
    reason = "the private custody seam is activated by platform credential adapters"
)]
mod secure_secret;
mod targeted_transfer;
mod ticket;
mod transfer_state;
mod util;

pub use api::{
    clear_inactive_transfer_cache, default_core_limits, default_core_network_config,
    experimental_saved_device_capabilities, ContactSendResult, ContactSummary, CoreEvent,
    CoreEventSink, CoreLimits, CoreNetworkConfig, CoreRelayMode, CoreStorageUsage,
    DeviceRelationship, DeviceRelationshipState, ExperimentalSavedDeviceCapabilities,
    GrantLifetimeSetting, HeldOfferSummary, IncomingOffer, PendingPairing, PublishedOutput,
    ReceiveOutputSink, ReceiveOutputSinkV2, ReceivedArtifact, ReceivedLocatorKind, ReceiverRequest,
    RuntimeStatus, SavedDevice, ShareMetadataInput, ShareResult, ShareSource, SourceKind,
    StoredTransfer, TargetedTransfer, TargetedTransferState, TicketInspection, TransferAccessMode,
    TransferMetadata,
};
pub use error::VnidropError;
pub use runtime::VnidropCore;

uniffi::setup_scaffolding!();

#[cfg(test)]
#[path = "tests.rs"]
mod core_tests;
