//! Durable device-relationship rows (schema + queries).
//!
//! Orchestration (custody, pairing RPC, events) stays on
//! [`super::DeviceRelationshipService`]; this store is the domain adapter held
//! in [`crate::persistence::AppDataStores`].

use sqlx::{Row, SqlitePool};

use crate::{
    api::{DeviceRelationship, DeviceRelationshipState, SavedDevice},
    error::VnidropError,
    util::now_ms,
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
pub(super) struct RelationshipRow {
    pub(super) state: DeviceRelationshipState,
    pub(super) generation: u64,
    pub(super) minimum_protocol_version: u16,
    pub(super) session_id: Option<String>,
    pub(super) issued_grant_handle: Option<String>,
    pub(super) held_grant_handle: Option<String>,
    pub(super) issued_grant_id: Option<String>,
    pub(super) held_grant_id: Option<String>,
    pub(super) created_at: i64,
}

/// Compact projection used by grant-secret reconcile.
#[derive(Debug, Clone)]
pub(super) struct ReconcileRow {
    pub(super) remote_endpoint_id: String,
    pub(super) state: DeviceRelationshipState,
    pub(super) issued_grant_handle: Option<String>,
    pub(super) held_grant_handle: Option<String>,
}

pub(super) struct RelationshipUpsert<'a> {
    pub(super) remote_endpoint_id: &'a str,
    pub(super) state: DeviceRelationshipState,
    pub(super) generation: u64,
    pub(super) minimum_protocol_version: u16,
    pub(super) session_id: Option<&'a str>,
    pub(super) issued_grant_handle: Option<&'a str>,
    pub(super) held_grant_handle: Option<&'a str>,
    pub(super) issued_grant_id: Option<&'a str>,
    pub(super) held_grant_id: Option<&'a str>,
    pub(super) peer_ack: bool,
    pub(super) local_ack: bool,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
}

/// Domain store for `device_relationships` (+ generation tombstones).
#[derive(Clone)]
pub(crate) struct DeviceRelationshipStore {
    pool: SqlitePool,
}

