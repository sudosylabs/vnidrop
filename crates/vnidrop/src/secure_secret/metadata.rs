use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SecretMetadataState {
    Staged,
    Active,
    Disabled,
}

impl SecretMetadataState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    fn parse(value: &str) -> Result<Self, VnidropError> {
        match value {
            "staged" => Ok(Self::Staged),
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            _ => Err(VnidropError::SecureStorageCorrupted {
                reason: "protected secret has an unknown metadata state".to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SecretMetadata {
    pub(super) handle: SecretHandle,
    pub(super) kind: SecretKind,
    pub(super) state: SecretMetadataState,
    pub(super) expected_identity: Option<String>,
}

pub(crate) async fn ensure_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS protected_secret_refs (
            handle TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            state TEXT NOT NULL,
            expected_identity TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS protected_secret_one_endpoint_identity
            ON protected_secret_refs(kind)
            WHERE kind = 'endpoint-identity' AND state != 'disabled'
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct SecretMetadataStore {
    pool: SqlitePool,
}

impl SecretMetadataStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(super) async fn stage(
        &self,
        handle: &SecretHandle,
        kind: SecretKind,
        expected_identity: Option<&str>,
    ) -> Result<(), VnidropError> {
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO protected_secret_refs
                (handle, kind, state, expected_identity, created_at, updated_at)
            VALUES (?1, ?2, 'staged', ?3, ?4, ?4)
            "#,
        )
        .bind(handle.as_str())
        .bind(kind.as_str())
        .bind(expected_identity)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn activate(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        self.set_state(handle, SecretMetadataState::Active).await
    }

    pub(super) async fn disable(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        self.set_state(handle, SecretMetadataState::Disabled).await
    }

    pub(super) async fn purge_disabled(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        sqlx::query(
            "DELETE FROM protected_secret_refs WHERE handle = ?1 AND state = 'disabled' AND kind != 'endpoint-identity'",
        )
        .bind(handle.as_str())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    async fn set_state(
        &self,
        handle: &SecretHandle,
        state: SecretMetadataState,
    ) -> Result<(), VnidropError> {
        sqlx::query(
            "UPDATE protected_secret_refs SET state = ?2, updated_at = ?3 WHERE handle = ?1",
        )
        .bind(handle.as_str())
        .bind(state.as_str())
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn find(
        &self,
        handle: &SecretHandle,
    ) -> Result<Option<SecretMetadata>, VnidropError> {
        let row = sqlx::query(
            "SELECT handle, kind, state, expected_identity FROM protected_secret_refs WHERE handle = ?1",
        )
        .bind(handle.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(row_to_metadata).transpose()
    }

    pub(super) async fn list(&self) -> Result<Vec<SecretMetadata>, VnidropError> {
        let rows = sqlx::query(
            "SELECT handle, kind, state, expected_identity FROM protected_secret_refs ORDER BY handle",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_metadata).collect()
    }

    pub(super) async fn find_active_kind(
        &self,
        kind: SecretKind,
    ) -> Result<Option<SecretMetadata>, VnidropError> {
        let row = sqlx::query(
            r#"
            SELECT handle, kind, state, expected_identity
            FROM protected_secret_refs
            WHERE kind = ?1 AND state = 'active'
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .bind(kind.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(row_to_metadata).transpose()
    }

    pub(super) async fn contains_kind(&self, kind: SecretKind) -> Result<bool, VnidropError> {
        let row = sqlx::query("SELECT 1 FROM protected_secret_refs WHERE kind = ?1 LIMIT 1")
            .bind(kind.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(VnidropError::repository)?;
        Ok(row.is_some())
    }
}

fn row_to_metadata(row: sqlx::sqlite::SqliteRow) -> Result<SecretMetadata, VnidropError> {
    Ok(SecretMetadata {
        handle: SecretHandle(row.get(0)),
        kind: SecretKind::parse(row.get::<String, _>(1).as_str())?,
        state: SecretMetadataState::parse(row.get::<String, _>(2).as_str())?,
        expected_identity: row.get(3),
    })
}
