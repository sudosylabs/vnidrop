//! Runtime operations for experimental saved devices and device relationships.

use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use super::CoreInner;
use crate::{error::VnidropError, util::now_ms};

impl CoreInner {
    pub(super) async fn list_pairing_eligibilities(
        &self,
    ) -> Result<Vec<crate::api::PairingEligibilitySummary>, crate::error::VnidropError> {
        self.pairing_eligibility.list().await
    }

    pub(super) async fn decline_pairing_eligibility(
        &self,
        peer_endpoint_id: String,
    ) -> Result<(), crate::error::VnidropError> {
        self.pairing_eligibility.decline(&peer_endpoint_id).await
    }

    pub(super) async fn request_saved_device_pairing(
        &self,
        peer_endpoint_id: String,
    ) -> Result<bool, crate::error::VnidropError> {
        self.device_relationships
            .request_pairing(peer_endpoint_id)
            .await
    }

    pub(super) async fn list_device_relationships(
        &self,
    ) -> Result<Vec<crate::api::DeviceRelationship>, crate::error::VnidropError> {
        self.device_relationships.list().await
    }

    pub(super) async fn list_saved_devices(
        &self,
    ) -> Result<Vec<crate::api::SavedDevice>, crate::error::VnidropError> {
        self.device_relationships.list_saved_devices().await
    }

    pub(super) async fn respond_to_device_pairing(
        self: &Arc<Self>,
        peer_endpoint_id: String,
        accepted: bool,
    ) -> Result<bool, crate::error::VnidropError> {
        if self
            .blocked_devices
            .is_blocked(&peer_endpoint_id)
            .await
            .unwrap_or(true)
        {
            return Ok(false);
        }
        self.device_relationships
            .respond_to_pairing(peer_endpoint_id, accepted)
            .await
    }

    pub(super) async fn forget_saved_device(
        self: &Arc<Self>,
        peer_endpoint_id: String,
    ) -> Result<(), crate::error::VnidropError> {
        let outcome = self
            .device_relationships
            .forget(peer_endpoint_id.clone())
            .await?;
        // Targeted transfers for this relationship only.
        // Invitation-domain shares are deliberately not cancelled here.
        self.cancel_targeted_transfers_for_peer(&peer_endpoint_id)
            .await?;
        self.emit_endpoint(
            "pairing",
            "saved-device-forgotten",
            json!({
                "peer_endpoint_id": peer_endpoint_id,
                "had_relationship": outcome.had_relationship,
            }),
        );
        if outcome.had_relationship {
            if let Some(generation) = outcome.generation {
                self.device_relationships
                    .notify_remote_revoke(&peer_endpoint_id, generation, outcome.issued_grant_id)
                    .await;
            }
        }
        Ok(())
    }

    pub(super) async fn block_device(
        self: &Arc<Self>,
        peer_endpoint_id: String,
    ) -> Result<(), crate::error::VnidropError> {
        let now = now_ms();
        self.blocked_devices
            .block_endpoint(&peer_endpoint_id, now)
            .await
            .map_err(VnidropError::repository)?;
        self.device_relationships
            .revoke_for_block(&peer_endpoint_id)
            .await?;
        self.cancel_targeted_transfers_for_peer(&peer_endpoint_id)
            .await?;
        self.emit_endpoint(
            "pairing",
            "device-blocked",
            json!({ "peer_endpoint_id": peer_endpoint_id }),
        );
        // Silence: blocked peers are not notified (design §8).
        Ok(())
    }

    pub(super) async fn unblock_device(
        &self,
        peer_endpoint_id: String,
    ) -> Result<(), crate::error::VnidropError> {
        self.blocked_devices
            .unblock_endpoint(&peer_endpoint_id)
            .await
            .map_err(VnidropError::repository)?;
        // Unblock removes only the deny rule; grants/relationships stay gone.
        Ok(())
    }

    pub(super) async fn list_blocked_devices(
        &self,
    ) -> Result<Vec<String>, crate::error::VnidropError> {
        self.blocked_devices
            .list_blocked()
            .await
            .map_err(VnidropError::repository)
    }

    pub(super) async fn rotate_relationship_grant(
        &self,
        peer_endpoint_id: String,
    ) -> Result<u64, crate::error::VnidropError> {
        self.device_relationships
            .rotate_relationship_grant(peer_endpoint_id)
            .await
    }

    #[cfg(test)]
    pub(super) fn targeted_cancel_log_for_test(&self) -> Vec<String> {
        self.targeted_cancel_log
            .lock()
            .expect("targeted cancel log")
            .clone()
    }

    #[cfg(test)]
    pub(super) async fn submit_pairing_eligibility_for_test(
        &self,
        peer_endpoint_id: String,
        session_id: String,
        capability: Vec<u8>,
    ) -> Result<bool, crate::error::VnidropError> {
        let material = crate::secure_secret::SecretMaterial::new(capability)?;
        self.pairing_eligibility
            .accept_presented_eligibility(&peer_endpoint_id, &session_id, &material)
            .await
    }
}
