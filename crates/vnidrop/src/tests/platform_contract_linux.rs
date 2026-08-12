//! Linux core/platform contract harness for saved devices (ticket 17).
//!
//! Proves the Secret Service bridge can drive identity restart, the public
//! saved-device lifecycle, fault isolation, event recovery, and binding hygiene
//! without product UI. Injectable Secret Service fakes run on every host;
//! real Secret Service connect is exercised under `cfg(target_os = "linux")`.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    secure_secret::{
        linux::{LinuxSecretServiceApi, LinuxSecretServiceStore},
        FaultInjectingSecretStore, ReferenceStoreFailure, SecretMaterial, SecureSecretStore,
        SecureSecretStoreError,
    },
    CoreEvent, CoreEventSink, DeviceRelationshipState, ShareMetadataInput, ShareSource, SourceKind,
    TargetedTransferState, TransferAccessMode, VnidropCore, VnidropError,
};
use data_encoding::HEXLOWER;
use iroh::SecretKey;

#[cfg(target_os = "linux")]
use crate::{CoreLimits, CoreNetworkConfig};

struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl CoreEventSink for RecordingSink {
    fn on_event(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingSink {
    fn snapshot(&self) -> Vec<CoreEvent> {
        self.events.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

/// Node backed by an injectable Secret Service API (runs on non-Linux hosts).
struct SecretServiceNode {
    data_dir: tempfile::TempDir,
    api: Arc<ControllableSecretService>,
    sink: Arc<RecordingSink>,
    core: Option<Arc<VnidropCore>>,
}

impl SecretServiceNode {
    fn new_with_legacy(identity: &SecretKey) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            data_dir.path().join("iroh.secret"),
            HEXLOWER.encode(&identity.to_bytes()),
        )
        .unwrap();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let api = Arc::new(ControllableSecretService::default());
        let store = Arc::new(LinuxSecretServiceStore::with_api(api.clone()));
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store,
        )
        .expect("Linux legacy migration");
        Self {
            data_dir,
            api,
            sink,
            core: Some(core),
        }
    }
    fn new() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let api = Arc::new(ControllableSecretService::default());
        let store = Arc::new(LinuxSecretServiceStore::with_api(api.clone()));
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store,
        )
        .expect("Secret Service-backed protected core");
        Self {
            data_dir,
            api,
            sink,
            core: Some(core),
        }
    }

    fn core(&self) -> Arc<VnidropCore> {
        self.core.as_ref().expect("core alive").clone()
    }

    fn restart(mut self) -> Self {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = Arc::new(LinuxSecretServiceStore::with_api(self.api.clone()));
        let core = VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store,
        )
        .expect("restarted Secret Service-backed core");
        self.sink = sink;
        self.core = Some(core);
        self
    }

    fn try_restart(mut self) -> Result<Self, (Self, VnidropError)> {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = Arc::new(LinuxSecretServiceStore::with_api(self.api.clone()));
        match VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store,
        ) {
            Ok(core) => {
                self.sink = sink;
                self.core = Some(core);
                Ok(self)
            }
            Err(error) => {
                self.sink = sink;
                Err((self, error))
            }
        }
    }
}

impl Drop for SecretServiceNode {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

struct FaultNode {
    data_dir: tempfile::TempDir,
    secret_store: Arc<FaultInjectingSecretStore>,
    sink: Arc<RecordingSink>,
    core: Option<Arc<VnidropCore>>,
}

impl FaultNode {
    fn new() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = Arc::new(FaultInjectingSecretStore::default());
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store.clone(),
        )
        .expect("fault-injecting protected core");
        Self {
            data_dir,
            secret_store: store,
            sink,
            core: Some(core),
        }
    }

    fn core(&self) -> Arc<VnidropCore> {
        self.core.as_ref().expect("core alive").clone()
    }

    fn restart(mut self) -> Self {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let core = VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            self.secret_store.clone(),
        )
        .expect("restarted fault-injecting core");
        self.sink = sink;
        self.core = Some(core);
        self
    }

    fn try_restart(mut self) -> Result<Self, (Self, VnidropError)> {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        match VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            self.secret_store.clone(),
        ) {
            Ok(core) => {
                self.sink = sink;
                self.core = Some(core);
                Ok(self)
            }
            Err(error) => {
                self.sink = sink;
                Err((self, error))
            }
        }
    }
}

