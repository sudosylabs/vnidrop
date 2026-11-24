use std::{collections::HashSet, fmt, path::Path, sync::Arc, time::Duration};

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
use std::{collections::HashMap, sync::Mutex};

use iroh::SecretKey;
use sqlx::{Row, SqlitePool};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::ZeroizeOnDrop;

use crate::{error::VnidropError, util::now_ms};

#[cfg(any(all(test, unix), target_os = "android"))]
pub(crate) mod android;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) mod apple;
mod legacy_identity;
#[cfg(any(test, target_os = "linux"))]
pub(crate) mod linux;
mod metadata;
mod platform;

use legacy_identity::{read_legacy_endpoint_identity, remove_legacy_identity_durably};
use metadata::SecretMetadataState;
pub(crate) use metadata::{ensure_schema, SecretMetadataStore};
#[cfg(any(test, target_os = "windows"))]
pub(crate) mod windows;

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
pub(crate) use platform::install_platform_secret_store_for_test;
#[cfg(test)]
pub(crate) use platform::scope_store;
#[cfg(test)]
pub(crate) use platform::unlocked_profile_for_test;
pub(crate) use platform::{lock_profile, platform_secret_store, ProfileLock};

const SECRET_BYTES: usize = 32;
const HANDLE_NAMESPACE: &str = "vnidrop";
const HANDLE_VERSION: &str = "v1";

#[derive(Clone, ZeroizeOnDrop)]
pub(crate) struct SecretMaterial(Vec<u8>);

impl SecretMaterial {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self, VnidropError> {
        let material = Self(bytes);
        if material.0.len() != SECRET_BYTES || material.0.iter().all(|byte| *byte == 0) {
            return Err(VnidropError::SecureStorageCorrupted {
                reason: "protected secret has invalid key material".to_string(),
            });
        }
        Ok(material)
    }

    fn endpoint_id(&self) -> String {
        self.secret_key().public().to_string()
    }

