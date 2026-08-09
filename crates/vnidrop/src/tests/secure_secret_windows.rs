use std::{fs, sync::Arc};

use data_encoding::HEXLOWER;
use iroh::SecretKey;

use crate::{
    repository::Repository,
    secure_secret::{
        windows::WindowsDpapiSecretStore, CustodyCrashPoint, SecretCustody, SecretMaterial,
        SecureSecretStore, SecureSecretStoreError,
    },
    VnidropError,
};

fn handle() -> crate::secure_secret::SecretHandle {
    WindowsDpapiSecretStore::relationship_handle_for_test()
}

fn material(seed: u8) -> SecretMaterial {
    SecretMaterial::new(vec![seed; 32]).unwrap()
}

#[test]
fn round_trip_survives_adapter_restart_and_never_persists_plaintext() {
    let directory = tempfile::tempdir().unwrap();
    let handle = handle();
    let secret = material(0xa7);

    WindowsDpapiSecretStore::new(directory.path())
        .unwrap()
        .put(&handle, secret.clone())
        .unwrap();

    let restarted = WindowsDpapiSecretStore::new(directory.path()).unwrap();
    assert_eq!(restarted.get(&handle).unwrap(), secret);
    assert_eq!(restarted.list_handles().unwrap(), vec![handle]);

    for entry in fs::read_dir(directory.path()).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes.windows(32).any(|window| window == [0xa7; 32]));
    }
}

#[test]
fn delete_removes_only_the_selected_protected_value() {
    let directory = tempfile::tempdir().unwrap();
    let store = WindowsDpapiSecretStore::new(directory.path()).unwrap();
    let retained = handle();
    let removed = handle();
    store.put(&retained, material(1)).unwrap();
    store.put(&removed, material(2)).unwrap();

    store.delete(&removed).unwrap();

    assert!(matches!(
        store.get(&removed),
        Err(SecureSecretStoreError::Missing)
    ));
    assert_eq!(store.get(&retained).unwrap(), material(1));
    assert_eq!(store.list_handles().unwrap(), vec![retained]);
}

#[test]
fn repeated_put_is_idempotent_and_atomically_updates_changed_material() {
    let directory = tempfile::tempdir().unwrap();
    let store = WindowsDpapiSecretStore::new(directory.path()).unwrap();
    let handle = handle();

    store.put(&handle, material(6)).unwrap();
    let first_blob = fs::read(store.path_for_test(&handle)).unwrap();
    store.put(&handle, material(6)).unwrap();
    assert_eq!(fs::read(store.path_for_test(&handle)).unwrap(), first_blob);

    store.put(&handle, material(7)).unwrap();
    assert_eq!(store.get(&handle).unwrap(), material(7));
    assert!(!fs::read(store.path_for_test(&handle))
        .unwrap()
        .windows(32)
        .any(|window| window == [7; 32]));
}

#[test]
fn missing_corrupt_and_wrong_context_values_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let handle = handle();
    let store = WindowsDpapiSecretStore::new(directory.path()).unwrap();
    assert!(matches!(
        store.get(&handle),
        Err(SecureSecretStoreError::Missing)
    ));

    store.put(&handle, material(3)).unwrap();
    fs::write(store.path_for_test(&handle), b"not a protected envelope").unwrap();
    assert!(matches!(
        store.get(&handle),
        Err(SecureSecretStoreError::Corrupted)
    ));

    let isolated = tempfile::tempdir().unwrap();
    let original =
        WindowsDpapiSecretStore::with_context_for_test(isolated.path(), b"first-context").unwrap();
    original.put(&handle, material(4)).unwrap();
    let wrong_context =
        WindowsDpapiSecretStore::with_context_for_test(isolated.path(), b"second-context").unwrap();
    assert!(matches!(
        wrong_context.get(&handle),
        Err(SecureSecretStoreError::Corrupted)
    ));
}

#[test]
fn interrupted_replacement_preserves_the_previous_value() {
    let directory = tempfile::tempdir().unwrap();
    let handle = handle();
    let store = WindowsDpapiSecretStore::new(directory.path()).unwrap();
    store.put(&handle, material(5)).unwrap();
    let temporary = directory.path().join("interrupted.tmp-123");
    fs::write(&temporary, b"incomplete protected replacement").unwrap();

    let restarted = WindowsDpapiSecretStore::new(directory.path()).unwrap();

    assert!(!temporary.exists());
    assert_eq!(restarted.get(&handle).unwrap(), material(5));
}

#[test]
fn unusable_backing_path_is_reported_as_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("not-a-directory");
    fs::write(&file, b"occupied").unwrap();

    assert!(matches!(
        WindowsDpapiSecretStore::new(&file),
        Err(SecureSecretStoreError::Unavailable)
    ));
}

#[tokio::test]
async fn endpoint_migration_survives_activation_crash_without_changing_identity() {
    let directory = tempfile::tempdir().unwrap();
    let app_data = directory.path().join("app-data");
    fs::create_dir(&app_data).unwrap();
    let legacy = app_data.join("iroh.secret");
    let original = SecretKey::generate();
    fs::write(&legacy, HEXLOWER.encode(&original.to_bytes())).unwrap();

    let repository = Repository::open(&app_data).await.unwrap();
    let protected_directory = app_data.join("protected-secrets");
    let store = Arc::new(WindowsDpapiSecretStore::new(&protected_directory).unwrap());
    let custody = SecretCustody::new(repository.protected_secrets(), store);
    custody.crash_once_at(CustodyCrashPoint::MetadataActivation);
    assert!(custody
        .migrate_legacy_endpoint_identity(&legacy)
        .await
        .is_err());
    assert!(legacy.exists());
    drop(custody);
    drop(repository);

    let repository = Repository::open(&app_data).await.unwrap();
    let restarted_store = Arc::new(WindowsDpapiSecretStore::new(&protected_directory).unwrap());
    let (custody, _) = SecretCustody::start(repository.protected_secrets(), restarted_store)
        .await
        .unwrap();
    let handle = custody
        .migrate_legacy_endpoint_identity(&legacy)
        .await
        .unwrap();
    assert!(!legacy.exists());
    assert_eq!(
        custody.load(&handle).await.unwrap(),
        SecretMaterial::new(original.to_bytes().to_vec()).unwrap()
    );

    let replacement = SecretKey::generate();
    fs::write(&legacy, HEXLOWER.encode(&replacement.to_bytes())).unwrap();
    assert!(matches!(
        custody.migrate_legacy_endpoint_identity(&legacy).await,
        Err(VnidropError::SecureStorageCorrupted { .. })
    ));
    assert!(legacy.exists());
    assert_eq!(
        custody.load(&handle).await.unwrap(),
        SecretMaterial::new(original.to_bytes().to_vec()).unwrap()
    );
}
