use super::*;

impl Repository {
    pub(crate) async fn complete_receive_with_pending_receipt(
        &self,
        receipt: PendingDeliveryReceiptInsert<'_>,
    ) -> Result<()> {
        self.maybe_fail_write()?;
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query(
            r#"
            UPDATE transfers
            SET status = 'done', updated_at = ?1
            WHERE transfer_id = ?2 AND direction = 'receive' AND status = 'receiving'
            "#,
        )
        .bind(now_ms())
        .bind(to_db_id(receipt.local_transfer_id)?)
        .execute(&mut *transaction)
        .await?;
        require_one_changed(updated.rows_affected(), "complete receive")?;
        sqlx::query(
            r#"
            INSERT INTO pending_delivery_receipts (
                request_id, local_transfer_id, sender_blob_ticket,
                sender_transfer_id, token, failure_reason, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(request_id) DO UPDATE SET
                local_transfer_id = excluded.local_transfer_id,
                sender_blob_ticket = excluded.sender_blob_ticket,
                sender_transfer_id = excluded.sender_transfer_id,
                token = excluded.token,
                failure_reason = excluded.failure_reason,
                created_at = excluded.created_at
            "#,
        )
        .bind(receipt.request_id)
        .bind(to_db_id(receipt.local_transfer_id)?)
        .bind(receipt.sender_blob_ticket)
        .bind(to_db_id(receipt.sender_transfer_id)?)
        .bind(receipt.token)
        .bind(receipt.failure_reason)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn queue_failed_delivery_receipt(
        &self,
        receipt: PendingDeliveryReceiptInsert<'_>,
    ) -> Result<()> {
        let Some(reason) = receipt.failure_reason else {
            anyhow::bail!("failed delivery receipt requires a reason");
        };
        sqlx::query(
            r#"
            INSERT INTO pending_delivery_receipts (
                request_id, local_transfer_id, sender_blob_ticket,
                sender_transfer_id, token, failure_reason, created_at
            )
            SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7
            WHERE NOT EXISTS (
                SELECT 1 FROM transfers
                WHERE transfer_id = ?2 AND direction = 'receive' AND status = 'done'
            )
            ON CONFLICT(request_id) DO UPDATE SET
                local_transfer_id = excluded.local_transfer_id,
                sender_blob_ticket = excluded.sender_blob_ticket,
                sender_transfer_id = excluded.sender_transfer_id,
                token = excluded.token,
                failure_reason = excluded.failure_reason
            WHERE pending_delivery_receipts.failure_reason IS NOT NULL
            "#,
        )
        .bind(receipt.request_id)
        .bind(to_db_id(receipt.local_transfer_id)?)
        .bind(receipt.sender_blob_ticket)
        .bind(to_db_id(receipt.sender_transfer_id)?)
        .bind(receipt.token)
        .bind(reason)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn list_pending_delivery_receipts(
        &self,
    ) -> Result<Vec<PendingDeliveryReceipt>> {
        let rows = sqlx::query(
            r#"
            SELECT local_transfer_id, sender_blob_ticket, request_id,
                   sender_transfer_id, token, failure_reason, created_at
            FROM pending_delivery_receipts
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PendingDeliveryReceipt {
                local_transfer_id: row.get::<i64, _>("local_transfer_id") as u64,
                sender_blob_ticket: row.get("sender_blob_ticket"),
                request_id: row.get("request_id"),
                sender_transfer_id: row.get::<i64, _>("sender_transfer_id") as u64,
                token: row.get("token"),
                failure_reason: row.get("failure_reason"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub(crate) async fn delete_pending_delivery_receipt(
        &self,
        receipt: &PendingDeliveryReceipt,
    ) -> Result<bool> {
        let deleted = sqlx::query(
            "DELETE FROM pending_delivery_receipts WHERE request_id = ?1 AND created_at = ?2 AND failure_reason IS ?3",
        )
        .bind(&receipt.request_id)
        .bind(receipt.created_at)
        .bind(&receipt.failure_reason)
        .execute(&self.pool)
        .await?;
        Ok(deleted.rows_affected() != 0)
    }
}