#[test]
fn linux_adapter_protects_identity_before_plaintext_removal() {
    let identity = SecretKey::generate();
    let node = SecretServiceNode::new_with_legacy(&identity);
    assert!(!node.data_dir.path().join("iroh.secret").exists());
    let endpoint = node.core().status().endpoint_id;
    assert_eq!(endpoint, identity.public().to_string());
    let node = node.restart();
    assert_eq!(node.core().status().endpoint_id, endpoint);
}

impl Drop for FaultNode {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

#[derive(Clone, Default)]
struct ControllableSecretService {
    state: Arc<Mutex<ControllableState>>,
}

#[derive(Default)]
struct ControllableState {
    values: HashMap<String, Vec<u8>>,
    locked_handles: HashSet<String>,
    global_failure: Option<SecureSecretStoreError>,
}

impl ControllableSecretService {
    fn lock_handle(&self, handle: &str) {
        self.state
            .lock()
            .unwrap()
            .locked_handles
            .insert(handle.to_string());
    }

    fn fail_all(&self, failure: Option<SecureSecretStoreError>) {
        self.state.lock().unwrap().global_failure = failure;
    }

    fn check(&self, handle: &str) -> Result<(), SecureSecretStoreError> {
        let state = self.state.lock().unwrap();
        if let Some(failure) = &state.global_failure {
            return Err(match failure {
                SecureSecretStoreError::Locked => SecureSecretStoreError::Locked,
                SecureSecretStoreError::Missing => SecureSecretStoreError::Missing,
                SecureSecretStoreError::Corrupted => SecureSecretStoreError::Corrupted,
                SecureSecretStoreError::Unavailable => SecureSecretStoreError::Unavailable,
            });
        }
        if state.locked_handles.contains(handle) {
            return Err(SecureSecretStoreError::Locked);
        }
        Ok(())
    }
}

impl LinuxSecretServiceApi for ControllableSecretService {
    fn put(&self, handle: &str, material: &[u8]) -> Result<(), SecureSecretStoreError> {
        self.check(handle)?;
        self.state
            .lock()
            .unwrap()
            .values
            .insert(handle.to_string(), material.to_vec());
        Ok(())
    }

    fn get(&self, handle: &str) -> Result<Vec<u8>, SecureSecretStoreError> {
        self.check(handle)?;
        self.state
            .lock()
            .unwrap()
            .values
            .get(handle)
            .cloned()
            .ok_or(SecureSecretStoreError::Missing)
    }

    fn delete(&self, handle: &str) -> Result<(), SecureSecretStoreError> {
        self.check(handle)?;
        self.state
            .lock()
            .unwrap()
            .values
            .remove(handle)
            .map(|_| ())
            .ok_or(SecureSecretStoreError::Missing)
    }

