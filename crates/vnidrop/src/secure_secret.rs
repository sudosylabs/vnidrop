use std::{collections::HashSet, fmt, io, path::Path, sync::Arc, time::Duration};

#[cfg(test)]
use std::{collections::HashMap, sync::Mutex};

use data_encoding::HEXLOWER;
use iroh::SecretKey;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{error::VnidropError, util::now_ms};

#[cfg(any(test, target_os = "android"))]
pub(crate) mod android;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) mod apple;
#[cfg(any(test, target_os = "linux"))]
pub(crate) mod linux;
mod platform;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(test)]
pub(crate) use platform::scope_store;
pub(crate) use platform::{lock_profile, platform_secret_store, ProfileLock};

const SECRET_BYTES: usize = 32;
const HANDLE_NAMESPACE: &str = "vnidrop";
const HANDLE_VERSION: &str = "v1";

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SecretMaterial(Vec<u8>);

impl SecretMaterial {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self, VnidropError> {
        if bytes.len() != SECRET_BYTES || bytes.iter().all(|byte| *byte == 0) {
            return Err(VnidropError::SecureStorageCorrupted {
                reason: "protected secret has invalid key material".to_string(),
            });
        }
        Ok(Self(bytes))
    }

    fn endpoint_id(&self) -> String {
        let bytes: [u8; SECRET_BYTES] = self.0.as_slice().try_into().expect("validated length");
        SecretKey::from_bytes(&bytes).public().to_string()
    }

    fn into_secret_key(self) -> SecretKey {
        let bytes: [u8; SECRET_BYTES] = self.0.try_into().expect("validated length");
        SecretKey::from_bytes(&bytes)
    }
}

impl fmt::Debug for SecretMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretMaterial(redacted)")
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct SecretHandle(String);

impl SecretHandle {
    fn generate(kind: SecretKind) -> Self {
        Self(format!(
            "{HANDLE_NAMESPACE}/{HANDLE_VERSION}/{}/{}",
            kind.as_str(),
            Uuid::new_v4()
        ))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SecretHandle")
            .field(&self.0)
            .finish()
    }
}

