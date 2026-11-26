//! Invitation-transfer domain store (history, approvals, delivery receipts).
//!
//! This is the invitation half of [`crate::persistence::AppDataStores`]. It owns
//! only invitation-transfer tables — not relationships, eligibility, blocks, or
//! secret metadata (those have their own domain stores).

#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    access_policy::mode_from_storage,
    api::{CoreEvent, ReceivedArtifact, ReceivedLocatorKind, ReceiverRequest, StoredTransfer},
    transfer_state::{ReceiverRequestStatus, TransferDirection, TransferStatus},
    util::now_ms,
};

mod delivery_receipts;
mod receiver_requests;
mod schema;

/// Invitation-transfer durable store (history, receiver requests, receipts, events).
///
/// Type name kept for call-site stability; module path is [`crate::invitation`].
#[derive(Debug, Clone)]
pub(crate) struct Repository {
    pool: SqlitePool,
    #[cfg(test)]
    fail_next_write: Arc<AtomicBool>,
    #[cfg(test)]
    fail_receive_history_after_dependants: Arc<AtomicBool>,
}

pub(crate) struct TransferUpsert<'a> {
    pub(crate) transfer_id: u64,
    pub(crate) peer_id: Option<&'a str>,
    pub(crate) direction: TransferDirection,
    pub(crate) status: TransferStatus,
    pub(crate) transfer_name: Option<&'a str>,
    pub(crate) sender_name: Option<&'a str>,
    pub(crate) content_hash: Option<&'a str>,
    pub(crate) ticket: Option<&'a str>,
    pub(crate) file_count: u64,
    pub(crate) total_size: u64,
    pub(crate) access_mode: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct PersistedShare {
    pub(crate) transfer_id: u64,
    pub(crate) local_id: String,
    pub(crate) content_hash: String,
    pub(crate) ticket: Option<String>,
    pub(crate) access_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoveredTransfer {
    pub(crate) transfer_id: u64,
    pub(crate) direction: TransferDirection,
    pub(crate) previous_status: TransferStatus,
}

pub(crate) struct ReceivedArtifactInsert<'a> {
    pub(crate) transfer_local_id: &'a str,
    pub(crate) protocol_transfer_id: u64,
    pub(crate) relative_path: &'a str,
    pub(crate) locator_kind: ReceivedLocatorKind,
    pub(crate) locator: &'a str,
    pub(crate) logical_size: u64,
}

pub(crate) struct ReceiverRequestInsert<'a> {
    pub(crate) id: &'a str,
    pub(crate) transfer_id: u64,
    pub(crate) remote_endpoint_id: &'a str,
    pub(crate) transfer_name: &'a str,
    pub(crate) receiver_name: Option<&'a str>,
    pub(crate) receiver_device_name: Option<&'a str>,
    pub(crate) app_version: &'a str,
}

pub(crate) enum DeliveryCompletion {
    Recorded { remote_display_name: Option<String> },
    AlreadyRecorded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingDeliveryReceipt {
    pub(crate) local_transfer_id: u64,
    pub(crate) sender_blob_ticket: String,
    pub(crate) request_id: String,
    pub(crate) sender_transfer_id: u64,
    pub(crate) token: String,
    pub(crate) failure_reason: Option<String>,
    pub(crate) created_at: i64,
}

pub(crate) struct PendingDeliveryReceiptInsert<'a> {
    pub(crate) local_transfer_id: u64,
    pub(crate) sender_blob_ticket: &'a str,
    pub(crate) request_id: &'a str,
    pub(crate) sender_transfer_id: u64,
    pub(crate) token: &'a str,
    pub(crate) failure_reason: Option<&'a str>,
}

