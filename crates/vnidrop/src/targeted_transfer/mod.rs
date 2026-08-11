//! Immutable one-sender, one-receiver transfers between Saved devices.
//!
//! Separate from ordinary multi-receiver shares: own protocol, authorization,
//! and public APIs. Blob import/streaming/output sinks are reused.

mod auth;
pub(crate) mod inbox;
pub(crate) mod protocol;
mod state;
mod store;

pub(crate) use auth::{
    auth_secret_material, reconstruct_authorization, TargetedAuthorization,
    TargetedAuthorizationDraft,
};
pub(crate) use inbox::{RespondError, TargetedOfferInbox};
pub(crate) use protocol::TargetedTransferProtocol;
pub(crate) use store::{
    ensure_schema, state_as_str, TargetedTransferRole, TargetedTransferRow, TargetedTransferStore,
};
