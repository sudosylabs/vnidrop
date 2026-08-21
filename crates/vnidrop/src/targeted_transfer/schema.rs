//! Targeted-transfer SQLite schema and one-time migrations.

use sqlx::{Row, SqlitePool};

use crate::util::now_ms;

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
            verified_bytes INTEGER NOT NULL DEFAULT 0,
            blob_ticket TEXT,
            authorization_secret_handle TEXT,
            role TEXT NOT NULL DEFAULT 'sender',
            state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targeted_accepted_offer_intents (
            transfer_id TEXT PRIMARY KEY,
            sender_endpoint_id TEXT NOT NULL,
            receiver_endpoint_id TEXT NOT NULL,
            manifest_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            transfer_name TEXT NOT NULL,
            file_count INTEGER NOT NULL,
            total_size INTEGER NOT NULL,
            protocol_version INTEGER NOT NULL,
            accepted_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targeted_authorization_delivery_outbox (
            transfer_id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            next_attempt_at INTEGER NOT NULL,
            FOREIGN KEY(transfer_id) REFERENCES targeted_transfers(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targeted_completion_outbox (
            transfer_id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            next_attempt_at INTEGER NOT NULL,
            FOREIGN KEY(transfer_id) REFERENCES targeted_transfers(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await?;
    let completion_columns = sqlx::query("PRAGMA table_info(targeted_completion_outbox)")
        .fetch_all(pool)
        .await?;
    if !completion_columns
        .iter()
        .any(|row| row.get::<String, _>(1) == "next_attempt_at")
    {
        sqlx::query(
            "ALTER TABLE targeted_completion_outbox ADD COLUMN next_attempt_at INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targeted_payload_release_outbox (
            transfer_id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(transfer_id) REFERENCES targeted_transfers(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await?;
    let columns = sqlx::query("PRAGMA table_info(targeted_transfers)")
        .fetch_all(pool)
        .await?;
    let has = |name: &str| columns.iter().any(|row| row.get::<String, _>(1) == name);
    if !has("verified_bytes") {
        sqlx::query(
            "ALTER TABLE targeted_transfers ADD COLUMN verified_bytes INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    if !has("blob_ticket") {
        sqlx::query("ALTER TABLE targeted_transfers ADD COLUMN blob_ticket TEXT")
            .execute(pool)
            .await?;
    }
    if !has("authorization_secret_handle") {
        sqlx::query("ALTER TABLE targeted_transfers ADD COLUMN authorization_secret_handle TEXT")
            .execute(pool)
            .await?;
    }
    if !has("role") {
        sqlx::query(
            "ALTER TABLE targeted_transfers ADD COLUMN role TEXT NOT NULL DEFAULT 'sender'",
        )
        .execute(pool)
        .await?;
    }
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targeted_schema_migrations (
            name TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;
    let mut transaction = pool.begin().await?;
    let applied = sqlx::query(
        "SELECT EXISTS(SELECT 1 FROM targeted_schema_migrations WHERE name = 'authorization-delivery-outbox-v1')",
    )
    .fetch_one(&mut *transaction)
    .await?
    .get::<i64, _>(0)
        != 0;
    if !applied {
        // Pre-outbox Approved sender rows already hold a receiver-bound authorization.
        // Re-delivery is idempotent and cannot create consent on a receiver.
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO targeted_authorization_delivery_outbox
                (transfer_id, created_at, next_attempt_at)
            SELECT id, updated_at, 0 FROM targeted_transfers
            WHERE role = 'sender' AND state = 'approved'
              AND blob_ticket IS NOT NULL AND authorization_secret_handle IS NOT NULL
            "#,
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO targeted_schema_migrations (name, applied_at) VALUES ('authorization-delivery-outbox-v1', ?1)",
        )
        .bind(now_ms())
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}
