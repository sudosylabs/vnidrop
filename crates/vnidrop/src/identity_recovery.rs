//! Explicit recovery for an unrecoverable protected endpoint identity.

use sqlx::{Row, SqlitePool};

use crate::{error::VnidropError, util::now_ms};

/// Owns the cross-domain transaction that invalidates trust bound to an old identity.
#[derive(Clone)]
pub(crate) struct IdentityRecoveryStore {
    pool: SqlitePool,
}

impl IdentityRecoveryStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Preserve completed history and received artifacts, but revoke every
    /// capability that could authenticate or resume work as the lost identity.
    pub(crate) async fn reset_identity_bound_state(&self) -> Result<Vec<String>, VnidropError> {
        let now = now_ms();
        let mut transaction = self.pool.begin().await.map_err(VnidropError::repository)?;
        let handles = sqlx::query("SELECT handle FROM protected_secret_refs ORDER BY handle")
            .fetch_all(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect::<Vec<_>>();

        sqlx::query(
            r#"
            UPDATE transfers
            SET status = CASE
                    WHEN status = 'sharing' THEN 'stopped'
                    WHEN status IN ('importing', 'receiving') THEN 'cancelled'
                    ELSE status
                END,
                ticket = CASE
                    WHEN status IN ('sharing', 'importing', 'receiving') THEN NULL
                    ELSE ticket
                END,
                updated_at = CASE
                    WHEN status IN ('sharing', 'importing', 'receiving') THEN ?1
                    ELSE updated_at
                END
            WHERE status IN ('sharing', 'importing', 'receiving')
            "#,
        )
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        sqlx::query(
            r#"
            UPDATE receiver_requests
            SET status = CASE WHEN status = 'requested' THEN 'expired' ELSE 'failed' END,
                reason = 'device identity reset',
                responded_at = COALESCE(responded_at, ?1)
            WHERE status IN ('requested', 'accepted')
            "#,
        )
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;
        sqlx::query("DELETE FROM pending_delivery_receipts")
            .execute(&mut *transaction)
            .await
            .map_err(VnidropError::repository)?;

        for table in [
            "targeted_accepted_offer_intents",
            "targeted_authorization_delivery_outbox",
            "targeted_completion_outbox",
            "targeted_payload_release_outbox",
        ] {
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&mut *transaction)
                .await
                .map_err(VnidropError::repository)?;
        }
        sqlx::query(
            r#"
            UPDATE targeted_transfers
            SET state = CASE
                    WHEN state IN (
                        'preparing', 'offering', 'awaiting_approval', 'approved',
                        'connecting', 'transferring', 'interrupted'
                    ) THEN 'cancelled'
                    ELSE state
                END,
                blob_ticket = NULL,
                authorization_secret_handle = NULL,
                updated_at = CASE
                    WHEN state IN (
                        'preparing', 'offering', 'awaiting_approval', 'approved',
                        'connecting', 'transferring', 'interrupted'
                    ) OR blob_ticket IS NOT NULL OR authorization_secret_handle IS NOT NULL
                    THEN ?1
                    ELSE updated_at
                END
            "#,
        )
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(VnidropError::repository)?;

        for table in [
            "pairing_eligibilities",
            "device_relationships",
            "relationship_generation_tombstones",
            "protected_secret_refs",
        ] {
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&mut *transaction)
                .await
                .map_err(VnidropError::repository)?;
        }
        transaction
            .commit()
            .await
            .map_err(VnidropError::repository)?;
        Ok(handles)
    }
}
