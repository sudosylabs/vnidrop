use super::*;

const SCHEMA_VERSION: i64 = 13;

impl Repository {
    pub(crate) async fn ensure_schema(&self) -> Result<()> {
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
        .execute(&self.pool)
        .await?;

        let columns = sqlx::query("PRAGMA table_info(transfers)")
            .fetch_all(&self.pool)
            .await?;
        let has_access_mode = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "access_mode");
        if !columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "sender_name")
        {
            sqlx::query("ALTER TABLE transfers ADD COLUMN sender_name TEXT")
                .execute(&self.pool)
                .await?;
        }
        if !has_access_mode {
            sqlx::query(
                "ALTER TABLE transfers ADD COLUMN access_mode TEXT NOT NULL DEFAULT 'approval_required'",
            )
            .execute(&self.pool)
            .await?;
        }

        let has_local_id = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "local_id");
        if !has_local_id {
            sqlx::query("ALTER TABLE transfers ADD COLUMN local_id TEXT")
                .execute(&self.pool)
                .await?;
        }
        let has_protocol_transfer_id = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "protocol_transfer_id");
        if !has_protocol_transfer_id {
            sqlx::query("ALTER TABLE transfers ADD COLUMN protocol_transfer_id INTEGER")
                .execute(&self.pool)
                .await?;
        }
        let has_peer_id = columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "peer_id");
        if !has_peer_id {
            sqlx::query("ALTER TABLE transfers ADD COLUMN peer_id TEXT")
                .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_transfers_local_id ON transfers(local_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
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
        .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;

        let event_columns = sqlx::query("PRAGMA table_info(transfer_events)")
            .fetch_all(&self.pool)
            .await?;
        if !event_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "revision")
        {
            sqlx::query(
                "ALTER TABLE transfer_events ADD COLUMN revision INTEGER NOT NULL DEFAULT 0",
            )
            .execute(&self.pool)
            .await?;
        }

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transfer_events_transfer_id ON transfer_events(transfer_id, timestamp);",
        )
        .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;

        let receiver_columns = sqlx::query("PRAGMA table_info(receiver_requests)")
            .fetch_all(&self.pool)
            .await?;
        if !receiver_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "receipt_token_hash")
        {
            sqlx::query("ALTER TABLE receiver_requests ADD COLUMN receipt_token_hash TEXT")
                .execute(&self.pool)
                .await?;
        }
        if !receiver_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "completed_at")
        {
            sqlx::query("ALTER TABLE receiver_requests ADD COLUMN completed_at INTEGER")
                .execute(&self.pool)
                .await?;
        }

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_receiver_requests_transfer_id ON receiver_requests(transfer_id, requested_at DESC);",
        )
        .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;
        let receipt_columns = sqlx::query("PRAGMA table_info(pending_delivery_receipts)")
            .fetch_all(&self.pool)
            .await?;
        if !receipt_columns
            .iter()
            .any(|row| row.get::<String, _>(1) == "failure_reason")
        {
            sqlx::query("ALTER TABLE pending_delivery_receipts ADD COLUMN failure_reason TEXT")
                .execute(&self.pool)
                .await?;
        }

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA user_version = {SCHEMA_VERSION}"
        )))
        .execute(&self.pool)
        .await?;
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
