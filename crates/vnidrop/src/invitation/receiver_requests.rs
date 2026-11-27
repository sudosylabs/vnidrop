use super::*;

impl Repository {
    pub(crate) async fn insert_receiver_request(
        &self,
        request: ReceiverRequestInsert<'_>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO receiver_requests (
                id, transfer_id, remote_endpoint_id, transfer_name,
                receiver_name, receiver_device_name, app_version, status,
                reason, requested_at, responded_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'requested', NULL, ?8, NULL)
            "#,
        )
        .bind(request.id)
        .bind(to_db_id(request.transfer_id)?)
        .bind(request.remote_endpoint_id)
        .bind(request.transfer_name)
        .bind(request.receiver_name)
        .bind(request.receiver_device_name)
        .bind(request.app_version)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn refuse_pending_requests(
        &self,
        transfer_id: u64,
        reason: &str,
    ) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "UPDATE receiver_requests SET status = 'refused', reason = ?2, responded_at = ?3
             WHERE transfer_id = ?1 AND status = 'requested' RETURNING id",
        )
        .bind(to_db_id(transfer_id)?)
        .bind(reason)
        .bind(now_ms())
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn update_receiver_request_status(
        &self,
        id: &str,
        status: ReceiverRequestStatus,
        reason: Option<&str>,
    ) -> Result<()> {
        self.maybe_fail_write()?;
        let result = sqlx::query(
            r#"
            UPDATE receiver_requests
            SET status = ?1, reason = ?2, responded_at = ?3
            WHERE id = ?4
              AND status = 'requested'
            "#,
        )
        .bind(status.as_str())
        .bind(reason)
        .bind(now_ms())
        .bind(id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            anyhow::bail!("receiver request not found or already handled");
        }
        Ok(())
    }

    pub(crate) async fn set_receiver_receipt_token(
        &self,
        id: &str,
        token_hash: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE receiver_requests SET receipt_token_hash = ?1 WHERE id = ?2 AND status = 'accepted'",
        )
        .bind(token_hash)
        .bind(id)
        .execute(&self.pool)
        .await?;
        require_one_changed(result.rows_affected(), "attach receiver receipt token")
    }

    pub(crate) async fn complete_receiver_delivery(
        &self,
        id: &str,
        transfer_id: u64,
        remote_endpoint_id: &str,
        token_hash: &str,
    ) -> Result<DeliveryCompletion> {
        let transfer_id = to_db_id(transfer_id)?;
        let completed = sqlx::query(
            r#"
            UPDATE receiver_requests
            SET status = 'completed', completed_at = ?1
            WHERE id = ?2 AND transfer_id = ?3 AND remote_endpoint_id = ?4 AND receipt_token_hash = ?5
              AND status = 'accepted'
            RETURNING COALESCE(receiver_device_name, receiver_name) AS remote_display_name
            "#,
        )
        .bind(now_ms())
        .bind(id)
        .bind(transfer_id)
        .bind(remote_endpoint_id)
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = completed {
            return Ok(DeliveryCompletion::Recorded {
                remote_display_name: row.get("remote_display_name"),
            });
        }
        let already_recorded = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM receiver_requests
                WHERE id = ?1 AND transfer_id = ?2 AND remote_endpoint_id = ?3
                  AND receipt_token_hash = ?4 AND status = 'completed'
            )
            "#,
        )
        .bind(id)
        .bind(transfer_id)
        .bind(remote_endpoint_id)
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await?;
        if !already_recorded {
            anyhow::bail!("delivery receipt did not match an accepted receiver request");
        }
        Ok(DeliveryCompletion::AlreadyRecorded)
    }

    pub(crate) async fn fail_receiver_delivery(
        &self,
        id: &str,
        transfer_id: u64,
        remote_endpoint_id: &str,
        token_hash: &str,
        reason: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE receiver_requests
            SET status = 'failed', reason = ?1
            WHERE id = ?2 AND transfer_id = ?3 AND remote_endpoint_id = ?4 AND receipt_token_hash = ?5
              AND status = 'accepted'
            "#,
        )
        .bind(reason)
        .bind(id)
        .bind(to_db_id(transfer_id)?)
        .bind(remote_endpoint_id)
        .bind(token_hash)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 1 {
            return Ok(true);
        }
        let already_recorded = sqlx::query(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM receiver_requests
                WHERE id = ?1 AND transfer_id = ?2 AND remote_endpoint_id = ?3
                  AND receipt_token_hash = ?4 AND status = 'failed'
            )
            "#,
        )
        .bind(id)
        .bind(to_db_id(transfer_id)?)
        .bind(remote_endpoint_id)
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await?
        .get::<i64, _>(0)
            != 0;
        if already_recorded {
            Ok(false)
        } else {
            anyhow::bail!("delivery failure did not match an accepted receiver request")
        }
    }

    pub(crate) async fn expire_pending_receiver_requests(&self, reason: &str) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE receiver_requests
            SET status = 'expired', reason = ?1, responded_at = ?2
            WHERE status = 'requested'
            "#,
        )
        .bind(reason)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub(crate) async fn list_receiver_requests(
        &self,
        transfer_id: u64,
    ) -> Result<Vec<ReceiverRequest>> {
        let rows = sqlx::query(
            r#"
            SELECT id, transfer_id, remote_endpoint_id, transfer_name,
                   receiver_name, receiver_device_name, app_version, status,
                   reason, requested_at, responded_at, completed_at
            FROM receiver_requests
            WHERE transfer_id = ?1
            ORDER BY requested_at DESC
            "#,
        )
        .bind(to_db_id(transfer_id)?)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_receiver_request).collect())
    }
}
