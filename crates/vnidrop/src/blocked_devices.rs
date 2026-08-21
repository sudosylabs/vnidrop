//! Identity-wide deny list for saved-device and invitation traffic.

use anyhow::Result;
use sqlx::{Row, SqlitePool};

pub(crate) async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blocked_endpoints (
            endpoint_id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Durable deny records for one app-data profile.
#[derive(Debug, Clone)]
pub(crate) struct BlockStore {
    pool: SqlitePool,
}

impl BlockStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn block_endpoint(&self, endpoint_id: &str, now_ms: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO blocked_endpoints (endpoint_id, created_at) VALUES (?1, ?2)
             ON CONFLICT(endpoint_id) DO NOTHING",
        )
        .bind(endpoint_id)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn unblock_endpoint(&self, endpoint_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM blocked_endpoints WHERE endpoint_id = ?1")
            .bind(endpoint_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn is_blocked(&self, endpoint_id: &str) -> Result<bool> {
        let row =
            sqlx::query("SELECT EXISTS(SELECT 1 FROM blocked_endpoints WHERE endpoint_id = ?1)")
                .bind(endpoint_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.get::<i64, _>(0) == 1)
    }

    pub(crate) async fn list_blocked(&self) -> Result<Vec<String>> {
        let rows =
            sqlx::query("SELECT endpoint_id FROM blocked_endpoints ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }
}
