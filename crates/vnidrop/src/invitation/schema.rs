use super::*;

const SCHEMA_VERSION: i64 = 13;

impl Repository {
    pub(crate) async fn ensure_schema(&self) -> Result<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let current: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *transaction)
            .await?;
        anyhow::ensure!(
            current <= SCHEMA_VERSION,
            "profile schema version {current} is newer than supported version {SCHEMA_VERSION}"
        );

        // Invitation-transfer tables only. Other domains apply schema from
        // [`crate::persistence::open_all`]. Keep migrations explicit so releases
        // can move invitation history forward in place.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transfers (
                transfer_id INTEGER PRIMARY KEY,
                local_id TEXT NOT NULL,
                protocol_transfer_id INTEGER NOT NULL,
                peer_id TEXT,
                direction TEXT NOT NULL,
                status TEXT NOT NULL,
                transfer_name TEXT,
                sender_name TEXT,
                content_hash TEXT,
                ticket TEXT,
                file_count INTEGER NOT NULL DEFAULT 0,
                total_size INTEGER NOT NULL DEFAULT 0,
                access_mode TEXT NOT NULL DEFAULT 'approval_required',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        let columns = sqlx::query("PRAGMA table_info(transfers)")
            .fetch_all(&mut *transaction)
            .await?;
        let has_access_mode = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "access_mode");
        if !columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "sender_name")
        {
            sqlx::query("ALTER TABLE transfers ADD COLUMN sender_name TEXT")
                .execute(&mut *transaction)
                .await?;
        }
        if !has_access_mode {
            sqlx::query(
                "ALTER TABLE transfers ADD COLUMN access_mode TEXT NOT NULL DEFAULT 'approval_required'",
            )
            .execute(&mut *transaction)
            .await?;
        }

        let has_local_id = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "local_id");
        if !has_local_id {
            sqlx::query("ALTER TABLE transfers ADD COLUMN local_id TEXT")
                .execute(&mut *transaction)
                .await?;
        }
        let has_protocol_transfer_id = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "protocol_transfer_id");
        if !has_protocol_transfer_id {
            sqlx::query("ALTER TABLE transfers ADD COLUMN protocol_transfer_id INTEGER")
                .execute(&mut *transaction)
                .await?;
        }
        let has_peer_id = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "peer_id");
        if !has_peer_id {
            sqlx::query("ALTER TABLE transfers ADD COLUMN peer_id TEXT")
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(
            r#"
            UPDATE transfers
            SET local_id = COALESCE(local_id, 'legacy-' || transfer_id || '-' || direction),
                protocol_transfer_id = COALESCE(protocol_transfer_id, transfer_id)
            WHERE local_id IS NULL OR protocol_transfer_id IS NULL
            "#,
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_transfers_local_id ON transfers(local_id)",
        )
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS invitation_receive_outputs (
                local_id TEXT NOT NULL REFERENCES transfers(local_id) ON DELETE CASCADE,
                locator TEXT NOT NULL,
                file_identity TEXT NOT NULL,
                PRIMARY KEY(local_id, locator)
            );
            CREATE TRIGGER IF NOT EXISTS clear_invitation_outputs AFTER UPDATE ON transfers
            WHEN NEW.status = 'done' OR OLD.content_hash IS NOT NEW.content_hash OR OLD.peer_id IS NOT NEW.peer_id
            BEGIN
                DELETE FROM invitation_receive_outputs WHERE local_id = OLD.local_id;
            END;
            CREATE TABLE IF NOT EXISTS received_artifacts (
                id TEXT PRIMARY KEY,
                transfer_local_id TEXT NOT NULL,
                protocol_transfer_id INTEGER NOT NULL,
                relative_path TEXT NOT NULL,
                locator_kind TEXT NOT NULL,
                locator TEXT NOT NULL,
                logical_size INTEGER NOT NULL,
                published_at INTEGER NOT NULL,
                UNIQUE(transfer_local_id, relative_path)
            );
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transfer_events (
                id TEXT PRIMARY KEY,
                revision INTEGER NOT NULL DEFAULT 0,
                timestamp INTEGER NOT NULL,
                scope TEXT NOT NULL,
                transfer_id INTEGER,
                direction TEXT,
                phase TEXT NOT NULL,
                kind TEXT NOT NULL,
                data_json TEXT NOT NULL
            );
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        let event_columns = sqlx::query("PRAGMA table_info(transfer_events)")
            .fetch_all(&mut *transaction)
            .await?;
        if !event_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "revision")
        {
            sqlx::query(
                "ALTER TABLE transfer_events ADD COLUMN revision INTEGER NOT NULL DEFAULT 0",
            )
            .execute(&mut *transaction)
            .await?;
        }

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transfer_events_transfer_id ON transfer_events(transfer_id, timestamp);",
        )
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS receiver_requests (
                id TEXT PRIMARY KEY,
                transfer_id INTEGER NOT NULL,
                remote_endpoint_id TEXT NOT NULL,
                transfer_name TEXT NOT NULL,
                receiver_name TEXT,
                receiver_device_name TEXT,
                app_version TEXT NOT NULL,
                status TEXT NOT NULL,
                reason TEXT,
                requested_at INTEGER NOT NULL,
                responded_at INTEGER
                ,receipt_token_hash TEXT
                ,completed_at INTEGER
            );
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        let receiver_columns = sqlx::query("PRAGMA table_info(receiver_requests)")
            .fetch_all(&mut *transaction)
            .await?;
        if !receiver_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "receipt_token_hash")
        {
            sqlx::query("ALTER TABLE receiver_requests ADD COLUMN receipt_token_hash TEXT")
                .execute(&mut *transaction)
                .await?;
        }
        if !receiver_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "completed_at")
        {
            sqlx::query("ALTER TABLE receiver_requests ADD COLUMN completed_at INTEGER")
                .execute(&mut *transaction)
                .await?;
        }

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_receiver_requests_transfer_id ON receiver_requests(transfer_id, requested_at DESC);",
        )
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pending_delivery_receipts (
                request_id TEXT PRIMARY KEY,
                local_transfer_id INTEGER NOT NULL,
                sender_blob_ticket TEXT NOT NULL,
                sender_transfer_id INTEGER NOT NULL,
                token TEXT NOT NULL,
                failure_reason TEXT,
                created_at INTEGER NOT NULL
            );
            "#,
        )
        .execute(&mut *transaction)
        .await?;
        let receipt_columns = sqlx::query("PRAGMA table_info(pending_delivery_receipts)")
            .fetch_all(&mut *transaction)
            .await?;
        if !receipt_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "failure_reason")
        {
            sqlx::query("ALTER TABLE pending_delivery_receipts ADD COLUMN failure_reason TEXT")
                .execute(&mut *transaction)
                .await?;
        }

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA user_version = {SCHEMA_VERSION}"
        )))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn schema_version(&self) -> Result<i64> {
        let row = sqlx::query("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get(0))
    }
}