impl Repository {
    pub(crate) fn from_pool(pool: SqlitePool) -> Self {
        Self {
            pool,
            #[cfg(test)]
            fail_next_write: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            fail_receive_history_after_dependants: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Test/helper entry: opens [`AppDataStores`] and returns the invitation store.
    #[cfg(test)]
    pub(crate) async fn open(app_data_dir: &Path) -> Result<Self> {
        Ok(crate::persistence::open_all(app_data_dir).await?.invitation)
    }

    pub(crate) async fn insert_transfer(&self, transfer: TransferUpsert<'_>) -> Result<()> {
        self.maybe_fail_write()?;
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO transfers (
                transfer_id, local_id, protocol_transfer_id, peer_id, direction, status,
                transfer_name, content_hash, ticket, file_count, total_size, access_mode,
                created_at, updated_at
            )
            VALUES (?1, ?2, ?1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12);
            "#,
        )
        .bind(to_db_id(transfer.transfer_id)?)
        .bind(Uuid::new_v4().to_string())
        .bind(transfer.peer_id)
        .bind(transfer.direction.as_str())
        .bind(transfer.status.as_str())
        .bind(transfer.transfer_name)
        .bind(transfer.content_hash)
        .bind(transfer.ticket)
        .bind(to_db_id(transfer.file_count)?)
        .bind(to_db_id(transfer.total_size)?)
        .bind(transfer.access_mode)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn start_receive(&self, transfer: TransferUpsert<'_>) -> Result<()> {
        self.maybe_fail_write()?;
        if transfer.direction != TransferDirection::Receive
            || transfer.status != TransferStatus::Receiving
        {
            anyhow::bail!("receive must start in the receiving state");
        }
        let now = now_ms();
        let result = sqlx::query(
            r#"
            INSERT INTO transfers (
                transfer_id, local_id, protocol_transfer_id, peer_id, direction, status,
                transfer_name, content_hash, ticket, file_count, total_size, access_mode,
                created_at, updated_at
            )
            VALUES (?1, ?2, ?1, ?3, 'receive', 'receiving', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
            ON CONFLICT(transfer_id) DO UPDATE SET
                status = 'receiving',
                transfer_name = excluded.transfer_name,
                content_hash = excluded.content_hash,
                ticket = excluded.ticket,
                file_count = excluded.file_count,
                total_size = excluded.total_size,
                access_mode = excluded.access_mode,
                peer_id = excluded.peer_id,
                updated_at = excluded.updated_at
            WHERE transfers.direction = 'receive'
              AND transfers.status IN ('done', 'failed', 'cancelled')
            "#,
        )
        .bind(to_db_id(transfer.transfer_id)?)
        .bind(Uuid::new_v4().to_string())
        .bind(transfer.peer_id)
        .bind(transfer.transfer_name)
        .bind(transfer.content_hash)
        .bind(transfer.ticket)
        .bind(to_db_id(transfer.file_count)?)
        .bind(to_db_id(transfer.total_size)?)
        .bind(transfer.access_mode)
        .bind(now)
        .execute(&self.pool)
        .await?;
        require_one_changed(result.rows_affected(), "start receive")?;
        Ok(())
    }

    pub(crate) async fn transfer_local_id(&self, transfer_id: u64) -> Result<String> {
        let row = sqlx::query("SELECT local_id FROM transfers WHERE transfer_id = ?1")
            .bind(to_db_id(transfer_id)?)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get(0))
    }

    pub(crate) async fn output_identity(&self, id: &str, locator: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT file_identity FROM invitation_receive_outputs WHERE local_id = ? AND locator = ?",
        )
        .bind(id)
        .bind(locator)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn checkpoint_output(
        &self,
        id: &str,
        locator: &str,
        identity: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO invitation_receive_outputs (local_id, locator, file_identity)
             VALUES (?, ?, ?) ON CONFLICT(local_id, locator)
             DO UPDATE SET file_identity = excluded.file_identity",
        )
        .bind(id)
        .bind(locator)
        .bind(identity)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn forget_output(&self, id: &str, locator: &str) -> Result<()> {
        sqlx::query("DELETE FROM invitation_receive_outputs WHERE local_id = ? AND locator = ?")
            .bind(id)
            .bind(locator)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn record_received_artifact(
        &self,
        artifact: ReceivedArtifactInsert<'_>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO received_artifacts (
                id, transfer_local_id, protocol_transfer_id, relative_path,
                locator_kind, locator, logical_size, published_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(transfer_local_id, relative_path) DO UPDATE SET
                locator_kind = excluded.locator_kind,
                locator = excluded.locator,
                logical_size = excluded.logical_size,
                published_at = excluded.published_at
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(artifact.transfer_local_id)
        .bind(to_db_id(artifact.protocol_transfer_id)?)
        .bind(artifact.relative_path)
        .bind(locator_kind_to_storage(&artifact.locator_kind))
        .bind(artifact.locator)
        .bind(to_db_id(artifact.logical_size)?)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn list_received_artifacts(&self) -> Result<Vec<ReceivedArtifact>> {
        let rows = sqlx::query(
            r#"
            SELECT id, transfer_local_id, protocol_transfer_id, relative_path,
                   locator_kind, locator, logical_size, published_at
            FROM received_artifacts
            ORDER BY published_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ReceivedArtifact {
                    id: row.get("id"),
                    transfer_local_id: row.get("transfer_local_id"),
                    protocol_transfer_id: row.get::<i64, _>("protocol_transfer_id") as u64,
                    relative_path: row.get("relative_path"),
                    locator_kind: locator_kind_from_storage(&row.get::<String, _>("locator_kind"))?,
                    locator: row.get("locator"),
                    logical_size: row.get::<i64, _>("logical_size") as u64,
                    published_at: row.get("published_at"),
                })
            })
            .collect()
    }

    pub(crate) async fn complete_share_import(&self, transfer: TransferUpsert<'_>) -> Result<()> {
        self.maybe_fail_write()?;
        if transfer.direction != TransferDirection::Send
            || transfer.status != TransferStatus::Sharing
        {
            anyhow::bail!("share import must complete in the sharing state");
        }
        let result = sqlx::query(
            r#"
            UPDATE transfers
            SET status = ?1,
                transfer_name = ?2,
                sender_name = ?3,
                content_hash = ?4,
                ticket = ?5,
                file_count = ?6,
                total_size = ?7,
                access_mode = ?8,
                updated_at = ?9
            WHERE transfer_id = ?10
              AND direction = 'send'
              AND status = 'importing'
            "#,
        )
        .bind(transfer.status.as_str())
        .bind(transfer.transfer_name)
        .bind(transfer.sender_name)
        .bind(transfer.content_hash)
        .bind(transfer.ticket)
        .bind(to_db_id(transfer.file_count)?)
        .bind(to_db_id(transfer.total_size)?)
        .bind(transfer.access_mode)
        .bind(now_ms())
        .bind(to_db_id(transfer.transfer_id)?)
        .execute(&self.pool)
        .await?;
        require_one_changed(result.rows_affected(), "complete share import")?;
        Ok(())
    }

    pub(crate) async fn cancel_share_preparation(&self, transfer_id: u64) -> Result<()> {
        self.maybe_fail_write()?;
        let result = sqlx::query(
            "UPDATE transfers SET status = CASE status WHEN 'sharing' THEN 'stopped' ELSE 'cancelled' END, updated_at = ?1
             WHERE transfer_id = ?2 AND direction = 'send' AND status IN ('importing', 'sharing')",
        )
        .bind(now_ms())
        .bind(to_db_id(transfer_id)?)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            let status: Option<String> = sqlx::query_scalar(
                "SELECT status FROM transfers WHERE transfer_id = ?1 AND direction = 'send'",
            )
            .bind(to_db_id(transfer_id)?)
            .fetch_optional(&self.pool)
            .await?;
            if !matches!(status.as_deref(), Some("cancelled" | "stopped" | "failed")) {
                anyhow::bail!("share preparation was not found");
            }
        }
        Ok(())
    }

    pub(crate) async fn transition_transfer_status(
        &self,
        transfer_id: u64,
        expected: TransferStatus,
        next: TransferStatus,
    ) -> Result<()> {
        self.maybe_fail_write()?;
        if !expected.can_transition_to(next) {
            anyhow::bail!(
                "illegal transfer status transition: {} -> {}",
                expected.as_str(),
                next.as_str()
            );
        }
        let result = sqlx::query(
            r#"
            UPDATE transfers
            SET status = ?1, updated_at = ?2
            WHERE transfer_id = ?3 AND status = ?4
            "#,
        )
        .bind(next.as_str())
        .bind(now_ms())
        .bind(to_db_id(transfer_id)?)
        .bind(expected.as_str())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            let current = sqlx::query("SELECT status FROM transfers WHERE transfer_id = ?1")
                .bind(to_db_id(transfer_id)?)
                .fetch_optional(&self.pool)
                .await?;
            if current
                .as_ref()
                .map(|row| row.get::<String, _>(0))
                .as_deref()
                == Some(next.as_str())
            {
                return Ok(());
            }
        }
        require_one_changed(result.rows_affected(), "transition transfer status")?;
        Ok(())
    }

