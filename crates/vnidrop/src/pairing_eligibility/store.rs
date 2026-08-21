//! Durable pairing-eligibility rows (schema + queries).

use sqlx::{Row, SqlitePool};

use crate::{api::PairingEligibilitySummary, error::VnidropError};

use super::{PairingEligibilityInsert, PairingEligibilityRecord};

/// Domain store for `pairing_eligibilities`.
#[derive(Clone)]
pub(crate) struct PairingEligibilityStore {
    pool: SqlitePool,
}

impl PairingEligibilityStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn ensure_schema(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pairing_eligibilities (
                session_id TEXT PRIMARY KEY,
                peer_endpoint_id TEXT NOT NULL,
                remote_display_name TEXT,
                protocol_version INTEGER NOT NULL,
                secret_handle TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );
            "#,
        )
        .execute(pool)
        .await?;
        let columns = sqlx::query("PRAGMA table_info(pairing_eligibilities)")
            .fetch_all(pool)
            .await?;
        if !columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "remote_display_name")
        {
            sqlx::query("ALTER TABLE pairing_eligibilities ADD COLUMN remote_display_name TEXT")
                .execute(pool)
                .await?;
        }
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS pairing_eligibilities_peer
                ON pairing_eligibilities(peer_endpoint_id);
            "#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn insert(
        &self,
        entry: PairingEligibilityInsert<'_>,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            r#"
            INSERT INTO pairing_eligibilities (
                session_id, peer_endpoint_id, remote_display_name, protocol_version,
                secret_handle, created_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(entry.session_id)
        .bind(entry.peer_endpoint_id)
        .bind(entry.remote_display_name)
        .bind(i64::from(entry.protocol_version))
        .bind(entry.secret_handle)
        .bind(entry.created_at)
        .bind(entry.expires_at)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(crate) async fn list_summaries(
        &self,
    ) -> Result<Vec<PairingEligibilitySummary>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT peer_endpoint_id, remote_display_name, session_id, protocol_version,
                   created_at, expires_at
            FROM pairing_eligibilities
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows
            .into_iter()
            .map(|row| PairingEligibilitySummary {
                peer_endpoint_id: row.get("peer_endpoint_id"),
                remote_display_name: row.get("remote_display_name"),
                session_id: row.get("session_id"),
                protocol_version: row.get::<i64, _>("protocol_version") as u16,
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            })
            .collect())
    }

    pub(crate) async fn list_records(&self) -> Result<Vec<PairingEligibilityRecord>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT peer_endpoint_id, remote_display_name, session_id, protocol_version,
                   secret_handle, created_at, expires_at
            FROM pairing_eligibilities
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }

    pub(crate) async fn list_for_peer(
        &self,
        peer_endpoint_id: &str,
    ) -> Result<Vec<PairingEligibilityRecord>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT peer_endpoint_id, remote_display_name, session_id, protocol_version,
                   secret_handle, created_at, expires_at
            FROM pairing_eligibilities
            WHERE peer_endpoint_id = ?1
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }

    pub(crate) async fn list_expired(
        &self,
        now_ms: i64,
    ) -> Result<Vec<PairingEligibilityRecord>, VnidropError> {
        let rows = sqlx::query(
            r#"
            SELECT peer_endpoint_id, remote_display_name, session_id, protocol_version,
                   secret_handle, created_at, expires_at
            FROM pairing_eligibilities
            WHERE expires_at <= ?1
            "#,
        )
        .bind(now_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }

    pub(crate) async fn find_by_session(
        &self,
        session_id: &str,
    ) -> Result<Option<PairingEligibilityRecord>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT peer_endpoint_id, remote_display_name, session_id, protocol_version,
                   secret_handle, created_at, expires_at
            FROM pairing_eligibilities
            WHERE session_id = ?1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(row.map(row_to_record))
    }

    pub(crate) async fn delete(&self, session_id: &str) -> Result<(), VnidropError> {
        sqlx::query("DELETE FROM pairing_eligibilities WHERE session_id = ?1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn force_expiry_for_test(
        &self,
        session_id: &str,
        expires_at: i64,
    ) -> Result<(), VnidropError> {
        sqlx::query("UPDATE pairing_eligibilities SET expires_at = ?2 WHERE session_id = ?1")
            .bind(session_id)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }
}

fn row_to_record(row: sqlx::sqlite::SqliteRow) -> PairingEligibilityRecord {
    PairingEligibilityRecord {
        peer_endpoint_id: row.get("peer_endpoint_id"),
        remote_display_name: row.get("remote_display_name"),
        session_id: row.get("session_id"),
        protocol_version: row.get::<i64, _>("protocol_version") as u16,
        secret_handle: row.get("secret_handle"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    }
}
