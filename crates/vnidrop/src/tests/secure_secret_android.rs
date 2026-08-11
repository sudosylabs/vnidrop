use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
};

use tempfile::TempDir;

use crate::secure_secret::{
    android::{
        secret_handle_for_test, AndroidKeystore, AndroidSealedValue, AndroidSecureSecretStore,
    },
    SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError,
};

const TEST_SECRET_BYTES: usize = 32;

#[derive(Default)]
struct FakeKeystore {
    keys: Mutex<HashMap<String, u8>>,
    seal_failure: Mutex<Option<SecureSecretStoreError>>,
    delete_failure: Mutex<Option<SecureSecretStoreError>>,
}

impl AndroidKeystore for FakeKeystore {
    fn seal(
        &self,
        alias: &str,
        plaintext: &[u8],
    ) -> Result<AndroidSealedValue, SecureSecretStoreError> {
        if let Some(error) = self.seal_failure.lock().unwrap().take() {
            return Err(error);
        }
        let mask = 0xa7;
        self.keys.lock().unwrap().insert(alias.to_string(), mask);
        Ok(AndroidSealedValue {
            nonce: vec![4; 12],
            ciphertext: plaintext.iter().map(|byte| byte ^ mask).collect(),
        })
    }

    fn open(
        &self,
        alias: &str,
        sealed: &AndroidSealedValue,
    ) -> Result<Vec<u8>, SecureSecretStoreError> {
        let mask = *self
            .keys
            .lock()
            .unwrap()
            .get(alias)
            .ok_or(SecureSecretStoreError::Missing)?;
        Ok(sealed.ciphertext.iter().map(|byte| byte ^ mask).collect())
    }

    fn delete(&self, alias: &str) -> Result<(), SecureSecretStoreError> {
        if let Some(error) = self.delete_failure.lock().unwrap().take() {
            return Err(error);
        }
        self.keys.lock().unwrap().remove(alias);
        Ok(())
    }
}

fn fixture() -> (TempDir, AndroidSecureSecretStore, Arc<FakeKeystore>) {
    let directory = TempDir::new().unwrap();
    let keystore = Arc::new(FakeKeystore::default());
    let store = AndroidSecureSecretStore::new(directory.path(), keystore.clone()).unwrap();
    (directory, store, keystore)
}

fn handle() -> SecretHandle {
    secret_handle_for_test("vnidrop/v1/endpoint-identity/test")
}

#[test]
fn adapter_round_trips_lists_and_deletes_without_plaintext_persistence() {
    let (directory, store, keystore) = fixture();
    let handle = handle();
    let plaintext = vec![0x5a; TEST_SECRET_BYTES];
    let material = SecretMaterial::new(plaintext.clone()).unwrap();

    store.put(&handle, material.clone()).unwrap();

    let persisted = fs::read(store.record_path_for_test(&handle)).unwrap();
    assert!(!persisted
        .windows(plaintext.len())
        .any(|window| window == plaintext));

    drop(store);
    let restarted = AndroidSecureSecretStore::new(directory.path(), keystore).unwrap();
    assert_eq!(restarted.list_handles().unwrap(), vec![handle.clone()]);
    assert_eq!(restarted.get(&handle).unwrap(), material);

    restarted.delete(&handle).unwrap();
    assert!(restarted.list_handles().unwrap().is_empty());
    assert!(matches!(
        restarted.get(&handle),
        Err(SecureSecretStoreError::Missing)
    ));
}

#[test]
fn staged_crash_record_remains_discoverable_and_fails_closed() {
    let (_directory, store, _keystore) = fixture();
    let handle = handle();
    store.stage_for_test(&handle).unwrap();

    assert_eq!(store.list_handles().unwrap(), vec![handle.clone()]);
    assert!(matches!(
        store.get(&handle),
        Err(SecureSecretStoreError::Corrupted)
    ));
    store.delete(&handle).unwrap();
    assert!(store.list_handles().unwrap().is_empty());
}

#[test]
fn tampering_and_missing_keystore_keys_are_distinct_failures() {
    let (_directory, store, keystore) = fixture();
    let handle = handle();
    store
        .put(
            &handle,
            SecretMaterial::new(vec![9; TEST_SECRET_BYTES]).unwrap(),
        )
        .unwrap();

    keystore.keys.lock().unwrap().clear();
    assert!(matches!(
        store.get(&handle),
        Err(SecureSecretStoreError::Missing)
    ));

    fs::write(store.record_path_for_test(&handle), b"tampered").unwrap();
    assert!(matches!(
        store.get(&handle),
        Err(SecureSecretStoreError::Corrupted)
    ));
}

#[test]
fn failed_key_deletion_retains_the_record_for_safe_retry() {
    let (_directory, store, keystore) = fixture();
    let handle = handle();
    store
        .put(
            &handle,
            SecretMaterial::new(vec![7; TEST_SECRET_BYTES]).unwrap(),
        )
        .unwrap();
    *keystore.delete_failure.lock().unwrap() = Some(SecureSecretStoreError::Locked);

    assert!(matches!(
        store.delete(&handle),
        Err(SecureSecretStoreError::Locked)
    ));
    assert_eq!(store.list_handles().unwrap(), vec![handle.clone()]);

    store.delete(&handle).unwrap();
    assert!(store.list_handles().unwrap().is_empty());
}

#[test]
fn failed_replacement_keeps_the_previous_secret_readable() {
    let (_directory, store, keystore) = fixture();
    let handle = handle();
    let original = SecretMaterial::new(vec![3; TEST_SECRET_BYTES]).unwrap();
    store.put(&handle, original.clone()).unwrap();
    *keystore.seal_failure.lock().unwrap() = Some(SecureSecretStoreError::Locked);

    assert!(matches!(
        store.put(
            &handle,
            SecretMaterial::new(vec![8; TEST_SECRET_BYTES]).unwrap()
        ),
        Err(SecureSecretStoreError::Locked)
    ));
    assert_eq!(store.get(&handle).unwrap(), original);
}

#[test]
fn scoped_handle_record_names_fit_linux_name_max() {
    let (_directory, store, _keystore) = fixture();
    // Mirrors ScopedSecretStore physical handles used on Android profiles.
    let handle = secret_handle_for_test(&format!(
        "vnidrop/v1/scope-{}/endpoint-identity/{}",
        "a".repeat(64),
        "b".repeat(36),
    ));
    assert!(handle.as_str().len() > 100);
    let path = store.record_path_for_test(&handle);
    let file_name = path.file_name().and_then(|value| value.to_str()).unwrap();
    assert!(
        file_name.len() <= 255,
        "record file name exceeds NAME_MAX: {} bytes ({file_name})",
        file_name.len()
    );

    let material = SecretMaterial::new(vec![0x5a; TEST_SECRET_BYTES]).unwrap();
    store.put(&handle, material.clone()).unwrap();
    assert_eq!(store.list_handles().unwrap(), vec![handle.clone()]);
    assert_eq!(store.get(&handle).unwrap(), material);
}
