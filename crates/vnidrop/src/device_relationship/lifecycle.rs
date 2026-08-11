//! Forget, block, grant rotation, and minimal revocation tombstones (design §7–§8).

use serde_json::json;
use sqlx::Row;

use super::{DeviceRelationshipService, RelationshipRow};
use crate::{
    api::DeviceRelationshipState, error::VnidropError, grant::GrantRejection,
    secure_secret::SecretHandle, util::now_ms,
};

/// Minimal non-secret tombstone for a revoked relationship generation.
///
/// Retains only what is needed to reject replay: peer identity, generation,
/// opaque grant ids, and revocation time. No names, filenames, history, or
/// capability material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenerationTombstone {
    pub(crate) remote_endpoint_id: String,
    pub(crate) generation: u64,
    pub(crate) issued_grant_id: Option<String>,
    pub(crate) held_grant_id: Option<String>,
    pub(crate) revoked_at: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ForgetOutcome {
    pub(crate) had_relationship: bool,
    pub(crate) generation: Option<u64>,
    pub(crate) issued_grant_id: Option<String>,
}

impl DeviceRelationshipService {
    pub(crate) async fn ensure_lifecycle_schema(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS relationship_generation_tombstones (
                remote_endpoint_id TEXT NOT NULL,
                generation INTEGER NOT NULL,
                issued_grant_id TEXT,
                held_grant_id TEXT,
                revoked_at INTEGER NOT NULL,
                PRIMARY KEY (remote_endpoint_id, generation)
            );
            "#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

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
        let now = now_ms();
        sqlx::query(
            r#"
            UPDATE device_relationships
            SET generation = ?2,
                issued_grant_handle = NULL,
                held_grant_handle = NULL,
                issued_grant_id = NULL,
                held_grant_id = NULL,
                updated_at = ?3
            WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(&peer_endpoint_id)
        .bind(new_generation as i64)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;

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
    ) -> Result<Vec<GenerationTombstone>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, generation, issued_grant_id, held_grant_id, revoked_at
            FROM relationship_generation_tombstones
            WHERE remote_endpoint_id = ?1
            ORDER BY generation ASC
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows
            .into_iter()
            .map(|row| GenerationTombstone {
                remote_endpoint_id: row.get("remote_endpoint_id"),
                generation: row.get::<i64, _>("generation") as u64,
                issued_grant_id: row.get("issued_grant_id"),
                held_grant_id: row.get("held_grant_id"),
                revoked_at: row.get("revoked_at"),
            })
            .collect())
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
        sqlx::query(
            r#"
            INSERT INTO relationship_generation_tombstones (
                remote_endpoint_id, generation, issued_grant_id, held_grant_id, revoked_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(remote_endpoint_id, generation) DO UPDATE SET
                issued_grant_id = COALESCE(excluded.issued_grant_id, relationship_generation_tombstones.issued_grant_id),
                held_grant_id = COALESCE(excluded.held_grant_id, relationship_generation_tombstones.held_grant_id),
                revoked_at = excluded.revoked_at
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(row.generation as i64)
        .bind(row.issued_grant_id.as_deref())
        .bind(row.held_grant_id.as_deref())
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    async fn find_tombstone(
        &self,
        peer_endpoint_id: &str,
        generation: u64,
    ) -> Result<Option<GenerationTombstone>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT remote_endpoint_id, generation, issued_grant_id, held_grant_id, revoked_at
            FROM relationship_generation_tombstones
            WHERE remote_endpoint_id = ?1 AND generation = ?2
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(generation as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(row.map(|row| GenerationTombstone {
            remote_endpoint_id: row.get("remote_endpoint_id"),
            generation: row.get::<i64, _>("generation") as u64,
            issued_grant_id: row.get("issued_grant_id"),
            held_grant_id: row.get("held_grant_id"),
            revoked_at: row.get("revoked_at"),
        }))
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
