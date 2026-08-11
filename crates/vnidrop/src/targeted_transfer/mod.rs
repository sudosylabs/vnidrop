//! Immutable one-sender, one-receiver transfers between Saved devices.
//!
//! Separate from ordinary multi-receiver shares: own protocol, authorization,
//! and public APIs. Blob import/streaming/output sinks are reused.

mod auth;
pub(crate) mod inbox;
pub(crate) mod protocol;
mod state;

pub(crate) use auth::{TargetedAuthorization, TargetedAuthorizationDraft};
pub(crate) use inbox::{RespondError, TargetedOfferInbox};
pub(crate) use protocol::TargetedTransferProtocol;

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
            state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

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
                state, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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

    pub(crate) async fn get(&self, id: &str) -> Result<Option<TargetedTransfer>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT id, sender_endpoint_id, receiver_endpoint_id, manifest_id,
                   file_count, total_size, state, created_at, updated_at
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
                   file_count, total_size, state, created_at, updated_at
            FROM targeted_transfers
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_transfer).collect()
    }

    #[allow(
        dead_code,
        reason = "called via cancel_targeted_transfers_for_peer for ticket 09"
    )]
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
