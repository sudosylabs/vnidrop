//! Immutable one-sender, one-receiver transfers between Saved devices.
//!
//! Separate from ordinary multi-receiver shares: own protocol, authorization,
//! and public APIs. Blob import/streaming/output sinks are reused.

mod auth;
pub(crate) mod inbox;
pub(crate) mod protocol;
mod schema;
mod state;
mod store;
mod store_outbox;

pub(crate) use crate::api::TargetedTransferRole;
pub(crate) use auth::{
    auth_secret_material, reconstruct_authorization, TargetedAuthorization,
    TargetedAuthorizationDraft,
};
pub(crate) use inbox::{RespondError, TargetedOfferInbox};
pub(crate) use protocol::TargetedTransferProtocol;
pub(crate) use schema::ensure_schema;
pub(crate) use store::{state_as_str, TargetedTransferRow, TargetedTransferStore};