    fn secret_key(&self) -> SecretKey {
        let bytes: &[u8; SECRET_BYTES] = self.as_bytes().try_into().expect("validated length");
        SecretKey::from_bytes(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn to_vec(&self) -> Vec<u8> {
        self.0.clone()
    }
}

impl PartialEq for SecretMaterial {
    fn eq(&self, other: &Self) -> bool {
        bool::from(self.as_bytes().ct_eq(other.as_bytes()))
    }
}

impl Eq for SecretMaterial {}

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

    pub(crate) fn from_stored(value: String) -> Self {
        Self(value)
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
    TargetedAuthorization,
}

impl SecretKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::EndpointIdentity => "endpoint-identity",
            Self::RelationshipGrant => "relationship-grant",
            Self::PairingEligibility => "pairing-eligibility",
            Self::TargetedAuthorization => "targeted-authorization",
        }
    }

    fn parse(value: &str) -> Result<Self, VnidropError> {
        match value {
            "endpoint-identity" => Ok(Self::EndpointIdentity),
            "relationship-grant" => Ok(Self::RelationshipGrant),
            "pairing-eligibility" => Ok(Self::PairingEligibility),
            "targeted-authorization" => Ok(Self::TargetedAuthorization),
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
        .secret_key();
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

    pub(crate) fn for_explicit_identity_reset(
        metadata: SecretMetadataStore,
        store: Arc<dyn SecureSecretStore>,
    ) -> Self {
        Self::from_parts(metadata, store)
    }

    /// Reject reset while the current identity remains readable. Missing or
    /// corrupted endpoint custody is unrecoverable without an explicit reset.
    pub(crate) async fn require_unrecoverable_endpoint_identity(&self) -> Result<(), VnidropError> {
        let endpoints = self
            .metadata
            .list()
            .await?
            .into_iter()
            .filter(|entry| {
                entry.kind == SecretKind::EndpointIdentity
                    && entry.state != SecretMetadataState::Disabled
            })
            .collect::<Vec<_>>();
        if endpoints.is_empty() {
            // Idempotent continuation after a reset transaction committed but
            // the process stopped before a replacement identity was activated.
            return Ok(());
        }
        for endpoint in endpoints {
            match self.store_get_raw(endpoint.handle).await? {
                Ok(material) => {
                    if validate_material(
                        SecretKind::EndpointIdentity,
                        &material,
                        endpoint.expected_identity.as_deref(),
                    )
                    .is_ok()
                    {
                        return Err(VnidropError::InvalidInput {
                            reason: "protected endpoint identity is still available".to_string(),
                        });
                    }
                }
                Err(SecureSecretStoreError::Missing | SecureSecretStoreError::Corrupted) => {}
                Err(error) => return Err(map_store_error(error)),
            }
        }
        Ok(())
    }

    pub(crate) async fn delete_reset_handles(
        &self,
        handles: Vec<String>,
    ) -> Result<(), VnidropError> {
        for handle in handles {
            self.delete_if_present(&SecretHandle::from_stored(handle))
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn protect(
        &self,
        kind: SecretKind,
        material: SecretMaterial,
        expected_identity: Option<&str>,
    ) -> Result<SecretHandle, VnidropError> {
        validate_material(kind, &material, expected_identity)?;
        let handle = SecretHandle::generate(kind);
        self.store_put(handle.clone(), material.clone()).await?;
        #[cfg(test)]
        self.maybe_crash(CustodyCrashPoint::StoreWrite)?;
        let stored = self.store_get(handle.clone()).await?;
        if stored != material {
            return Err(VnidropError::SecureStorageCorrupted {
                reason: "credential store did not preserve protected material".to_string(),
            });
        }
        validate_material(kind, &stored, expected_identity)?;
        if let Err(error) = self.metadata.stage(&handle, kind, expected_identity).await {
            self.delete_if_present(&handle).await?;
            return Err(error);
        }
        #[cfg(test)]
        self.maybe_crash(CustodyCrashPoint::MetadataStage)?;
        self.metadata.activate(&handle).await?;
        #[cfg(test)]
        self.maybe_crash(CustodyCrashPoint::MetadataActivation)?;
        Ok(handle)
    }

    #[cfg(test)]
    pub(crate) async fn create_orphaned_targeted_authorization_for_test(
        &self,
    ) -> Result<(), VnidropError> {
        self.protect(
            SecretKind::TargetedAuthorization,
            SecretMaterial::new(vec![7; 32])?,
            None,
        )
        .await?;
        Ok(())
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
        let material = self.store_get(handle.clone()).await?;
        validate_material(
            metadata.kind,
            &material,
            metadata.expected_identity.as_deref(),
        )?;
        Ok(material)
    }

    /// Disables access before deleting the protected material. Idempotent.
    pub(crate) async fn remove(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        self.metadata.disable(handle).await?;
        self.delete_if_present(handle).await?;
        self.metadata.purge_disabled(handle).await
    }

    pub(crate) async fn list_active_handles(
        &self,
        kind: SecretKind,
    ) -> Result<Vec<SecretHandle>, VnidropError> {
        Ok(self
            .metadata
            .list()
            .await?
            .into_iter()
            .filter(|entry| entry.kind == kind && entry.state == SecretMetadataState::Active)
            .map(|entry| entry.handle)
            .collect())
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
                    remove_legacy_identity_durably(legacy_path).await?;
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
        remove_legacy_identity_durably(legacy_path).await?;
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
            // Concurrent first-start may have staged (not yet active) metadata.
            // Wait for activation before treating leftover rows as disabled.
            return match self.wait_for_active_endpoint_identity().await {
                Ok(handle) => self.load(&handle).await,
                Err(_) => Err(VnidropError::SecureStorageUnavailable {
                    reason: "protected endpoint identity is disabled".to_string(),
                }),
            };
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
                    Err(error) => match self.wait_for_active_endpoint_identity().await {
                        Ok(handle) => self.load(&handle).await,
                        Err(_) => Err(error),
                    },
                }
            }
            Err(error) => Err(VnidropError::filesystem(error)),
        }
    }

    async fn wait_for_active_endpoint_identity(&self) -> Result<SecretHandle, VnidropError> {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(active) = self
                    .metadata
                    .find_active_kind(SecretKind::EndpointIdentity)
                    .await?
                {
                    return Ok(active.handle);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| VnidropError::SecureStorageUnavailable {
            reason: "timed out waiting for protected endpoint identity".to_string(),
        })?
    }

    pub(crate) async fn reconcile(&self) -> Result<ReconciliationSummary, VnidropError> {
        let metadata = self.metadata.list().await?;
        let stored_handles = self.store_list_handles().await?;
        let known_handles = metadata
            .iter()
            .map(|entry| entry.handle.clone())
            .collect::<HashSet<_>>();
        let mut summary = ReconciliationSummary::default();

        for entry in metadata {
            if entry.state == SecretMetadataState::Disabled {
                // The identity marker prevents an implicit rekey on startup.
                if entry.kind != SecretKind::EndpointIdentity
                    || stored_handles.contains(&entry.handle)
                {
                    self.delete_if_present(&entry.handle).await?;
                }
                self.metadata.purge_disabled(&entry.handle).await?;
                continue;
            }
            match self.store_get_raw(entry.handle.clone()).await? {
                Ok(material) => {
                    if validate_material(entry.kind, &material, entry.expected_identity.as_deref())
                        .is_err()
                    {
                        if entry.kind == SecretKind::EndpointIdentity {
                            return Err(VnidropError::SecureStorageCorrupted {
                                reason: "protected endpoint identity is corrupted".to_string(),
                            });
                        }
                        self.remove(&entry.handle).await?;
                        summary.disabled += 1;
                    } else if entry.state == SecretMetadataState::Staged {
                        self.metadata.activate(&entry.handle).await?;
                        summary.staged_activated += 1;
                    }
                }
                Err(
                    error @ (SecureSecretStoreError::Missing | SecureSecretStoreError::Corrupted),
                ) => {
                    if entry.kind == SecretKind::EndpointIdentity {
                        return Err(map_store_error(error));
                    }
                    self.remove(&entry.handle).await?;
                    summary.disabled += 1;
                }
                Err(error) => return Err(map_store_error(error)),
            }
        }

        for handle in stored_handles {
            if !known_handles.contains(&handle) {
                self.store_delete(handle).await?;
                summary.orphans_deleted += 1;
            }
        }
        Ok(summary)
    }

    pub(crate) async fn remove_orphaned_targeted_authorizations(
        &self,
        referenced_handles: &HashSet<String>,
    ) -> Result<u64, VnidropError> {
        let entries = self.metadata.list().await?;
        let mut removed = 0;
        for entry in entries {
            if entry.kind == SecretKind::TargetedAuthorization
                && entry.state == SecretMetadataState::Active
                && !referenced_handles.contains(entry.handle.as_str())
            {
                self.remove(&entry.handle).await?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn delete_if_present(&self, handle: &SecretHandle) -> Result<(), VnidropError> {
        match self.store_delete_raw(handle.clone()).await? {
            Ok(()) | Err(SecureSecretStoreError::Missing) => Ok(()),
            Err(error) => Err(map_store_error(error)),
        }
    }

    // Platform credential stores (especially Linux Secret Service via zbus
    // blocking) nest their own Tokio `block_on`. Calling them on a worker
    // already inside Vnidrop's runtime panics with "Cannot start a runtime
    // from within a runtime" and breaks protected-core desktop startup.
    async fn store_put(
        &self,
        handle: SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), VnidropError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.put(&handle, material))
            .await
            .map_err(VnidropError::internal)?
            .map_err(map_store_error)
    }

    async fn store_get(&self, handle: SecretHandle) -> Result<SecretMaterial, VnidropError> {
        self.store_get_raw(handle).await?.map_err(map_store_error)
    }

    async fn store_get_raw(
        &self,
        handle: SecretHandle,
    ) -> Result<Result<SecretMaterial, SecureSecretStoreError>, VnidropError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.get(&handle))
            .await
            .map_err(VnidropError::internal)
    }

    async fn store_delete(&self, handle: SecretHandle) -> Result<(), VnidropError> {
        self.store_delete_raw(handle)
            .await?
            .map_err(map_store_error)
    }

    async fn store_delete_raw(
        &self,
        handle: SecretHandle,
    ) -> Result<Result<(), SecureSecretStoreError>, VnidropError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.delete(&handle))
            .await
            .map_err(VnidropError::internal)
    }

    async fn store_list_handles(&self) -> Result<Vec<SecretHandle>, VnidropError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.list_handles())
            .await
            .map_err(VnidropError::internal)?
            .map_err(map_store_error)
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ReconciliationSummary {
    pub(crate) orphans_deleted: u64,
    pub(crate) staged_activated: u64,
    pub(crate) disabled: u64,
}

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
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

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceStoreFailure {
    Locked,
    Unavailable,
}

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
#[derive(Default)]
pub(crate) struct FaultInjectingSecretStore {
    values: Mutex<HashMap<SecretHandle, SecretMaterial>>,
    failure: Mutex<Option<ReferenceStoreFailure>>,
    corrupted: Mutex<Vec<SecretHandle>>,
}

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
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

    pub(crate) fn stored_value_count_for_test(&self) -> usize {
        self.values.lock().unwrap().len()
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

    pub(crate) fn endpoint_identity_handle_for_test(&self) -> SecretHandle {
        let handles = self
            .values
            .lock()
            .unwrap()
            .keys()
            .filter(|handle| handle.as_str().contains("/endpoint-identity/"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            handles.len(),
            1,
            "expected exactly one protected endpoint identity"
        );
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

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
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
