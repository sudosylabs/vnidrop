//! Experimental saved-device mutual-consent relationships.
//!
//! Implements design §6/§7: pending outgoing/incoming states, directional grants
//! bound to relationship generation, and Saved only after mutual acknowledgement.

mod crypto;
mod lifecycle;
mod protocol;
mod service;
mod store;

pub(crate) use protocol::{RelationshipProtocol, WireProof};
pub(crate) use service::DeviceRelationshipService;
pub(crate) use store::DeviceRelationshipStore;
#[cfg(test)]
pub(crate) use store::GenerationTombstone;
