//! Durable targeted-transfer rows (schema + queries).
//!
//! This is the domain store adapter for targeted transfers. Callers use store
//! methods — not a raw SQL pool.

use sqlx::{Row, SqlitePool};

use crate::{
    api::{TargetedTransfer, TargetedTransferRole, TargetedTransferState},
    error::VnidropError,
    util::now_ms,
};

#[derive(Clone)]
pub(crate) struct TargetedTransferStore {
    pub(super) pool: SqlitePool,
}

impl TargetedTransferStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn persist_accepted_offer_intent(
        &self,
        offer: &crate::api::PendingTargetedOffer,
    ) -> Result<(), VnidropError> {
        let result = sqlx::query(
            r#"
            INSERT INTO targeted_accepted_offer_intents (
                transfer_id, sender_endpoint_id, receiver_endpoint_id, manifest_id,
                content_hash, transfer_name, file_count, total_size, protocol_version, accepted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(transfer_id) DO UPDATE SET accepted_at = excluded.accepted_at
            WHERE sender_endpoint_id = excluded.sender_endpoint_id
              AND receiver_endpoint_id = excluded.receiver_endpoint_id
              AND manifest_id = excluded.manifest_id
              AND content_hash = excluded.content_hash
              AND transfer_name = excluded.transfer_name
              AND file_count = excluded.file_count
              AND total_size = excluded.total_size
              AND protocol_version = excluded.protocol_version
            "#,
        )
        .bind(&offer.transfer_id)
        .bind(&offer.sender_endpoint_id)
        .bind(&offer.receiver_endpoint_id)
        .bind(&offer.manifest_id)
        .bind(&offer.content_hash)
        .bind(&offer.transfer_name)
        .bind(offer.file_count as i64)
        .bind(offer.total_size as i64)
        .bind(offer.protocol_version as i64)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "accepted targeted offer conflicts with durable consent"
            )));
        }
        Ok(())
    }

    pub(crate) async fn clear_accepted_offer_intent(&self, id: &str) -> Result<(), VnidropError> {
        sqlx::query("DELETE FROM targeted_accepted_offer_intents WHERE transfer_id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn clear_accepted_intent_if_sender(
        &self,
        id: &str,
        sender_endpoint_id: &str,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "DELETE FROM targeted_accepted_offer_intents WHERE transfer_id = ?1 AND sender_endpoint_id = ?2",
        )
        .bind(id)
        .bind(sender_endpoint_id)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn list_accepted_intent_senders(
        &self,
    ) -> Result<Vec<(String, String)>, VnidropError> {
        let rows = sqlx::query(
            "SELECT transfer_id, sender_endpoint_id FROM targeted_accepted_offer_intents",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect())
    }

    pub(crate) async fn contains_protocol_id(
        &self,
        protocol_transfer_id: u64,
    ) -> Result<bool, VnidropError> {
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM targeted_transfers WHERE protocol_transfer_id = ?1)",
        )
        .bind(protocol_transfer_id as i64)
        .fetch_one(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(row.get::<i64, _>(0) != 0)
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

    pub(crate) async fn mark_sender_completed(&self, id: &str) -> Result<bool, VnidropError> {
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'completed', verified_bytes = total_size, updated_at = ?2
            WHERE id = ?1 AND role = 'sender'
              AND state IN ('approved', 'connecting', 'transferring', 'interrupted')
            "#,
        )
        .bind(id)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            let completed = sqlx::query(
                "SELECT EXISTS(SELECT 1 FROM targeted_transfers WHERE id = ?1 AND role = 'sender' AND state = 'completed')",
            )
            .bind(id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?
            .get::<i64, _>(0)
                != 0;
            if completed {
                sqlx::query(
                    "DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id = ?1",
                )
                .bind(id)
                .execute(&mut *transaction)
                .await
                .map_err(VnidropError::repository)?;
            }
            transaction
                .commit()
                .await
                .map_err(VnidropError::repository)?;
            return if completed {
                Ok(false)
            } else {
                Err(VnidropError::InvalidTransition {
                    reason: "sender transfer cannot be completed".to_string(),
                })
            };
        }
        sqlx::query("DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id = ?1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        sqlx::query(
            "INSERT OR IGNORE INTO targeted_payload_release_outbox (transfer_id, created_at) VALUES (?1, ?2)",
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
        Ok(true)
    }

    pub(crate) async fn finalize_sender_authorization_and_enqueue(
        &self,
        id: &str,
        blob_ticket: &str,
        authorization_secret_handle: &str,
    ) -> Result<(), VnidropError> {
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        let now = now_ms();
        let result = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'approved', blob_ticket = ?2,
                authorization_secret_handle = ?3, updated_at = ?4
            WHERE id = ?1 AND role = 'sender' AND state = 'awaiting_approval'
            "#,
        )
        .bind(id)
        .bind(blob_ticket)
        .bind(authorization_secret_handle)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 0 {
            return Err(VnidropError::InvalidTransition {
                reason: "sender transfer cannot be approved".to_string(),
            });
        }
        sqlx::query(
            "INSERT OR REPLACE INTO targeted_authorization_delivery_outbox (transfer_id, created_at, next_attempt_at) VALUES (?1, ?2, ?2)",
        )
        .bind(id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn persist_receiver_authorization_and_consume_intent(
        &self,
        auth: &crate::targeted_transfer::TargetedAuthorization,
        authorization_secret_handle: &str,
    ) -> Result<bool, VnidropError> {
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        let matches = sqlx::query(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM targeted_accepted_offer_intents
                WHERE transfer_id = ?1 AND sender_endpoint_id = ?2 AND receiver_endpoint_id = ?3
                  AND manifest_id = ?4 AND content_hash = ?5 AND transfer_name = ?6
                  AND file_count = ?7 AND total_size = ?8 AND protocol_version = ?9
            )
            "#,
        )
        .bind(&auth.transfer_id)
        .bind(&auth.sender_endpoint_id)
        .bind(&auth.receiver_endpoint_id)
        .bind(&auth.manifest_id)
        .bind(&auth.content_hash)
        .bind(&auth.transfer_name)
        .bind(auth.file_count as i64)
        .bind(auth.total_size as i64)
        .bind(auth.protocol_version as i64)
        .fetch_one(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?
        .get::<i64, _>(0)
            != 0;
        if !matches {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "authorization does not match durable receiver consent"
            )));
        }
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO targeted_transfers (
                id, protocol_transfer_id, sender_endpoint_id, receiver_endpoint_id,
                manifest_id, content_hash, transfer_name, file_count, total_size,
                verified_bytes, blob_ticket, authorization_secret_handle, role,
                state, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, 'receiver', 'approved', ?12, ?12)
            "#,
        )
        .bind(&auth.transfer_id)
        .bind(auth.protocol_transfer_id as i64)
        .bind(&auth.sender_endpoint_id)
        .bind(&auth.receiver_endpoint_id)
        .bind(&auth.manifest_id)
        .bind(&auth.content_hash)
        .bind(&auth.transfer_name)
        .bind(auth.file_count as i64)
        .bind(auth.total_size as i64)
        .bind(&auth.blob_ticket)
        .bind(authorization_secret_handle)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM targeted_accepted_offer_intents WHERE transfer_id = ?1")
            .bind(&auth.transfer_id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(true)
    }

    pub(crate) async fn fail_resumable_and_clear_delivery(
        &self,
        id: &str,
    ) -> Result<bool, VnidropError> {
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        let result = sqlx::query(
            "UPDATE targeted_transfers SET state = 'failed', updated_at = ?2 WHERE id = ?1 AND state IN ('approved', 'connecting', 'transferring', 'interrupted')",
        )
        .bind(id)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        if result.rows_affected() == 1 {
            sqlx::query(
                "DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id = ?1",
            )
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;
        }
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn get(&self, id: &str) -> Result<Option<TargetedTransfer>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT id, role, sender_endpoint_id, receiver_endpoint_id, manifest_id, transfer_name,
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
            SELECT id, role, sender_endpoint_id, receiver_endpoint_id, manifest_id, transfer_name,
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
            ORDER BY created_at, id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    pub(crate) async fn list_resumable_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT id, protocol_transfer_id, sender_endpoint_id, receiver_endpoint_id,
                   manifest_id, content_hash, transfer_name, file_count, total_size,
                   verified_bytes, blob_ticket, authorization_secret_handle, role,
                   state, created_at, updated_at
            FROM targeted_transfers
            WHERE state IN ('approved', 'connecting', 'transferring', 'interrupted')
            ORDER BY created_at, id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    #[cfg(test)]
    pub(crate) async fn corrupt_content_hash_for_test(&self, id: &str) -> Result<(), VnidropError> {
        sqlx::query("UPDATE targeted_transfers SET content_hash = ?2 WHERE id = ?1")
            .bind(id)
            .bind(iroh_blobs::Hash::new([0xff; 32]).to_string())
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn cancel_by_peer(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Vec<String>, VnidropError> {
        let now = now_ms();
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        sqlx::query(
            "DELETE FROM targeted_accepted_offer_intents WHERE sender_endpoint_id = ?1 OR receiver_endpoint_id = ?1",
        )
        .bind(peer_endpoint_id)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        sqlx::query(
            "DELETE FROM targeted_authorization_delivery_outbox WHERE transfer_id IN (SELECT id FROM targeted_transfers WHERE sender_endpoint_id = ?1 OR receiver_endpoint_id = ?1)",
        )
        .bind(peer_endpoint_id)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        let rows = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'cancelled', blob_ticket = NULL,
                authorization_secret_handle = NULL, updated_at = ?2
            WHERE (sender_endpoint_id = ?1 OR receiver_endpoint_id = ?1)
              AND state NOT IN ('completed', 'declined', 'cancelled', 'failed', 'deleted')
            RETURNING id
            "#,
        )
        .bind(peer_endpoint_id)
        .bind(now)
        .fetch_all(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    pub(crate) async fn ids_for_peer(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Vec<String>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT id FROM targeted_transfers
            WHERE sender_endpoint_id = ?1 OR receiver_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    pub(crate) async fn authorization_rows(
        &self,
    ) -> Result<Vec<TargetedTransferRow>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT id, protocol_transfer_id, sender_endpoint_id, receiver_endpoint_id,
                   manifest_id, content_hash, transfer_name, file_count, total_size,
                   verified_bytes, blob_ticket, authorization_secret_handle, role,
                   state, created_at, updated_at
            FROM targeted_transfers
            WHERE authorization_secret_handle IS NOT NULL
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_full).collect()
    }

    pub(crate) async fn mark_interrupted_in_flight(&self) -> Result<Vec<String>, VnidropError> {
        let now = now_ms();
        let rows = sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = 'interrupted', updated_at = ?1
            WHERE state IN ('connecting', 'transferring')
            RETURNING id
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
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
        role: parse_role(&row.get::<String, _>("role"))?,
        sender_endpoint_id: row.get("sender_endpoint_id"),
        receiver_endpoint_id: row.get("receiver_endpoint_id"),
        manifest_id: row.get("manifest_id"),
        transfer_name: row.get("transfer_name"),
        file_count: row.get::<i64, _>("file_count") as u64,
        total_size: row.get::<i64, _>("total_size") as u64,
        verified_bytes: row.try_get::<i64, _>("verified_bytes").unwrap_or(0) as u64,
        state: parse_state(&row.get::<String, _>("state"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn row_to_full(
    row: sqlx::sqlite::SqliteRow,
) -> Result<TargetedTransferRow, VnidropError> {
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