impl DeviceRelationshipStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn ensure_schema(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_relationships (
                remote_endpoint_id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                generation INTEGER NOT NULL,
                minimum_protocol_version INTEGER NOT NULL,
                session_id TEXT,
                issued_grant_handle TEXT,
                held_grant_handle TEXT,
                issued_grant_id TEXT,
                held_grant_id TEXT,
                peer_ack INTEGER NOT NULL DEFAULT 0,
                local_ack INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            "#,
        )
        .execute(pool)
        .await?;
        let columns = sqlx::query("PRAGMA table_info(device_relationships)")
            .fetch_all(pool)
            .await?;
        let has = |name: &str| columns.iter().any(|row| row.get::<String, _>(1) == name);
        if !has("issued_grant_id") {
            sqlx::query("ALTER TABLE device_relationships ADD COLUMN issued_grant_id TEXT")
                .execute(pool)
                .await?;
        }
        if !has("held_grant_id") {
            sqlx::query("ALTER TABLE device_relationships ADD COLUMN held_grant_id TEXT")
                .execute(pool)
                .await?;
        }
        if !has("local_label") {
            sqlx::query("ALTER TABLE device_relationships ADD COLUMN local_label TEXT")
                .execute(pool)
                .await?;
        }
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

    pub(super) async fn count_active_slots(&self) -> Result<u64, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS n FROM device_relationships
            WHERE state IN ('saved', 'pending_outgoing', 'pending_incoming')
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(row.get::<i64, _>("n") as u64)
    }

    pub(super) async fn list_reconcile_rows(&self) -> Result<Vec<ReconcileRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, issued_grant_handle, held_grant_handle, state
            FROM device_relationships
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter()
            .map(|row| {
                Ok(ReconcileRow {
                    remote_endpoint_id: row.get("remote_endpoint_id"),
                    state: parse_state(&row.get::<String, _>("state"))?,
                    issued_grant_handle: row.get("issued_grant_handle"),
                    held_grant_handle: row.get("held_grant_handle"),
                })
            })
            .collect()
    }

    pub(super) async fn list(&self) -> Result<Vec<DeviceRelationship>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, state, generation, minimum_protocol_version, created_at, updated_at
            FROM device_relationships
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_relationship).collect()
    }

    pub(super) async fn list_saved_devices(&self) -> Result<Vec<SavedDevice>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id, local_label, created_at, updated_at
            FROM device_relationships
            WHERE state = 'saved'
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows
            .into_iter()
            .map(|row| SavedDevice {
                endpoint_id: row.get("remote_endpoint_id"),
                local_label: row.get("local_label"),
                remote_display_name: None,
                created_at: row.get("created_at"),
                last_authenticated_at: Some(row.get("updated_at")),
            })
            .collect())
    }

    pub(super) async fn set_saved_device_label(
        &self,
        peer_endpoint_id: &str,
        label: Option<String>,
    ) -> Result<bool, VnidropError> {
        let result = sqlx::query(
            r#"
            UPDATE device_relationships
            SET local_label = ?2, updated_at = ?3
            WHERE remote_endpoint_id = ?1 AND state = 'saved'
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(label)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(result.rows_affected() > 0)
    }

    pub(super) async fn set_issued_grant(
        &self,
        peer_endpoint_id: &str,
        handle: &str,
        grant_id: &str,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            UPDATE device_relationships
            SET issued_grant_handle = ?2, issued_grant_id = ?3, updated_at = ?4
            WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(handle)
        .bind(grant_id)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn set_held_grant(
        &self,
        peer_endpoint_id: &str,
        handle: &str,
        grant_id: &str,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            UPDATE device_relationships
            SET held_grant_handle = ?2, held_grant_id = ?3, updated_at = ?4
            WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(handle)
        .bind(grant_id)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) async fn set_minimum_protocol_version(
        &self,
        peer_endpoint_id: &str,
        minimum_protocol_version: u16,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE device_relationships SET minimum_protocol_version = ?2, updated_at = ?3 WHERE remote_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .bind(i64::from(minimum_protocol_version))
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn set_acks(
        &self,
        peer_endpoint_id: &str,
        local_ack: bool,
        peer_ack: bool,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE device_relationships SET local_ack = ?2, peer_ack = ?3, updated_at = ?4 WHERE remote_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .bind(i64::from(local_ack))
        .bind(i64::from(peer_ack))
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn set_state(
        &self,
        peer_endpoint_id: &str,
        state: DeviceRelationshipState,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE device_relationships SET state = ?2, updated_at = ?3 WHERE remote_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .bind(state_as_str(state))
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn list_expired_pending_peers(
        &self,
        cutoff_ms: i64,
    ) -> Result<Vec<String>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT remote_endpoint_id FROM device_relationships
            WHERE state IN ('pending_outgoing', 'pending_incoming') AND updated_at < ?1
            "#,
        )
        .bind(cutoff_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    pub(super) async fn delete(&self, peer_endpoint_id: &str) -> Result<(), VnidropError> {
        sqlx::query("DELETE FROM device_relationships WHERE remote_endpoint_id = ?1")
            .bind(peer_endpoint_id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn find_row(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Option<RelationshipRow>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT remote_endpoint_id, state, generation, minimum_protocol_version, session_id,
                   issued_grant_handle, held_grant_handle, issued_grant_id, held_grant_id,
                   peer_ack, local_ack, created_at, updated_at
            FROM device_relationships WHERE remote_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(relationship_row_from_sql).transpose()
    }

    pub(super) async fn upsert(&self, entry: RelationshipUpsert<'_>) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            INSERT INTO device_relationships (
                remote_endpoint_id, state, generation, minimum_protocol_version, session_id,
                issued_grant_handle, held_grant_handle, issued_grant_id, held_grant_id,
                peer_ack, local_ack, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(remote_endpoint_id) DO UPDATE SET
                state = excluded.state,
                generation = excluded.generation,
                minimum_protocol_version = excluded.minimum_protocol_version,
                session_id = excluded.session_id,
                issued_grant_handle = COALESCE(excluded.issued_grant_handle, device_relationships.issued_grant_handle),
                held_grant_handle = COALESCE(excluded.held_grant_handle, device_relationships.held_grant_handle),
                issued_grant_id = COALESCE(excluded.issued_grant_id, device_relationships.issued_grant_id),
                held_grant_id = COALESCE(excluded.held_grant_id, device_relationships.held_grant_id),
                peer_ack = excluded.peer_ack,
                local_ack = excluded.local_ack,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(entry.remote_endpoint_id)
        .bind(state_as_str(entry.state))
        .bind(entry.generation as i64)
        .bind(i64::from(entry.minimum_protocol_version))
        .bind(entry.session_id)
        .bind(entry.issued_grant_handle)
        .bind(entry.held_grant_handle)
        .bind(entry.issued_grant_id)
        .bind(entry.held_grant_id)
        .bind(i64::from(entry.peer_ack))
        .bind(i64::from(entry.local_ack))
        .bind(entry.created_at)
        .bind(entry.updated_at)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    /// Bump generation and clear grant columns after a prior generation was tombstoned.
    pub(super) async fn begin_grant_rotation(
        &self,
        peer_endpoint_id: &str,
        new_generation: u64,
    ) -> Result<(), VnidropError> {
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
        .bind(peer_endpoint_id)
        .bind(new_generation as i64)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn insert_tombstone(
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

    pub(super) async fn find_tombstone(
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
}

pub(super) fn state_as_str(state: DeviceRelationshipState) -> &'static str {
    match state {
        DeviceRelationshipState::PendingOutgoing => "pending_outgoing",
        DeviceRelationshipState::PendingIncoming => "pending_incoming",
        DeviceRelationshipState::Saved => "saved",
        DeviceRelationshipState::Revoked => "revoked",
        DeviceRelationshipState::Blocked => "blocked",
    }
}

fn parse_state(value: &str) -> Result<DeviceRelationshipState, VnidropError> {
    match value {
        "pending_outgoing" => Ok(DeviceRelationshipState::PendingOutgoing),
        "pending_incoming" => Ok(DeviceRelationshipState::PendingIncoming),
        "saved" => Ok(DeviceRelationshipState::Saved),
        "revoked" => Ok(DeviceRelationshipState::Revoked),
        "blocked" => Ok(DeviceRelationshipState::Blocked),
        _ => Err(VnidropError::Internal {
            reason: "unknown device relationship state".to_string(),
        }),
    }
}

fn row_to_relationship(row: sqlx::sqlite::SqliteRow) -> Result<DeviceRelationship, VnidropError> {
    Ok(DeviceRelationship {
        remote_endpoint_id: row.get("remote_endpoint_id"),
        state: parse_state(&row.get::<String, _>("state"))?,
        generation: row.get::<i64, _>("generation") as u64,
        minimum_protocol_version: row.get::<i64, _>("minimum_protocol_version") as u16,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn relationship_row_from_sql(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RelationshipRow, VnidropError> {
    Ok(RelationshipRow {
        state: parse_state(&row.get::<String, _>("state"))?,
        generation: row.get::<i64, _>("generation") as u64,
        minimum_protocol_version: row.get::<i64, _>("minimum_protocol_version") as u16,
        session_id: row.get("session_id"),
        issued_grant_handle: row.get("issued_grant_handle"),
        held_grant_handle: row.get("held_grant_handle"),
        issued_grant_id: row.get("issued_grant_id"),
        held_grant_id: row.get("held_grant_id"),
        created_at: row.get("created_at"),
    })
}
