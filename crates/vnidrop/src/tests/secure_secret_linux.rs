use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use secret_service::Error;

use crate::{
    repository::Repository,
    secure_secret::{
        linux::{map_error, LinuxSecretServiceApi, LinuxSecretServiceStore},
        SecretCustody, SecretHandle, SecretKind, SecretMaterial, SecureSecretStore,
        SecureSecretStoreError,
    },
    VnidropError,
};

#[derive(Default)]
struct RecordingSecretService {
    values: Mutex<HashMap<String, Vec<u8>>>,
    failure: Mutex<Option<SecureSecretStoreError>>,
}

impl RecordingSecretService {
    fn failure(&self) -> Result<(), SecureSecretStoreError> {
        match &*self.failure.lock().unwrap() {
            Some(SecureSecretStoreError::Locked) => Err(SecureSecretStoreError::Locked),
            Some(SecureSecretStoreError::Missing) => Err(SecureSecretStoreError::Missing),
            Some(SecureSecretStoreError::Corrupted) => Err(SecureSecretStoreError::Corrupted),
            Some(SecureSecretStoreError::Unavailable) => Err(SecureSecretStoreError::Unavailable),
            None => Ok(()),
        }
    }
}

impl LinuxSecretServiceApi for RecordingSecretService {
    fn put(&self, handle: &str, material: &[u8]) -> Result<(), SecureSecretStoreError> {
        self.failure()?;
        self.values
            .lock()
            .unwrap()
            .insert(handle.to_string(), material.to_vec());
        Ok(())
    }

    fn get(&self, handle: &str) -> Result<Vec<u8>, SecureSecretStoreError> {
        self.failure()?;
        self.values
            .lock()
            .unwrap()
            .get(handle)
            .cloned()
            .ok_or(SecureSecretStoreError::Missing)
    }

    fn delete(&self, handle: &str) -> Result<(), SecureSecretStoreError> {
        self.failure()?;
        self.values
            .lock()
            .unwrap()
            .remove(handle)
            .map(|_| ())
            .ok_or(SecureSecretStoreError::Missing)
    }

    fn list_handles(&self) -> Result<Vec<String>, SecureSecretStoreError> {
        self.failure()?;
        Ok(self.values.lock().unwrap().keys().cloned().collect())
    }
}

fn handle(suffix: &str) -> SecretHandle {
    crate::secure_secret::secret_handle_for_test(format!("vnidrop/v1/relationship-grant/{suffix}"))
}

#[test]
fn adapter_survives_restart_and_deletes_only_the_selected_item() {
    let api = Arc::new(RecordingSecretService::default());
    let first = handle("first");
    let second = handle("second");
    let material = SecretMaterial::new(vec![0x5a; 32]).unwrap();
    let store = LinuxSecretServiceStore::with_api(api.clone());
    store.put(&first, material.clone()).unwrap();
    store
        .put(&second, SecretMaterial::new(vec![0x6b; 32]).unwrap())
        .unwrap();

    let restarted = LinuxSecretServiceStore::with_api(api);
    assert_eq!(restarted.get(&first).unwrap(), material);
    assert_eq!(
        restarted.list_handles().unwrap(),
        vec![first.clone(), second]
    );
    restarted.delete(&first).unwrap();
    assert!(matches!(
        restarted.get(&first),
        Err(SecureSecretStoreError::Missing)
    ));
}

#[tokio::test]
async fn transient_backend_failures_do_not_delete_protected_metadata_or_material() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let api = Arc::new(RecordingSecretService::default());
    let store = Arc::new(LinuxSecretServiceStore::with_api(api.clone()));
    let custody = SecretCustody::new(repository.protected_secrets(), store.clone());
    let protected = custody
        .protect(
            SecretKind::RelationshipGrant,
            SecretMaterial::new(vec![0x7c; 32]).unwrap(),
            None,
        )
        .await
        .unwrap();

    *api.failure.lock().unwrap() = Some(SecureSecretStoreError::Unavailable);
    drop(custody);
    assert!(matches!(
        SecretCustody::start(repository.protected_secrets(), store.clone()).await,
        Err(VnidropError::SecureStorageUnavailable { .. })
    ));

    *api.failure.lock().unwrap() = None;
    let (restarted, _) = SecretCustody::start(repository.protected_secrets(), store)
        .await
        .unwrap();
    assert_eq!(
        restarted.load(&protected).await.unwrap(),
        SecretMaterial::new(vec![0x7c; 32]).unwrap()
    );
}

#[test]
fn failures_are_typed_and_secret_material_is_redacted() {
    let api = Arc::new(RecordingSecretService::default());
    let store = LinuxSecretServiceStore::with_api(api.clone());
    let secret = SecretMaterial::new(vec![0x7c; 32]).unwrap();
    assert_eq!(format!("{secret:?}"), "SecretMaterial(redacted)");

    for failure in [
        SecureSecretStoreError::Locked,
        SecureSecretStoreError::Unavailable,
        SecureSecretStoreError::Corrupted,
    ] {
        *api.failure.lock().unwrap() = Some(failure);
        assert!(store.get(&handle("failure")).is_err());
    }
}

#[test]
fn secret_service_errors_map_without_exposing_details() {
    assert!(matches!(
        map_error(Error::Locked),
        SecureSecretStoreError::Locked
    ));
    assert!(matches!(
        map_error(Error::NoResult),
        SecureSecretStoreError::Missing
    ));
    assert!(matches!(
        map_error(Error::Crypto("distinctive-secret")),
        SecureSecretStoreError::Corrupted
    ));
    assert!(matches!(
        map_error(Error::Unavailable),
        SecureSecretStoreError::Unavailable
    ));
}
