//! Durable authorization, completion, release, and terminal transactions.

use sqlx::Row;

use super::store::{row_to_full, TargetedTransferStore};
use super::TargetedTransferRow;
use crate::{api::TargetedTransferState, error::VnidropError, util::now_ms};

impl TargetedTransferStore {
    pub(crate) async fn list_pending_authorization_deliveries(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT t.id, t.protocol_transfer_id, t.sender_endpoint_id, t.receiver_endpoint_id,
                   t.manifest_id, t.content_hash, t.transfer_name, t.file_count, t.total_size,
                   t.verified_bytes, t.blob_ticket, t.authorization_secret_handle, t.role,
                   t.state, t.created_at, t.updated_at
            FROM targeted_transfers t
            INNER JOIN targeted_authorization_delivery_outbox o ON o.transfer_id = t.id
            WHERE t.role = 'sender' AND t.state = 'approved' AND o.next_attempt_at <= ?1
            ORDER BY o.next_attempt_at, o.created_at
            "#,
        )
        .bind(now_ms())
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    pub(crate) async fn clear_pending_authorization_delivery(
        &self,
        id: &str,
    ) -> Result<(), VnidropError> {
        sqlx::query("DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn defer_pending_authorization_delivery(
        &self,
        id: &str,
        next_attempt_at: i64,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE targeted_authorization_delivery_outbox SET next_attempt_at = ?2 WHERE transfer_id = ?1",
        )
        .bind(id)
        .bind(next_attempt_at)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn complete_receiver_and_enqueue(
        &self,
        id: &str,
        verified_bytes: u64,
    ) -> Result<(), VnidropError> {
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'completed', verified_bytes = ?2, updated_at = ?3
            WHERE id = ?1 AND role = 'receiver' AND state = 'transferring'
            "#,
        )
        .bind(id)
        .bind(verified_bytes as i64)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            return Err(VnidropError::InvalidTransition {
                reason: "receiver transfer cannot be completed".to_string(),
            });
        }
        sqlx::query(
            "INSERT OR IGNORE INTO targeted_completion_outbox (transfer_id, created_at, next_attempt_at) VALUES (?1, ?2, ?2)",
        )
        .bind(id)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn advance_verified_bytes(
        &self,
        id: &str,
        verified_bytes: u64,
    ) -> Result<bool, VnidropError> {
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET verified_bytes = MIN(total_size, MAX(verified_bytes, ?2)), updated_at = ?3
            WHERE id = ?1 AND role = 'receiver' AND state = 'transferring'
              AND ?2 > verified_bytes
            "#,
        )
        .bind(id)
        .bind(verified_bytes as i64)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn list_pending_completions(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT t.id, t.protocol_transfer_id, t.sender_endpoint_id, t.receiver_endpoint_id,
                   t.manifest_id, t.content_hash, t.transfer_name, t.file_count, t.total_size,
                   t.verified_bytes, t.blob_ticket, t.authorization_secret_handle, t.role,
                   t.state, t.created_at, t.updated_at
            FROM targeted_transfers t
            INNER JOIN targeted_completion_outbox o ON o.transfer_id = t.id
            WHERE t.role = 'receiver' AND t.state = 'completed' AND o.next_attempt_at <= ?1
            ORDER BY o.next_attempt_at, o.created_at
            "#,
        )
        .bind(now_ms())
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    pub(crate) async fn clear_pending_completion(&self, id: &str) -> Result<(), VnidropError> {
        sqlx::query("DELETE FROM targeted_completion_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn defer_pending_completion(
        &self,
        id: &str,
        next_attempt_at: i64,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE targeted_completion_outbox SET next_attempt_at = ?2 WHERE transfer_id = ?1",
        )
        .bind(id)
        .bind(next_attempt_at)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn list_completed_sender_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT t.id, t.protocol_transfer_id, t.sender_endpoint_id, t.receiver_endpoint_id,
                   t.manifest_id, t.content_hash, t.transfer_name, t.file_count, t.total_size,
                   t.verified_bytes, t.blob_ticket, t.authorization_secret_handle, t.role,
                   t.state, t.created_at, t.updated_at
            FROM targeted_transfers t
            INNER JOIN targeted_payload_release_outbox o ON o.transfer_id = t.id
            WHERE t.role = 'sender' AND t.state = 'completed'
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    pub(crate) async fn clear_pending_payload_release(&self, id: &str) -> Result<(), VnidropError> {
        sqlx::query("DELETE FROM targeted_payload_release_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn authorization_secret_handles(
        &self,
    ) -> Result<std::collections::HashSet<String>, VnidropError> {
        let rows = sqlx::query(
            "SELECT authorization_secret_handle FROM targeted_transfers WHERE authorization_secret_handle IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    pub(crate) async fn clear_authorization(&self, id: &str) -> Result<(), VnidropError> {
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM targeted_completion_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM targeted_payload_release_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET blob_ticket = NULL,
                authorization_secret_handle = NULL
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    /// Commit a local terminal denial and optional secret cleanup atomically.
    pub(crate) async fn transition_terminal(
        &self,
        id: &str,
        state: TargetedTransferState,
        clear_authorization: bool,
    ) -> Result<bool, VnidropError> {
        let state = match state {
            TargetedTransferState::Cancelled => "cancelled",
            TargetedTransferState::Deleted => "deleted",
            _ => {
                return Err(VnidropError::invalid_input(anyhow::anyhow!(
                    "terminal transition requires cancelled or deleted"
                )))
            }
        };
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM targeted_accepted_offer_intents WHERE transfer_id = ?1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        if clear_authorization {
            sqlx::query("DELETE FROM targeted_completion_outbox WHERE transfer_id = ?1")
                .bind(id)
                .execute(&mut *transaction)
                .await
                .map_err(VnidropError::repository)?;
            sqlx::query("DELETE FROM targeted_payload_release_outbox WHERE transfer_id = ?1")
                .bind(id)
                .execute(&mut *transaction)
                .await
                .map_err(VnidropError::repository)?;
        }
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = ?2,
                blob_ticket = CASE WHEN ?3 THEN NULL ELSE blob_ticket END,
                authorization_secret_handle = CASE WHEN ?3 THEN NULL ELSE authorization_secret_handle END,
                updated_at = ?4
            WHERE id = ?1 AND state != ?2
              AND (
                (?2 = 'deleted' AND state != 'deleted')
                OR (?2 = 'cancelled' AND state NOT IN ('completed', 'declined', 'cancelled', 'failed', 'deleted'))
              )
            "#,
        )
        .bind(id)
        .bind(state)
        .bind(clear_authorization)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(result.rows_affected() == 1)
    }
}
