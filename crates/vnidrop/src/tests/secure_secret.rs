use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use data_encoding::HEXLOWER;
use iroh::SecretKey;

use crate::{
    repository::Repository,
    secure_secret::{
        CustodyCrashPoint, FaultInjectingSecretStore, ReferenceStoreFailure, SecretCustody,
        SecretKind, SecretMaterial,
    },
    VnidropError,
};

#[derive(Clone, Default)]
struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

struct CapturedWriter(CapturedOutput);

impl Write for CapturedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 .0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn custody_maps_reference_store_failures_to_typed_core_errors() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let store = Arc::new(FaultInjectingSecretStore::default());
    let custody = SecretCustody::new(repository.protected_secrets(), store.clone());
    let secret = SecretMaterial::new(vec![0x5a; 32]).unwrap();
    let handle = custody
        .protect(SecretKind::RelationshipGrant, secret.clone(), None)
        .await
        .unwrap();

    assert_eq!(custody.load(&handle).await.unwrap(), secret);

    store.fail_with(Some(ReferenceStoreFailure::Locked));
    assert!(matches!(
        custody.load(&handle).await,
        Err(VnidropError::SecureStorageLocked { .. })
    ));

    store.fail_with(Some(ReferenceStoreFailure::Unavailable));
    assert!(matches!(
        custody.load(&handle).await,
        Err(VnidropError::SecureStorageUnavailable { .. })
    ));

    store.fail_with(None);
    store.remove_for_test(&handle);
    assert!(matches!(
        custody.load(&handle).await,
        Err(VnidropError::SecureStorageMissing { .. })
    ));

    let corrupted_handle = custody
        .protect(SecretKind::RelationshipGrant, secret, None)
        .await
        .unwrap();
    store.corrupt_for_test(&corrupted_handle);
    assert!(matches!(
        custody.load(&corrupted_handle).await,
        Err(VnidropError::SecureStorageCorrupted { .. })
    ));
}

#[tokio::test]
async fn reconciliation_repairs_staged_metadata_and_disables_unusable_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(temp.path()).await.unwrap();
    let store = Arc::new(FaultInjectingSecretStore::default());
    let custody = SecretCustody::new(repository.protected_secrets(), store.clone());

    custody.crash_once_at(CustodyCrashPoint::StoreWrite);
    assert!(custody
        .protect(
            SecretKind::PairingEligibility,
            SecretMaterial::new(vec![0x11; 32]).unwrap(),
            None,
        )
        .await
        .is_err());
    drop(custody);
    drop(repository);
    repository = Repository::open(temp.path()).await.unwrap();
    let (custody, summary) = SecretCustody::start(repository.protected_secrets(), store.clone())
        .await
        .unwrap();
    assert_eq!(summary.orphans_deleted, 1);
    assert_eq!(summary.staged_activated, 0);

    custody.crash_once_at(CustodyCrashPoint::MetadataStage);
    assert!(custody
        .protect(
            SecretKind::PairingEligibility,
            SecretMaterial::new(vec![0x22; 32]).unwrap(),
            None,
        )
        .await
        .is_err());
    let staged_handle = store.only_handle_for_test();
    drop(custody);
    drop(repository);
    repository = Repository::open(temp.path()).await.unwrap();
    let (custody, summary) = SecretCustody::start(repository.protected_secrets(), store.clone())
        .await
        .unwrap();
    assert_eq!(summary.staged_activated, 1);
    assert_eq!(
        custody.load(&staged_handle).await.unwrap(),
        SecretMaterial::new(vec![0x22; 32]).unwrap()
    );

    store.remove_for_test(&staged_handle);
    drop(custody);
    drop(repository);
    repository = Repository::open(temp.path()).await.unwrap();
    let (custody, summary) = SecretCustody::start(repository.protected_secrets(), store.clone())
        .await
        .unwrap();
    assert_eq!(summary.disabled, 1);
    assert!(matches!(
        custody.load(&staged_handle).await,
        Err(VnidropError::SecureStorageUnavailable { .. })
    ));

    let corrupted = custody
        .protect(
            SecretKind::RelationshipGrant,
            SecretMaterial::new(vec![0x33; 32]).unwrap(),
            None,
        )
        .await
        .unwrap();
    store.corrupt_for_test(&corrupted);
    drop(custody);
    drop(repository);
    let repository = Repository::open(temp.path()).await.unwrap();
    let (custody, summary) = SecretCustody::start(repository.protected_secrets(), store.clone())
        .await
        .unwrap();
    assert_eq!(summary.disabled, 1);
    assert!(matches!(
        custody.load(&corrupted).await,
        Err(VnidropError::SecureStorageUnavailable { .. })
    ));
}