    pub(crate) async fn update_active_share_access_mode(
        &self,
        transfer_id: u64,
        access_mode: &str,
    ) -> Result<()> {
        self.maybe_fail_write()?;
        let result = sqlx::query(
            r#"
            UPDATE transfers
            SET access_mode = ?1, updated_at = ?2
            WHERE transfer_id = ?3
              AND direction = 'send'
              AND status = 'sharing'
            "#,
        )
        .bind(access_mode)
        .bind(now_ms())
        .bind(to_db_id(transfer_id)?)
        .execute(&self.pool)
        .await?;
        require_one_changed(result.rows_affected(), "update active share access mode")?;
        Ok(())
    }

    pub(crate) async fn recover_interrupted_transfers(&self) -> Result<Vec<RecoveredTransfer>> {
        self.maybe_fail_write()?;
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query(
            r#"
            SELECT transfer_id, direction, status
            FROM transfers
            WHERE status IN ('importing', 'receiving')
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&mut *transaction)
        .await?;
        let recovered = rows
            .into_iter()
            .map(|row| {
                Ok(RecoveredTransfer {
                    transfer_id: row.get::<i64, _>("transfer_id") as u64,
                    direction: TransferDirection::try_from(
                        row.get::<String, _>("direction").as_str(),
                    )?,
                    previous_status: TransferStatus::try_from(
                        row.get::<String, _>("status").as_str(),
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        if !recovered.is_empty() {
            sqlx::query(
                r#"
                UPDATE transfers
                SET status = 'failed', updated_at = ?1
                WHERE status IN ('importing', 'receiving')
                "#,
            )
            .bind(now_ms())
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(recovered)
    }

    #[cfg(test)]
    pub(crate) fn fail_next_write(&self) {
        self.fail_next_write.store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn fail_receive_history_after_dependants(&self) {
        self.fail_receive_history_after_dependants
            .store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    fn maybe_fail_write(&self) -> Result<()> {
        if self.fail_next_write.swap(false, Ordering::SeqCst) {
            anyhow::bail!("injected repository write failure");
        }
        Ok(())
    }

    #[cfg(not(test))]
    fn maybe_fail_write(&self) -> Result<()> {
        Ok(())
    }

    pub(crate) async fn list_active_shares(&self) -> Result<Vec<PersistedShare>> {
        let rows = sqlx::query(
            r#"
            SELECT transfer_id, local_id, content_hash, ticket, access_mode
            FROM transfers
            WHERE direction = 'send'
              AND status = 'sharing'
              AND content_hash IS NOT NULL
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PersistedShare {
                transfer_id: row.get::<i64, _>(0) as u64,
                local_id: row.get::<String, _>(1),
                content_hash: row.get::<String, _>(2),
                ticket: row.get::<Option<String>, _>(3),
                access_mode: row.get::<String, _>(4),
            })
            .collect())
    }

    pub(crate) async fn insert_event(&self, event: &CoreEvent, max_history: u64) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO transfer_events (
                id, revision, timestamp, scope, transfer_id, direction, phase, kind, data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);
            "#,
        )
        .bind(&event.id)
        .bind(to_db_id(event.revision)?)
        .bind(event.timestamp)
        .bind(&event.scope)
        .bind(event.transfer_id.map(to_db_id).transpose()?)
        .bind(&event.direction)
        .bind(&event.phase)
        .bind(&event.kind)
        .bind(&event.data_json)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"
            DELETE FROM transfer_events
            WHERE id NOT IN (
                SELECT id FROM transfer_events ORDER BY timestamp DESC, id DESC LIMIT ?1
            )
            "#,
        )
        .bind(to_db_id(max_history)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn send_exists(&self, transfer_id: u64, content_hash: &str) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM transfers
                WHERE transfer_id = ?1
                  AND content_hash = ?2
                  AND direction = 'send'
                  AND status = 'sharing'
            )
            "#,
        )
        .bind(to_db_id(transfer_id)?)
        .bind(content_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>(0) != 0)
    }

    pub(crate) async fn send_sender_name(
        &self,
        transfer_id: u64,
        content_hash: &str,
    ) -> Result<Option<String>> {
        let row = sqlx::query(
            r#"
            SELECT sender_name FROM transfers
            WHERE transfer_id = ?1 AND content_hash = ?2
              AND direction = 'send' AND status = 'sharing'
            "#,
        )
        .bind(to_db_id(transfer_id)?)
        .bind(content_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|row| row.get("sender_name")))
    }

    pub(crate) async fn list_transfers(&self) -> Result<Vec<StoredTransfer>> {
        let rows = sqlx::query(
            r#"
            SELECT transfer_id, direction, status, transfer_name, content_hash, ticket,
                   local_id, protocol_transfer_id, peer_id,
                   file_count, total_size, access_mode, created_at, updated_at
            FROM transfers
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_transfer).collect()
    }

    pub(crate) async fn delete_transfer(&self, transfer_id: u64) -> Result<()> {
        self.maybe_fail_write()?;
        let transfer_id = to_db_id(transfer_id)?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM receiver_requests WHERE transfer_id = ?1")
            .bind(transfer_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM transfer_events WHERE transfer_id = ?1")
            .bind(transfer_id)
            .execute(&mut *transaction)
            .await?;
        let deleted = sqlx::query(
            "DELETE FROM transfers WHERE transfer_id = ?1
             AND status NOT IN ('receiving', 'importing', 'sharing')",
        )
        .bind(transfer_id)
        .execute(&mut *transaction)
        .await?;
        require_one_changed(deleted.rows_affected(), "delete transfer")?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn delete_receive_history(&self) -> Result<u64> {
        self.maybe_fail_write()?;
        let mut transaction = self.pool.begin().await?;

        // Delete dependants before their transfer rows. Keep the terminal-state
        // predicate on every statement so receive work that is still active and
        // every send record remain outside this transaction's scope.
        sqlx::query(
            r#"
            DELETE FROM receiver_requests
            WHERE transfer_id IN (
                SELECT transfer_id
                FROM transfers
                WHERE direction = 'receive'
                  AND status IN ('done', 'failed', 'cancelled')
            )
            "#,
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"
            DELETE FROM transfer_events
            WHERE transfer_id IN (
                SELECT transfer_id
                FROM transfers
                WHERE direction = 'receive'
                  AND status IN ('done', 'failed', 'cancelled')
            )
            "#,
        )
        .execute(&mut *transaction)
        .await?;
        #[cfg(test)]
        if self
            .fail_receive_history_after_dependants
            .swap(false, Ordering::SeqCst)
        {
            anyhow::bail!("injected receive history failure after dependant deletion");
        }
        let deleted = sqlx::query(
            r#"
            DELETE FROM transfers
            WHERE direction = 'receive'
              AND status IN ('done', 'failed', 'cancelled')
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(deleted.rows_affected())
    }

    pub(crate) async fn list_events(
        &self,
        transfer_id: Option<u64>,
        limit: u64,
    ) -> Result<Vec<CoreEvent>> {
        let rows = if let Some(transfer_id) = transfer_id {
            sqlx::query(
                r#"
                SELECT id, revision, timestamp, scope, transfer_id, direction, phase, kind, data_json
                FROM transfer_events
                WHERE transfer_id = ?1
                ORDER BY timestamp ASC, revision ASC
                LIMIT ?2
                "#,
            )
            .bind(to_db_id(transfer_id)?)
            .bind(to_db_id(limit)?)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, revision, timestamp, scope, transfer_id, direction, phase, kind, data_json
                FROM transfer_events
                ORDER BY timestamp DESC, revision DESC
                LIMIT ?1
                "#,
            )
            .bind(to_db_id(limit)?)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.into_iter().map(row_to_event).collect())
    }
}

fn require_one_changed(rows_affected: u64, operation: &str) -> Result<()> {
    if rows_affected != 1 {
        anyhow::bail!("{operation} expected one matching transfer, changed {rows_affected}");
    }
    Ok(())
}

fn to_db_id(value: u64) -> Result<i64> {
    i64::try_from(value).context("transfer id exceeds SQLite signed integer range")
}

fn locator_kind_to_storage(kind: &ReceivedLocatorKind) -> &'static str {
    match kind {
        ReceivedLocatorKind::FilesystemPath => "filesystem_path",
        ReceivedLocatorKind::AndroidMediaStore => "android_media_store",
        ReceivedLocatorKind::AndroidDocument => "android_document",
    }
}

fn locator_kind_from_storage(value: &str) -> Result<ReceivedLocatorKind> {
    match value {
        "filesystem_path" => Ok(ReceivedLocatorKind::FilesystemPath),
        "android_media_store" => Ok(ReceivedLocatorKind::AndroidMediaStore),
        "android_document" => Ok(ReceivedLocatorKind::AndroidDocument),
        _ => anyhow::bail!("unknown received artifact locator kind: {value}"),
    }
}

fn row_to_transfer(row: sqlx::sqlite::SqliteRow) -> Result<StoredTransfer> {
    let direction = row.get::<String, _>("direction");
    let status = row.get::<String, _>("status");
    Ok(StoredTransfer {
        local_id: row.get("local_id"),
        transfer_id: row.get::<i64, _>("transfer_id") as u64,
        peer_id: row.get("peer_id"),
        direction: TransferDirection::try_from(direction.as_str())?
            .as_str()
            .to_string(),
        status: TransferStatus::try_from(status.as_str())?
            .as_str()
            .to_string(),
        transfer_name: row.get("transfer_name"),
        content_hash: row.get("content_hash"),
        ticket: row.get("ticket"),
        file_count: row.get::<i64, _>("file_count") as u64,
        total_size: row.get::<i64, _>("total_size") as u64,
        access_mode: mode_from_storage(&row.get::<String, _>("access_mode")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_event(row: sqlx::sqlite::SqliteRow) -> CoreEvent {
    CoreEvent {
        id: row.get("id"),
        revision: row.get::<i64, _>("revision") as u64,
        timestamp: row.get("timestamp"),
        scope: row.get("scope"),
        transfer_id: row
            .get::<Option<i64>, _>("transfer_id")
            .map(|value| value as u64),
        direction: row.get("direction"),
        phase: row.get("phase"),
        kind: row.get("kind"),
        data_json: row.get("data_json"),
    }
}

fn row_to_receiver_request(row: sqlx::sqlite::SqliteRow) -> ReceiverRequest {
    let status = row.get::<String, _>("status");
    ReceiverRequest {
        id: row.get("id"),
        transfer_id: row.get::<i64, _>("transfer_id") as u64,
        remote_endpoint_id: row.get("remote_endpoint_id"),
        transfer_name: row.get("transfer_name"),
        receiver_name: row.get("receiver_name"),
        receiver_device_name: row.get("receiver_device_name"),
        app_version: row.get("app_version"),
        status: ReceiverRequestStatus::try_from(status.as_str())
            .map(|status| status.as_str().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
        reason: row.get("reason"),
        requested_at: row.get("requested_at"),
        responded_at: row.get("responded_at"),
        completed_at: row.get("completed_at"),
    }
}