    fn list_handles(&self) -> Result<Vec<String>, SecureSecretStoreError> {
        let state = self.state.lock().unwrap();
        if let Some(failure) = &state.global_failure {
            return Err(match failure {
                SecureSecretStoreError::Locked => SecureSecretStoreError::Locked,
                SecureSecretStoreError::Missing => SecureSecretStoreError::Missing,
                SecureSecretStoreError::Corrupted => SecureSecretStoreError::Corrupted,
                SecureSecretStoreError::Unavailable => SecureSecretStoreError::Unavailable,
            });
        }
        Ok(state.values.keys().cloned().collect())
    }
}

fn share_path(core: &VnidropCore, source: &Path, transfer_id: u64) -> crate::ShareResult {
    core.share_files(
        vec![ShareSource {
            kind: SourceKind::Path,
            value: source.to_string_lossy().into_owned(),
            display_name: Some("hello.txt".to_string()),
            is_directory: false,
        }],
        ShareMetadataInput {
            transfer_id,
            transfer_name: Some("hello.txt".to_string()),
            sender_name: Some("sender".to_string()),
            access_mode: TransferAccessMode::ApprovalRequired,
        },
    )
    .unwrap()
}

fn wait_for_receiver_request(sender: &VnidropCore, transfer_id: u64) -> crate::ReceiverRequest {
    let started = Instant::now();
    loop {
        if let Some(request) = sender
            .list_receiver_requests(transfer_id)
            .unwrap()
            .into_iter()
            .find(|request| request.status == "requested")
        {
            return request;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "timed out waiting for receiver request"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn complete_transfer(sender: &Arc<VnidropCore>, receiver: &Arc<VnidropCore>, transfer_id: u64) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"mutual consent").unwrap();
    let share = share_path(sender, &source_path, transfer_id);
    let output_dir = output_dir.path().to_string_lossy().to_string();
    let receiver_core = receiver.clone();
    let ticket = share.ticket.clone();
    let handle = std::thread::spawn(move || {
        receiver_core.receive(ticket, output_dir, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(sender, share.transfer_id);
    sender
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();

    let started = Instant::now();
    let peer = receiver.status().endpoint_id.clone();
    loop {
        if sender
            .list_pairing_eligibilities()
            .unwrap()
            .iter()
            .any(|entry| entry.peer_endpoint_id == peer)
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "eligibility never appeared"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_relationship(
    core: &VnidropCore,
    peer: &str,
    state: DeviceRelationshipState,
) -> crate::DeviceRelationship {
    let started = Instant::now();
    loop {
        if let Some(relationship) = core
            .list_device_relationships()
            .unwrap()
            .into_iter()
            .find(|entry| entry.remote_endpoint_id == peer && entry.state == state)
        {
            return relationship;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "relationship {peer} never reached {state:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn establish_saved(alice: &Arc<VnidropCore>, bob: &Arc<VnidropCore>, transfer_id: u64) {
    let alice_id = alice.status().endpoint_id.clone();
    let bob_id = bob.status().endpoint_id.clone();
    complete_transfer(alice, bob, transfer_id);
    assert!(alice.request_saved_device_pairing(bob_id.clone()).unwrap());
    wait_for_relationship(bob, &alice_id, DeviceRelationshipState::PendingIncoming);
    assert!(bob
        .respond_to_device_pairing(alice_id.clone(), true)
        .unwrap());
    wait_for_relationship(alice, &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(bob, &alice_id, DeviceRelationshipState::Saved);
}

fn wait_for_pending_offer(core: &VnidropCore) -> crate::PendingTargetedOffer {
    let started = Instant::now();
    loop {
        let pending = core.list_pending_targeted_offers();
        if let Some(offer) = pending.into_iter().next() {
            return offer;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "timed out waiting for pending targeted offer"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn targeted_source(path: &Path) -> ShareSource {
    ShareSource {
        kind: SourceKind::Path,
        value: path.to_string_lossy().into_owned(),
        display_name: Some(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ),
        is_directory: false,
    }
}

fn approve_one(
    alice: &Arc<VnidropCore>,
    bob: &Arc<VnidropCore>,
    payload: &[u8],
    name: &str,
) -> crate::TargetedTransfer {
    let bob_id = bob.status().endpoint_id.clone();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join(name);
    std::fs::write(&source_path, payload).unwrap();

    let bob_core = bob.clone();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, true)
            .unwrap()
    });
    let transfer = alice
        .create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some(name.to_string()),
        )
        .unwrap();
    let response = accept.join().unwrap();
    assert!(matches!(
        response,
        crate::TargetedOfferResponse::Approved { .. }
    ));
    transfer
}

fn recover_authoritative_state(
    live_events: &[CoreEvent],
    replayed: &[CoreEvent],
) -> (HashSet<String>, u64) {
    let mut seen_ids = HashSet::new();
    let mut max_revision = 0u64;
    for event in live_events.iter().chain(replayed.iter()) {
        if !seen_ids.insert(event.id.clone()) {
            continue;
        }
        max_revision = max_revision.max(event.revision);
    }
    (seen_ids, max_revision)
}

#[test]
fn secret_service_identity_survives_core_restart() {
    let node = SecretServiceNode::new();
    let endpoint_id = node.core().status().endpoint_id.clone();
    assert!(!endpoint_id.is_empty());
    assert!(
        !node.data_dir.path().join("iroh.secret").exists(),
        "protected identity must not fall back to plaintext"
    );

    let node = node.restart();
    assert_eq!(node.core().status().endpoint_id, endpoint_id);
    assert!(!node.data_dir.path().join("iroh.secret").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn secret_service_identity_survives_core_restart_on_linux() {
    // Regression: protected init used to call blocking Secret Service on the
    // Tokio worker that drives `CoreInner::start`, which nested `block_on` and
    // aborted desktop startup with "Cannot start a runtime from within a runtime".
    let data_dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let core = VnidropCore::initialize_with_limits_and_network_config(
        data_dir.path().to_string_lossy().into_owned(),
        sink,
        CoreLimits::default(),
        CoreNetworkConfig::default(),
    )
    .expect("Linux Secret Service core");
    let endpoint_id = core.status().endpoint_id.clone();
    assert!(!endpoint_id.is_empty());
    assert!(!data_dir.path().join("iroh.secret").exists());
    core.shutdown();
    drop(core);

    let path = data_dir.path().to_string_lossy().into_owned();
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let started = Instant::now();
    let restarted = loop {
        match VnidropCore::initialize_with_limits_and_network_config(
            path.clone(),
            sink.clone(),
            CoreLimits::default(),
            CoreNetworkConfig::default(),
        ) {
            Ok(core) => break core,
            Err(VnidropError::SecureStorageUnavailable { .. })
                if started.elapsed() < Duration::from_secs(2) =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("restart Linux core: {error:?}"),
        }
    };
    assert_eq!(restarted.status().endpoint_id, endpoint_id);
    restarted.shutdown();
    cleanup_scoped_secret_service(data_dir.path());
}

#[cfg(target_os = "linux")]
fn cleanup_scoped_secret_service(app_data_dir: &Path) {
    let profile = blake3::hash(app_data_dir.to_string_lossy().as_bytes()).to_hex();
    let prefix = format!("vnidrop/v1/scope-{profile}/");
    let Ok(store) = LinuxSecretServiceStore::connect() else {
        return;
    };
    let Ok(handles) = store.list_handles() else {
        return;
    };
    for handle in handles {
        if handle.as_str().starts_with(&prefix) {
            let _ = store.delete(&handle);
        }
    }
}

#[test]
fn public_api_contract_eligibility_through_unblock_on_linux_path() {
    // Full lifecycle uses the public UniFFI surface Linux/desktop bindings expose.
    // FaultInjecting backs custody so the harness stays deterministic; Secret
    // Service restart + binding hygiene cover the real Linux adapter separately.
    let alice = FaultNode::new();
    let bob = FaultNode::new();
    let alice_id = alice.core().status().endpoint_id.clone();
    let bob_id = bob.core().status().endpoint_id.clone();

    establish_saved(&alice.core(), &bob.core(), 17_001);

    alice
        .core()
        .set_saved_device_label(bob_id.clone(), Some("Bob Linux".to_string()))
        .unwrap();
    let saved = alice.core().list_saved_devices().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].endpoint_id, bob_id);
    assert_eq!(saved[0].local_label.as_deref(), Some("Bob Linux"));

    let transfer = approve_one(&alice.core(), &bob.core(), b"linux contract", "a.txt");
    assert_eq!(transfer.receiver_endpoint_id, bob_id);

    let alice = alice.restart();
    let bob = bob.restart();
    let resumed_output = tempfile::tempdir().unwrap();
    bob.core()
        .resume_targeted_transfer(
            transfer.id.clone(),
            resumed_output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(resumed_output.path().join("a.txt")).unwrap(),
        b"linux contract"
    );
    let completed = bob
        .core()
        .get_targeted_transfer(transfer.id)
        .unwrap()
        .unwrap();
    assert_eq!(completed.state, TargetedTransferState::Completed);

    alice.core().forget_saved_device(bob_id.clone()).unwrap();
    assert!(alice.core().list_saved_devices().unwrap().is_empty());

    bob.core().block_device(alice_id.clone()).unwrap();
    assert!(bob
        .core()
        .list_blocked_devices()
        .unwrap()
        .contains(&alice_id));
    bob.core().unblock_device(alice_id.clone()).unwrap();
    assert!(!bob
        .core()
        .list_blocked_devices()
        .unwrap()
        .contains(&alice_id));
    assert!(
        bob.core().list_saved_devices().unwrap().is_empty()
            || bob
                .core()
                .list_saved_devices()
                .unwrap()
                .iter()
                .all(|device| device.endpoint_id != alice_id),
        "unblock must not restore a forgotten/revoked relationship"
    );
}

#[test]
fn locked_or_absent_identity_fails_closed_while_missing_relationship_secrets_keep_networking() {
    let alice = FaultNode::new();
    let bob = FaultNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    establish_saved(&alice.core(), &bob.core(), 17_010);
    let endpoint_before = alice.core().status().endpoint_id.clone();

    // Drop only relationship-grant material; identity stays loadable.
    let handles = alice.secret_store.list_handles().unwrap();
    for handle in handles {
        if handle.as_str().contains("relationship-grant") {
            alice.secret_store.remove_for_test(&handle);
        }
    }
    let alice = alice.restart();
    assert_eq!(alice.core().status().endpoint_id, endpoint_before);
    assert!(
        alice.core().list_saved_devices().unwrap().is_empty(),
        "missing relationship secrets must disable saved-device rows"
    );
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("still-works.txt");
    std::fs::write(&source_path, b"identity ok").unwrap();
    let share = share_path(&alice.core(), &source_path, 17_011);
    assert!(
        !share.ticket.is_empty(),
        "identity must still serve tickets"
    );

    let create_err = alice.core().create_targeted_transfer(
        bob_id,
        vec![targeted_source(&source_path)],
        Some("still-works.txt".to_string()),
    );
    assert!(
        create_err.is_err(),
        "saved-device transfer must fail without relationship secrets"
    );

    // Locked identity storage refuses networking on the next start.
    alice
        .secret_store
        .fail_with(Some(ReferenceStoreFailure::Locked));
    match alice.try_restart() {
        Err((_alice, error)) => {
            assert!(matches!(error, VnidropError::SecureStorageLocked { .. }));
        }
        Ok(_) => panic!("locked identity must fail closed"),
    }

    // Absent identity (store unavailable for every secret) also refuses start.
    let missing = SecretServiceNode::new();
    missing.api.fail_all(Some(SecureSecretStoreError::Missing));
    match missing.try_restart() {
        Err((_node, error)) => {
            assert!(matches!(
                error,
                VnidropError::SecureStorageMissing { .. }
                    | VnidropError::SecureStorageUnavailable { .. }
                    | VnidropError::SecureStorageCorrupted { .. }
            ));
        }
        Ok(_) => panic!("absent Secret Service identity must fail closed"),
    }

    // Secret Service adapter maps lock / unavailable the same way for identity gets.
    let api = Arc::new(ControllableSecretService::default());
    let store = LinuxSecretServiceStore::with_api(api.clone());
    let identity = crate::secure_secret::secret_handle_for_test(
        "vnidrop/v1/endpoint-identity/linux-contract-lock".to_string(),
    );
    store
        .put(&identity, SecretMaterial::new(vec![0x41; 32]).unwrap())
        .unwrap();
    api.lock_handle(identity.as_str());
    assert!(matches!(
        store.get(&identity),
        Err(SecureSecretStoreError::Locked)
    ));
    api.fail_all(Some(SecureSecretStoreError::Unavailable));
    assert!(matches!(
        store.get(&identity),
        Err(SecureSecretStoreError::Unavailable)
    ));
}

#[test]
fn event_ids_and_revisions_recover_authoritative_state_after_listener_restart() {
    let alice = FaultNode::new();
    let bob = FaultNode::new();
    establish_saved(&alice.core(), &bob.core(), 17_020);

    let live = alice.sink.snapshot();
    assert!(!live.is_empty());
    let mut revisions = live.iter().map(|event| event.revision).collect::<Vec<_>>();
    revisions.sort_unstable();
    let unique = revisions.iter().copied().collect::<HashSet<_>>();
    assert_eq!(
        unique.len(),
        live.len(),
        "live revisions must be unique and monotonic per emission"
    );

    // Simulate at-least-once delivery: duplicates + a fresh listener.
    let duplicates = live.clone();
    alice.sink.clear();
    let alice = alice.restart();
    let after_restart = alice.core().list_events(None).unwrap();
    assert!(!after_restart.is_empty());

    let (seen_ids, max_revision) = recover_authoritative_state(&after_restart, &duplicates);
    assert_eq!(seen_ids.len(), after_restart.len());
    assert!(max_revision >= 1);

    let saved = alice.core().list_saved_devices().unwrap();
    assert_eq!(saved.len(), 1);
    let relationships = alice.core().list_device_relationships().unwrap();
    assert!(relationships
        .iter()
        .any(|entry| entry.state == DeviceRelationshipState::Saved));
}

#[test]
fn linux_public_bindings_omit_raw_secrets_and_generic_mutation() {
    let api = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/api.rs"),
    )
    .expect("api.rs");
    let facade = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime/facade.rs"),
    )
    .expect("facade.rs");
    // Test-only injectors may name SecureSecretStore; strip that impl for hygiene.
    let public_facade = strip_cfg_test_impls(&facade);

    for (label, source) in [
        ("api.rs", api.as_str()),
        ("facade.rs", public_facade.as_str()),
    ] {
        for forbidden in [
            "SecretMaterial",
            "SecretHandle",
            "SecureSecretStore",
            "executeSql",
            "executeSQL",
            "mutateState",
            "applyRawState",
            "rawSecret",
            "grantSecret",
            "pairingCapabilityBytes",
            "iroh.secret",
        ] {
            assert!(
                !source.contains(forbidden),
                "{label} must not expose {forbidden} on the public surface"
            );
        }
    }
    assert!(
        public_facade.contains("initialize_with_limits_and_network_config"),
        "facade must expose standard protected initialization"
    );
    assert!(
        !public_facade.contains("fn set_state(") && !public_facade.contains("fn mutate_state("),
        "facade must not expose a generic state-mutation escape hatch"
    );
    assert!(api.contains("revision"), "CoreEvent must carry revision");
}

fn strip_cfg_test_impls(source: &str) -> String {
    let mut out = String::new();
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            let mut brace_depth = 0i32;
            let mut seen_brace = false;
            for next in lines.by_ref() {
                brace_depth += next.chars().filter(|c| *c == '{').count() as i32;
                brace_depth -= next.chars().filter(|c| *c == '}').count() as i32;
                if next.contains('{') {
                    seen_brace = true;
                }
                if seen_brace && brace_depth <= 0 {
                    break;
                }
                if !seen_brace && next.trim_end().ends_with(';') {
                    break;
                }
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