#[tokio::test]
async fn endpoint_migration_preserves_identity_across_crash_and_rejects_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let legacy_path = temp.path().join("iroh.secret");
    let original = SecretKey::generate();
    std::fs::write(&legacy_path, HEXLOWER.encode(&original.to_bytes())).unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let store = Arc::new(FaultInjectingSecretStore::default());
    let custody = SecretCustody::new(repository.protected_secrets(), store.clone());

    custody.crash_once_at(CustodyCrashPoint::MetadataActivation);
    assert!(custody
        .migrate_legacy_endpoint_identity(&legacy_path)
        .await
        .is_err());
    assert!(
        legacy_path.exists(),
        "legacy key must survive before activation"
    );

    drop(custody);
    drop(repository);
    let repository = Repository::open(temp.path()).await.unwrap();
    let (custody, summary) = SecretCustody::start(repository.protected_secrets(), store.clone())
        .await
        .unwrap();
    assert_eq!(summary.staged_activated, 0);
    let handle = custody
        .migrate_legacy_endpoint_identity(&legacy_path)
        .await
        .unwrap();
    assert!(!legacy_path.exists());
    assert_eq!(
        custody.load(&handle).await.unwrap(),
        SecretMaterial::new(original.to_bytes().to_vec()).unwrap()
    );

    let replacement = SecretKey::generate();
    std::fs::write(&legacy_path, HEXLOWER.encode(&replacement.to_bytes())).unwrap();
    assert!(matches!(
        custody.migrate_legacy_endpoint_identity(&legacy_path).await,
        Err(VnidropError::SecureStorageCorrupted { .. })
    ));
    assert!(legacy_path.exists());
    assert_eq!(
        custody.load(&handle).await.unwrap(),
        SecretMaterial::new(original.to_bytes().to_vec()).unwrap()
    );

    let missing = temp.path().join("missing.secret");
    let empty_store = Arc::new(FaultInjectingSecretStore::default());
    let other_dir = temp.path().join("other");
    std::fs::create_dir(&other_dir).unwrap();
    let other_repository = Repository::open(&other_dir).await.unwrap();
    let empty_custody = SecretCustody::new(other_repository.protected_secrets(), empty_store);
    assert!(matches!(
        empty_custody
            .migrate_legacy_endpoint_identity(&missing)
            .await,
        Err(VnidropError::SecureStorageMissing { .. })
    ));
}

#[tokio::test]
async fn protected_material_is_absent_from_database_and_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let store = Arc::new(FaultInjectingSecretStore::default());
    let custody = SecretCustody::new(repository.protected_secrets(), store.clone());
    let raw = (0u8..32).map(|value| value + 1).collect::<Vec<_>>();
    let encoded = HEXLOWER.encode(&raw);
    let material = SecretMaterial::new(raw.clone()).unwrap();

    let handle = custody
        .protect(SecretKind::RelationshipGrant, material.clone(), None)
        .await
        .unwrap();

    assert!(handle
        .as_str()
        .starts_with("vnidrop/v1/relationship-grant/"));
    assert_eq!(format!("{material:?}"), "SecretMaterial(redacted)");
    store.corrupt_for_test(&handle);
    let error = custody.load(&handle).await.unwrap_err().to_string();
    assert!(!error.contains(&encoded));

    let captured = CapturedOutput::default();
    let writer_output = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_writer(move || CapturedWriter(writer_output.clone()))
        .finish();
    let _subscriber = tracing::subscriber::set_default(subscriber);
    tracing::info!(material = ?material, error, "custody diagnostic");
    let diagnostics = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(!diagnostics.contains(&encoded));

    assert!(repository.list_events(None, 500).await.unwrap().is_empty());

    let mut persisted = Vec::new();
    for entry in std::fs::read_dir(temp.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            persisted.extend(std::fs::read(path).unwrap());
        }
    }
    assert!(!persisted.windows(raw.len()).any(|window| window == raw));
    assert!(!persisted
        .windows(encoded.len())
        .any(|window| window == encoded.as_bytes()));
}