#[cfg(test)]
pub(crate) fn secret_handle_for_test(value: String) -> SecretHandle {
    SecretHandle(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretKind {
    EndpointIdentity,
    RelationshipGrant,
    PairingEligibility,
}

impl SecretKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::EndpointIdentity => "endpoint-identity",
            Self::RelationshipGrant => "relationship-grant",
            Self::PairingEligibility => "pairing-eligibility",
        }
    }

    fn parse(value: &str) -> Result<Self, VnidropError> {
        match value {
            "endpoint-identity" => Ok(Self::EndpointIdentity),
            "relationship-grant" => Ok(Self::RelationshipGrant),
            "pairing-eligibility" => Ok(Self::PairingEligibility),
            _ => Err(VnidropError::SecureStorageCorrupted {
                reason: "protected secret has an unknown kind".to_string(),
            }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SecureSecretStoreError {
    #[error("credential store is locked")]
    Locked,
    #[error("credential is missing")]
    Missing,
    #[error("credential is corrupted")]
    Corrupted,
    #[error("credential store is unavailable")]
    Unavailable,
}

/// Opaque credential-store boundary implemented by each supported platform.
///
/// Implementations must persist material outside ordinary application storage and
/// must never include material in errors or diagnostics.
pub(crate) trait SecureSecretStore: Send + Sync {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError>;
    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError>;
    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError>;
    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretMetadataState {
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
struct SecretMetadata {
    handle: SecretHandle,
    kind: SecretKind,
    state: SecretMetadataState,
    expected_identity: Option<String>,
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

    async fn stage(
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

    async fn activate(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        self.set_state(handle, SecretMetadataState::Active).await
    }

    async fn disable(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        self.set_state(handle, SecretMetadataState::Disabled).await
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

    async fn find(&self, handle: &SecretHandle) -> Result<Option<SecretMetadata>, VnidropError> {
        let row = sqlx::query(
            "SELECT handle, kind, state, expected_identity FROM protected_secret_refs WHERE handle = ?1",
        )
        .bind(handle.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        row.map(row_to_metadata).transpose()
    }

    async fn list(&self) -> Result<Vec<SecretMetadata>, VnidropError> {
        let rows = sqlx::query(
            "SELECT handle, kind, state, expected_identity FROM protected_secret_refs ORDER BY handle",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(VnidropError::repository)?;
        rows.into_iter().map(row_to_metadata).collect()
    }

    async fn find_active_kind(
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

    async fn contains_kind(&self, kind: SecretKind) -> Result<bool, VnidropError> {
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

pub(crate) struct SecretCustody {
    metadata: SecretMetadataStore,
    store: Arc<dyn SecureSecretStore>,
    #[cfg(test)]
    crash_point: Mutex<Option<CustodyCrashPoint>>,
}

pub(crate) async fn start_endpoint_identity(
    metadata: SecretMetadataStore,
    store: Arc<dyn SecureSecretStore>,
    legacy_path: &Path,
) -> Result<(SecretKey, SecretCustody), VnidropError> {
    let (custody, _) = SecretCustody::start(metadata, store).await?;
    let secret_key = custody
        .initialize_endpoint_identity(legacy_path)
        .await?
        .into_secret_key();
    Ok((secret_key, custody))
}

impl SecretCustody {
    pub(crate) async fn start(
        metadata: SecretMetadataStore,
        store: Arc<dyn SecureSecretStore>,
    ) -> Result<(Self, ReconciliationSummary), VnidropError> {
        let custody = Self::from_parts(metadata, store);
        let summary = custody.reconcile().await?;
        Ok((custody, summary))
    }

    #[cfg(test)]
    pub(crate) fn new(metadata: SecretMetadataStore, store: Arc<dyn SecureSecretStore>) -> Self {
        Self::from_parts(metadata, store)
    }

    fn from_parts(metadata: SecretMetadataStore, store: Arc<dyn SecureSecretStore>) -> Self {
        Self {
            metadata,
            store,
            #[cfg(test)]
            crash_point: Mutex::new(None),
        }
    }

    pub(crate) async fn protect(
        &self,
        kind: SecretKind,
        material: SecretMaterial,
        expected_identity: Option<&str>,
    ) -> Result<SecretHandle, VnidropError> {
        validate_material(kind, &material, expected_identity)?;
        let handle = SecretHandle::generate(kind);
        self.store
            .put(&handle, material.clone())
            .map_err(map_store_error)?;
        #[cfg(test)]
        self.maybe_crash(CustodyCrashPoint::StoreWrite)?;
        let stored = self.store.get(&handle).map_err(map_store_error)?;
        if stored != material {
            return Err(VnidropError::SecureStorageCorrupted {
                reason: "credential store did not preserve protected material".to_string(),
            });
        }
        validate_material(kind, &stored, expected_identity)?;
        if let Err(error) = self.metadata.stage(&handle, kind, expected_identity).await {
            self.delete_if_present(&handle)?;
            return Err(error);
        }
        #[cfg(test)]
        self.maybe_crash(CustodyCrashPoint::MetadataStage)?;
        self.metadata.activate(&handle).await?;
        #[cfg(test)]
        self.maybe_crash(CustodyCrashPoint::MetadataActivation)?;
        Ok(handle)
    }

    pub(crate) async fn load(&self, handle: &SecretHandle) -> Result<SecretMaterial, VnidropError> {
        let metadata = self.metadata.find(handle).await?.ok_or_else(|| {
            VnidropError::SecureStorageMissing {
                reason: "protected secret metadata is missing".to_string(),
            }
        })?;
        if metadata.state != SecretMetadataState::Active {
            return Err(VnidropError::SecureStorageUnavailable {
                reason: "protected secret is not active".to_string(),
            });
        }
        let material = self.store.get(handle).map_err(map_store_error)?;
        validate_material(
            metadata.kind,
            &material,
            metadata.expected_identity.as_deref(),
        )?;
        Ok(material)
    }

    pub(crate) async fn migrate_legacy_endpoint_identity(
        &self,
        legacy_path: &Path,
    ) -> Result<SecretHandle, VnidropError> {
        if let Some(active) = self
            .metadata
            .find_active_kind(SecretKind::EndpointIdentity)
            .await?
        {
            let protected = self.load(&active.handle).await?;
            match read_legacy_endpoint_identity(legacy_path).await {
                Ok(legacy) => {
                    if legacy.endpoint_id() != protected.endpoint_id() {
                        return Err(VnidropError::SecureStorageCorrupted {
                            reason: "legacy endpoint key does not match protected identity"
                                .to_string(),
                        });
                    }
                    tokio::fs::remove_file(legacy_path)
                        .await
                        .map_err(VnidropError::filesystem)?;
                }
                Err(VnidropError::SecureStorageMissing { .. }) => {}
                Err(error) => return Err(error),
            }
            return Ok(active.handle);
        }

        let legacy = read_legacy_endpoint_identity(legacy_path).await?;
        let endpoint_id = legacy.endpoint_id();
        let handle = self
            .protect(
                SecretKind::EndpointIdentity,
                legacy,
                Some(endpoint_id.as_str()),
            )
            .await?;
        tokio::fs::remove_file(legacy_path)
            .await
            .map_err(VnidropError::filesystem)?;
        Ok(handle)
    }

    pub(crate) async fn initialize_endpoint_identity(
        &self,
        legacy_path: &Path,
    ) -> Result<SecretMaterial, VnidropError> {
        if self
            .metadata
            .find_active_kind(SecretKind::EndpointIdentity)
            .await?
            .is_some()
        {
            let handle = self.migrate_legacy_endpoint_identity(legacy_path).await?;
            return self.load(&handle).await;
        }
        if self
            .metadata
            .contains_kind(SecretKind::EndpointIdentity)
            .await?
        {
            return Err(VnidropError::SecureStorageUnavailable {
                reason: "protected endpoint identity is disabled".to_string(),
            });
        }
        match tokio::fs::try_exists(legacy_path).await {
            Ok(true) => {
                let handle = self.migrate_legacy_endpoint_identity(legacy_path).await?;
                self.load(&handle).await
            }
            Ok(false) => {
                let secret = SecretKey::generate();
                let material = SecretMaterial::new(secret.to_bytes().to_vec())?;
                let endpoint_id = material.endpoint_id();
                match self
                    .protect(
                        SecretKind::EndpointIdentity,
                        material,
                        Some(endpoint_id.as_str()),
                    )
                    .await
                {
                    Ok(handle) => self.load(&handle).await,
                    Err(error) => {
                        let winner = tokio::time::timeout(Duration::from_secs(1), async {
                            loop {
                                if let Some(active) = self
                                    .metadata
                                    .find_active_kind(SecretKind::EndpointIdentity)
                                    .await?
                                {
                                    return self.load(&active.handle).await;
                                }
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await;
                        match winner {
                            Ok(result) => result,
                            Err(_) => Err(error),
                        }
                    }
                }
            }
            Err(error) => Err(VnidropError::filesystem(error)),
        }
    }

    pub(crate) async fn reconcile(&self) -> Result<ReconciliationSummary, VnidropError> {
        let metadata = self.metadata.list().await?;
        let stored_handles = self.store.list_handles().map_err(map_store_error)?;
        let known_handles = metadata
            .iter()
            .map(|entry| entry.handle.clone())
            .collect::<HashSet<_>>();
        let mut summary = ReconciliationSummary::default();

        for entry in metadata {
            if entry.state == SecretMetadataState::Disabled {
                self.delete_if_present(&entry.handle)?;
                continue;
            }
            match self.store.get(&entry.handle) {
                Ok(material) => {
                    if validate_material(entry.kind, &material, entry.expected_identity.as_deref())
                        .is_err()
                    {
                        self.metadata.disable(&entry.handle).await?;
                        self.delete_if_present(&entry.handle)?;
                        summary.disabled += 1;
                    } else if entry.state == SecretMetadataState::Staged {
                        self.metadata.activate(&entry.handle).await?;
                        summary.staged_activated += 1;
                    }
                }
                Err(SecureSecretStoreError::Missing | SecureSecretStoreError::Corrupted) => {
                    self.metadata.disable(&entry.handle).await?;
                    self.delete_if_present(&entry.handle)?;
                    summary.disabled += 1;
                }
                Err(error) => return Err(map_store_error(error)),
            }
        }

        for handle in stored_handles {
            if !known_handles.contains(&handle) {
                self.store.delete(&handle).map_err(map_store_error)?;
                summary.orphans_deleted += 1;
            }
        }
        Ok(summary)
    }

    fn delete_if_present(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        match self.store.delete(handle) {
            Ok(()) | Err(SecureSecretStoreError::Missing) => Ok(()),
            Err(error) => Err(map_store_error(error)),
        }
    }

    #[cfg(test)]
    pub(crate) fn crash_once_at(&self, point: CustodyCrashPoint) {
        *self.crash_point.lock().unwrap() = Some(point);
    }

    #[cfg(test)]
    fn maybe_crash(&self, point: CustodyCrashPoint) -> Result<(), VnidropError> {
        let mut crash_point = self.crash_point.lock().unwrap();
        if *crash_point == Some(point) {
            *crash_point = None;
            return Err(VnidropError::Internal {
                reason: format!("simulated custody crash at {point:?}"),
            });
        }
        Ok(())
    }
}

async fn read_legacy_endpoint_identity(path: &Path) -> Result<SecretMaterial, VnidropError> {
    let encoded = match tokio::fs::read_to_string(path).await {
        Ok(encoded) => encoded,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(VnidropError::SecureStorageMissing {
                reason: "no protected or legacy endpoint identity exists".to_string(),
            });
        }
        Err(error) => return Err(VnidropError::filesystem(error)),
    };
    let bytes = HEXLOWER.decode(encoded.trim().as_bytes()).map_err(|_| {
        VnidropError::SecureStorageCorrupted {
            reason: "legacy endpoint key encoding is invalid".to_string(),
        }
    })?;
    SecretMaterial::new(bytes)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ReconciliationSummary {
    pub(crate) orphans_deleted: u64,
    pub(crate) staged_activated: u64,
    pub(crate) disabled: u64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CustodyCrashPoint {
    StoreWrite,
    MetadataStage,
    MetadataActivation,
}

fn validate_material(
    kind: SecretKind,
    material: &SecretMaterial,
    expected_identity: Option<&str>,
) -> Result<(), VnidropError> {
    if kind == SecretKind::EndpointIdentity {
        let expected_identity =
            expected_identity.ok_or_else(|| VnidropError::SecureStorageCorrupted {
                reason: "endpoint identity metadata lacks its expected endpoint id".to_string(),
            })?;
        if material.endpoint_id() != expected_identity {
            return Err(VnidropError::SecureStorageCorrupted {
                reason: "protected endpoint identity does not match its endpoint id".to_string(),
            });
        }
    }
    Ok(())
}

fn map_store_error(error: SecureSecretStoreError) -> VnidropError {
    let reason = error.to_string();
    match error {
        SecureSecretStoreError::Locked => VnidropError::SecureStorageLocked { reason },
        SecureSecretStoreError::Missing => VnidropError::SecureStorageMissing { reason },
        SecureSecretStoreError::Corrupted => VnidropError::SecureStorageCorrupted { reason },
        SecureSecretStoreError::Unavailable => VnidropError::SecureStorageUnavailable { reason },
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceStoreFailure {
    Locked,
    Unavailable,
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct FaultInjectingSecretStore {
    values: Mutex<HashMap<SecretHandle, SecretMaterial>>,
    failure: Mutex<Option<ReferenceStoreFailure>>,
    corrupted: Mutex<Vec<SecretHandle>>,
}

#[cfg(test)]
impl FaultInjectingSecretStore {
    pub(crate) fn fail_with(&self, failure: Option<ReferenceStoreFailure>) {
        *self.failure.lock().unwrap() = failure;
    }

    pub(crate) fn remove_for_test(&self, handle: &SecretHandle) {
        self.values.lock().unwrap().remove(handle);
    }

    pub(crate) fn corrupt_for_test(&self, handle: &SecretHandle) {
        self.corrupted.lock().unwrap().push(handle.clone());
    }

    pub(crate) fn only_handle_for_test(&self) -> SecretHandle {
        let handles = self
            .values
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(handles.len(), 1, "expected exactly one protected secret");
        handles.into_iter().next().unwrap()
    }

    fn check_available(&self) -> Result<(), SecureSecretStoreError> {
        match *self.failure.lock().unwrap() {
            Some(ReferenceStoreFailure::Locked) => Err(SecureSecretStoreError::Locked),
            Some(ReferenceStoreFailure::Unavailable) => Err(SecureSecretStoreError::Unavailable),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
impl SecureSecretStore for FaultInjectingSecretStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        self.check_available()?;
        self.values.lock().unwrap().insert(handle.clone(), material);
        Ok(())
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        self.check_available()?;
        if self.corrupted.lock().unwrap().contains(handle) {
            return Err(SecureSecretStoreError::Corrupted);
        }
        self.values
            .lock()
            .unwrap()
            .get(handle)
            .cloned()
            .ok_or(SecureSecretStoreError::Missing)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        self.check_available()?;
        self.values.lock().unwrap().remove(handle);
        Ok(())
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        self.check_available()?;
        Ok(self.values.lock().unwrap().keys().cloned().collect())
    }
}
