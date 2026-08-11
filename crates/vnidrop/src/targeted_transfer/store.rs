//! Durable targeted-transfer rows (schema + queries).
//!
//! This is the domain store adapter for targeted transfers. Callers use store
//! methods — not a raw SQL pool.

use sqlx::{Row, SqlitePool};

use crate::{
    api::{TargetedTransfer, TargetedTransferState},
    error::VnidropError,
    util::now_ms,
};

pub(crate) async fn ensure_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targeted_transfers (
            id TEXT PRIMARY KEY,
            protocol_transfer_id INTEGER NOT NULL UNIQUE,
            sender_endpoint_id TEXT NOT NULL,
            receiver_endpoint_id TEXT NOT NULL,
            manifest_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            transfer_name TEXT NOT NULL,
            file_count INTEGER NOT NULL,
            total_size INTEGER NOT NULL,
            verified_bytes INTEGER NOT NULL DEFAULT 0,
            blob_ticket TEXT,
            authorization_secret_handle TEXT,
            role TEXT NOT NULL DEFAULT 'sender',
            state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    let columns = sqlx::query("PRAGMA table_info(targeted_transfers)")
        .fetch_all(pool)
        .await?;
    let has = |name: &str| columns.iter().any(|row| row.get::<String, _>(1) == name);
    if !has("verified_bytes") {
        sqlx::query(
            "ALTER TABLE targeted_transfers ADD COLUMN verified_bytes INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    if !has("blob_ticket") {
        sqlx::query("ALTER TABLE targeted_transfers ADD COLUMN blob_ticket TEXT")
            .execute(pool)
            .await?;
    }
    if !has("authorization_secret_handle") {
        sqlx::query("ALTER TABLE targeted_transfers ADD COLUMN authorization_secret_handle TEXT")
            .execute(pool)
            .await?;
    }
    if !has("role") {
        sqlx::query(
            "ALTER TABLE targeted_transfers ADD COLUMN role TEXT NOT NULL DEFAULT 'sender'",
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct TargetedTransferStore {
    pool: SqlitePool,
}

impl TargetedTransferStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn insert(&self, transfer: &TargetedTransferRow) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            INSERT INTO targeted_transfers (
                id, protocol_transfer_id, sender_endpoint_id, receiver_endpoint_id,
                manifest_id, content_hash, transfer_name, file_count, total_size,
                verified_bytes, blob_ticket, authorization_secret_handle, role,
                state, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&transfer.id)
        .bind(transfer.protocol_transfer_id as i64)
        .bind(&transfer.sender_endpoint_id)
        .bind(&transfer.receiver_endpoint_id)
        .bind(&transfer.manifest_id)
        .bind(&transfer.content_hash)
        .bind(&transfer.transfer_name)
        .bind(transfer.file_count as i64)
        .bind(transfer.total_size as i64)
        .bind(transfer.verified_bytes as i64)
        .bind(&transfer.blob_ticket)
        .bind(&transfer.authorization_secret_handle)
        .bind(role_as_str(transfer.role))
        .bind(state_as_str(transfer.state))
        .bind(transfer.created_at)
        .bind(transfer.updated_at)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn set_state(
        &self,
        id: &str,
        from: TargetedTransferState,
        to: TargetedTransferState,
    ) -> Result<(), VnidropError> {
        from.validate_transition_to(to)?;
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = ?2, updated_at = ?3
            WHERE id = ?1 AND state = ?4
            "#,
        )
        .bind(id)
        .bind(state_as_str(to))
        .bind(now_ms())
        .bind(state_as_str(from))
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            return Err(VnidropError::InvalidTransition {
                reason: format!("{} -> {}", state_as_str(from), state_as_str(to)),
            });
        }
        Ok(())
    }

    /// Transition from any non-terminal state; used by cancel/delete.
    pub(crate) async fn set_state_from_any(
        &self,
        id: &str,
        to: TargetedTransferState,
    ) -> Result<(), VnidropError> {
        let Some(row) = self.get_row(id).await? else {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "unknown targeted transfer"
            )));
        };
        if row.state == to {
            return Ok(());
        }
        row.state.validate_transition_to(to)?;
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = ?2, updated_at = ?3
            WHERE id = ?1 AND state = ?4
            "#,
        )
        .bind(id)
        .bind(state_as_str(to))
        .bind(now_ms())
        .bind(state_as_str(row.state))
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            return Err(VnidropError::InvalidTransition {
                reason: format!("{} -> {}", state_as_str(row.state), state_as_str(to)),
            });
        }
        Ok(())
    }

    pub(crate) async fn set_verified_bytes(
        &self,
        id: &str,
        verified_bytes: u64,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET verified_bytes = ?2, updated_at = ?3
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .bind(verified_bytes as i64)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn store_authorization(
        &self,
        id: &str,
        blob_ticket: &str,
        authorization_secret_handle: &str,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET blob_ticket = ?2,
                authorization_secret_handle = ?3,
                updated_at = ?4
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .bind(blob_ticket)
        .bind(authorization_secret_handle)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn clear_authorization(&self, id: &str) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET blob_ticket = NULL,
                authorization_secret_handle = NULL,
                verified_bytes = 0,
                updated_at = ?2
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn get(&self, id: &str) -> Result<Option<TargetedTransfer>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT id, sender_endpoint_id, receiver_endpoint_id, manifest_id,
                   file_count, total_size, verified_bytes, state, created_at, updated_at
            FROM targeted_transfers WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(row_to_transfer).transpose()
    }

    pub(crate) async fn get_row(
        &self,
        id: &str,
    ) -> Result<Option<TargetedTransferRow>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT id, protocol_transfer_id, sender_endpoint_id, receiver_endpoint_id,
                   manifest_id, content_hash, transfer_name, file_count, total_size,
                   verified_bytes, blob_ticket, authorization_secret_handle, role,
                   state, created_at, updated_at
            FROM targeted_transfers WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(row_to_full).transpose()
    }

    pub(crate) async fn list(&self) -> Result<Vec<TargetedTransfer>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT id, sender_endpoint_id, receiver_endpoint_id, manifest_id,
                   file_count, total_size, verified_bytes, state, created_at, updated_at
            FROM targeted_transfers
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_transfer).collect()
    }

    pub(crate) async fn list_resumable_sender_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT id, protocol_transfer_id, sender_endpoint_id, receiver_endpoint_id,
                   manifest_id, content_hash, transfer_name, file_count, total_size,
                   verified_bytes, blob_ticket, authorization_secret_handle, role,
                   state, created_at, updated_at
            FROM targeted_transfers
            WHERE role = 'sender'
              AND state IN ('approved', 'connecting', 'transferring', 'interrupted')
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    pub(crate) async fn cancel_by_peer(&self, peer_endpoint_id: &str) -> Result<u64, VnidropError> {
        let now = now_ms();
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'cancelled', updated_at = ?2
            WHERE (sender_endpoint_id = ?1 OR receiver_endpoint_id = ?1)
              AND state NOT IN ('completed', 'declined', 'cancelled', 'failed', 'deleted')
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(result.rows_affected())
    }

    pub(crate) async fn protocol_ids_for_peer(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Vec<u64>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT protocol_transfer_id FROM targeted_transfers
            WHERE sender_endpoint_id = ?1 OR receiver_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>(0) as u64)
            .collect())
    }

    pub(crate) async fn mark_interrupted_in_flight(&self) -> Result<u64, VnidropError> {
        let now = now_ms();
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'interrupted', updated_at = ?1
            WHERE state IN ('connecting', 'transferring')
            "#,
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(result.rows_affected())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetedTransferRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetedTransferRow {
    pub(crate) id: String,
    pub(crate) protocol_transfer_id: u64,
    pub(crate) sender_endpoint_id: String,
    pub(crate) receiver_endpoint_id: String,
    pub(crate) manifest_id: String,
    pub(crate) content_hash: String,
    pub(crate) transfer_name: String,
    pub(crate) file_count: u64,
    pub(crate) total_size: u64,
    pub(crate) verified_bytes: u64,
    pub(crate) blob_ticket: Option<String>,
    pub(crate) authorization_secret_handle: Option<String>,
    pub(crate) role: TargetedTransferRole,
    pub(crate) state: TargetedTransferState,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

fn row_to_transfer(row: sqlx::sqlite::SqliteRow) -> Result<TargetedTransfer, VnidropError> {
    Ok(TargetedTransfer {
        id: row.get("id"),
        sender_endpoint_id: row.get("sender_endpoint_id"),
        receiver_endpoint_id: row.get("receiver_endpoint_id"),
        manifest_id: row.get("manifest_id"),
        file_count: row.get::<i64, _>("file_count") as u64,
        total_size: row.get::<i64, _>("total_size") as u64,
        verified_bytes: row.try_get::<i64, _>("verified_bytes").unwrap_or(0) as u64,
        state: parse_state(&row.get::<String, _>("state"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_full(row: sqlx::sqlite::SqliteRow) -> Result<TargetedTransferRow, VnidropError> {
    Ok(TargetedTransferRow {
        id: row.get("id"),
        protocol_transfer_id: row.get::<i64, _>("protocol_transfer_id") as u64,
        sender_endpoint_id: row.get("sender_endpoint_id"),
        receiver_endpoint_id: row.get("receiver_endpoint_id"),
        manifest_id: row.get("manifest_id"),
        content_hash: row.get("content_hash"),
        transfer_name: row.get("transfer_name"),
        file_count: row.get::<i64, _>("file_count") as u64,
        total_size: row.get::<i64, _>("total_size") as u64,
        verified_bytes: row.try_get::<i64, _>("verified_bytes").unwrap_or(0) as u64,
        blob_ticket: row.try_get("blob_ticket").ok().flatten(),
        authorization_secret_handle: row.try_get("authorization_secret_handle").ok().flatten(),
        role: parse_role(
            &row.try_get::<String, _>("role")
                .unwrap_or_else(|_| "sender".to_string()),
        )?,
        state: parse_state(&row.get::<String, _>("state"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(crate) fn state_as_str(state: TargetedTransferState) -> &'static str {
    match state {
        TargetedTransferState::Preparing => "preparing",
        TargetedTransferState::Offering => "offering",
        TargetedTransferState::AwaitingApproval => "awaiting_approval",
        TargetedTransferState::Approved => "approved",
        TargetedTransferState::Connecting => "connecting",
        TargetedTransferState::Transferring => "transferring",
        TargetedTransferState::Interrupted => "interrupted",
        TargetedTransferState::Completed => "completed",
        TargetedTransferState::Declined => "declined",
        TargetedTransferState::Cancelled => "cancelled",
        TargetedTransferState::Failed => "failed",
        TargetedTransferState::Deleted => "deleted",
    }
}

fn role_as_str(role: TargetedTransferRole) -> &'static str {
    match role {
        TargetedTransferRole::Sender => "sender",
        TargetedTransferRole::Receiver => "receiver",
    }
}

fn parse_role(value: &str) -> Result<TargetedTransferRole, VnidropError> {
    match value {
        "sender" => Ok(TargetedTransferRole::Sender),
        "receiver" => Ok(TargetedTransferRole::Receiver),
        other => Err(VnidropError::repository(anyhow::anyhow!(
            "unknown targeted transfer role: {other}"
        ))),
    }
}

fn parse_state(value: &str) -> Result<TargetedTransferState, VnidropError> {
    match value {
        "preparing" => Ok(TargetedTransferState::Preparing),
        "offering" => Ok(TargetedTransferState::Offering),
        "awaiting_approval" => Ok(TargetedTransferState::AwaitingApproval),
        "approved" => Ok(TargetedTransferState::Approved),
        "connecting" => Ok(TargetedTransferState::Connecting),
        "transferring" => Ok(TargetedTransferState::Transferring),
        "interrupted" => Ok(TargetedTransferState::Interrupted),
        "completed" => Ok(TargetedTransferState::Completed),
        "declined" => Ok(TargetedTransferState::Declined),
        "cancelled" => Ok(TargetedTransferState::Cancelled),
        "failed" => Ok(TargetedTransferState::Failed),
        "deleted" => Ok(TargetedTransferState::Deleted),
        other => Err(VnidropError::repository(anyhow::anyhow!(
            "unknown targeted transfer state: {other}"
        ))),
    }
}
