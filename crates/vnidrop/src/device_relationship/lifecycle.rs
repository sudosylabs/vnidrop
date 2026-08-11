//! Forget, block, grant rotation, and minimal revocation tombstones (design §7–§8).

use serde_json::json;

use super::{store::RelationshipRow, DeviceRelationshipService};
use crate::{
    api::DeviceRelationshipState, error::VnidropError, grant::GrantRejection,
    secure_secret::SecretHandle,
};

#[derive(Debug, Clone)]
pub(crate) struct ForgetOutcome {
    pub(crate) had_relationship: bool,
    pub(crate) generation: Option<u64>,
    pub(crate) issued_grant_id: Option<String>,
}

impl DeviceRelationshipService {
    /// Forget a saved (or pending) device: revoke locally first, clean secrets,
    /// then the caller sends a best-effort remote notice. Invitation-domain
    /// transfers are untouched.
    pub(crate) async fn forget(
        &self,
        peer_endpoint_id: String,
    ) -> Result<ForgetOutcome, VnidropError> {
        let peer_lock = self.lock_peer(&peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;

        let Some(row) = self.find_row(&peer_endpoint_id).await? else {
            self.eligibility.remove_for_peer(&peer_endpoint_id).await?;
            return Ok(ForgetOutcome {
                had_relationship: false,
                generation: None,
                issued_grant_id: None,
            });
        };

        let issued_grant_id = row.issued_grant_id.clone();
        let generation = row.generation;
        self.tombstone_generation(&peer_endpoint_id, &row).await?;
        self.delete_relationship(&peer_endpoint_id).await?;
        self.eligibility.remove_for_peer(&peer_endpoint_id).await?;
        self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::Revoked);
        drop(_guard);

        Ok(ForgetOutcome {
            had_relationship: true,
            generation: Some(generation),
            issued_grant_id,
        })
    }

    /// Identity-wide block: revoke relationship grants, keep deny + tombstones.
    /// Caller owns the durable deny record (`blocked_endpoints`).
    pub(crate) async fn revoke_for_block(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<(), VnidropError> {
        let peer_lock = self.lock_peer(peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;

        if let Some(row) = self.find_row(peer_endpoint_id).await? {
            self.tombstone_generation(peer_endpoint_id, &row).await?;
            self.delete_relationship(peer_endpoint_id).await?;
        }
        self.eligibility.remove_for_peer(peer_endpoint_id).await?;
        self.emit_changed(peer_endpoint_id, DeviceRelationshipState::Blocked);
        Ok(())
    }

    /// Activate a replacement grant: invalidate the prior generation first, then
    /// mint exactly one new active generation for the issued direction.
    pub(crate) async fn rotate_relationship_grant(
        &self,
        peer_endpoint_id: String,
    ) -> Result<u64, VnidropError> {
        let peer_lock = self.lock_peer(&peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;

        let Some(row) = self.find_row(&peer_endpoint_id).await? else {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "no relationship to rotate"
            )));
        };
        if row.state != DeviceRelationshipState::Saved {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "only saved relationships can rotate grants"
            )));
        }

        // Invalidate first: tombstone + secret removal before the new generation
        // becomes active, so a concurrent presenter cannot race past revocation.
        self.tombstone_generation(&peer_endpoint_id, &row).await?;
        self.clear_grant_secrets(&row).await?;

        let new_generation = row.generation.saturating_add(1);
        self.store
            .begin_grant_rotation(&peer_endpoint_id, new_generation)
            .await?;

        let _wire = self
            .mint_and_store_issued_grant(
                &peer_endpoint_id,
                new_generation,
                row.minimum_protocol_version,
            )
            .await?;

        self.event_hub.emit_endpoint(
            "pairing",
            "relationship-grant-rotated",
            json!({
                "peer_endpoint_id": peer_endpoint_id,
                "generation": new_generation,
            }),
        );
        Ok(new_generation)
    }

    /// Reject a presented generation when it is tombstoned or not the active one.
    pub(crate) async fn reject_replayed_generation(
        &self,
        peer_endpoint_id: &str,
        generation: u64,
        _grant_id: Option<&str>,
    ) -> Result<(), GrantRejection> {
        if self
            .find_tombstone(peer_endpoint_id, generation)
            .await
            .map_err(|_| GrantRejection::Unknown)?
            .is_some()
        {
            return Err(GrantRejection::Revoked);
        }

        let Some(row) = self
            .find_row(peer_endpoint_id)
            .await
            .map_err(|_| GrantRejection::Unknown)?
        else {
            return Err(GrantRejection::Unknown);
        };
        // Pending pairing and Saved both use the active row generation; only a
        // mismatch (or tombstone above) means the presenter is replaying.
        match row.state {
            DeviceRelationshipState::PendingOutgoing
            | DeviceRelationshipState::PendingIncoming
            | DeviceRelationshipState::Saved => {
                if row.generation != generation {
                    return Err(GrantRejection::Unknown);
                }
                Ok(())
            }
            DeviceRelationshipState::Revoked | DeviceRelationshipState::Blocked => {
                Err(GrantRejection::Unknown)
            }
        }
    }

    #[cfg(test)]
    pub(crate) async fn list_tombstones(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Vec<super::store::GenerationTombstone>, VnidropError> {
        self.store.list_tombstones(peer_endpoint_id).await
    }

    #[cfg(test)]
    pub(crate) async fn issued_grant_snapshot(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Option<(u64, String)>, VnidropError> {
        let Some(row) = self.find_row(peer_endpoint_id).await? else {
            return Ok(None);
        };
        let Some(grant_id) = row.issued_grant_id else {
            return Ok(None);
        };
        Ok(Some((row.generation, grant_id)))
    }

    async fn tombstone_generation(
        &self,
        peer_endpoint_id: &str,
        row: &RelationshipRow,
    ) -> Result<(), VnidropError> {
        self.store.insert_tombstone(peer_endpoint_id, row).await
    }

    async fn find_tombstone(
        &self,
        peer_endpoint_id: &str,
        generation: u64,
    ) -> Result<Option<super::store::GenerationTombstone>, VnidropError> {
        self.store
            .find_tombstone(peer_endpoint_id, generation)
            .await
    }

    async fn clear_grant_secrets(&self, row: &RelationshipRow) -> Result<(), VnidropError> {
        let Some(custody) = &self.custody else {
            return Ok(());
        };
        for handle in [&row.issued_grant_handle, &row.held_grant_handle]
            .into_iter()
            .flatten()
        {
            let _ = custody
                .remove(&SecretHandle::from_stored(handle.clone()))
                .await;
        }
        Ok(())
    }

    /// Apply a best-effort remote revocation notice from a peer.
    pub(crate) async fn handle_remote_revoke(
        &self,
        remote_endpoint_id: String,
        generation: u64,
    ) -> bool {
        let peer_lock = self.lock_peer(&remote_endpoint_id).await;
        let _guard = peer_lock.lock().await;
        let Ok(Some(row)) = self.find_row(&remote_endpoint_id).await else {
            return true;
        };
        if row.generation != generation
            && generation != 0
            && self
                .find_tombstone(&remote_endpoint_id, generation)
                .await
                .ok()
                .flatten()
                .is_some()
        {
            return true;
        }
        let _ = self.tombstone_generation(&remote_endpoint_id, &row).await;
        let _ = self.delete_relationship(&remote_endpoint_id).await;
        let _ = self.eligibility.remove_for_peer(&remote_endpoint_id).await;
        self.emit_changed(&remote_endpoint_id, DeviceRelationshipState::Revoked);
        true
    }
}
