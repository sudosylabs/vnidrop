use super::*;

impl TargetedTransferStore {
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
        self.reconciliation_changed.notify_one();
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
        self.reconciliation_changed.notify_one();
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
}
