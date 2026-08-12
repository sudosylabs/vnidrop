//! Windows saved-device core contract harness.
//!
//! Compiles on every host. Uses [`FakeWindowsDpapiApi`] as an injectable
//! current-user DPAPI stand-in; real DPAPI is exercised under `cfg(windows)`.

use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    secure_secret::{
        scope_store,
        windows::{FakeWindowsDpapiApi, WindowsDpapiSecretStore},
        FaultInjectingSecretStore, ReferenceStoreFailure, SecretMaterial, SecureSecretStore,
        SecureSecretStoreError,
    },
    CoreEvent, CoreEventSink, DeviceRelationshipState, ShareMetadataInput, ShareSource, SourceKind,
    TargetedTransferState, TransferAccessMode, VnidropCore, VnidropError,
};
use data_encoding::HEXLOWER;
use iroh::SecretKey;

#[cfg(target_os = "windows")]
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
    fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().unwrap().clone()
    }
}

struct WindowsContractNode {
    data_dir: tempfile::TempDir,
    api: Arc<FakeWindowsDpapiApi>,
    sink: Arc<RecordingSink>,
    core: Option<Arc<VnidropCore>>,
}

impl WindowsContractNode {
    fn new_with_legacy(identity: &SecretKey) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            data_dir.path().join("iroh.secret"),
            HEXLOWER.encode(&identity.to_bytes()),
        )
        .unwrap();
        let api = Arc::new(FakeWindowsDpapiApi::new());
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = windows_scoped_store(data_dir.path(), api.clone());
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store,
        )
        .expect("Windows legacy migration");
        Self {
            data_dir,
            api,
            sink,
            core: Some(core),
        }
    }
    fn new() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let api = Arc::new(FakeWindowsDpapiApi::new());
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = windows_scoped_store(data_dir.path(), api.clone());
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store,
        )
        .expect("windows contract core");
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

    fn restart(&mut self) -> Arc<VnidropCore> {
        if let Some(core) = self.core.take() {
            core.shutdown();
            drop(core);
        }
        self.sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = windows_scoped_store(self.data_dir.path(), self.api.clone());
        let core = VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            self.sink.clone(),
            store,
        )
        .expect("restarted windows contract core");
        self.core = Some(core.clone());
        core
    }
}

#[test]
fn windows_adapter_protects_identity_before_plaintext_removal() {
    let identity = SecretKey::generate();
    let mut node = WindowsContractNode::new_with_legacy(&identity);
    assert!(!node.data_dir.path().join("iroh.secret").exists());
    let endpoint = node.core().status().endpoint_id;
    assert_eq!(endpoint, identity.public().to_string());
    let restarted = node.restart();
    assert_eq!(restarted.status().endpoint_id, endpoint);
}

impl Drop for WindowsContractNode {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

fn windows_scoped_store(
    app_data_dir: &Path,
    api: Arc<FakeWindowsDpapiApi>,
) -> Arc<dyn SecureSecretStore> {
    let store = WindowsDpapiSecretStore::with_api(app_data_dir.join("protected-secrets-v1"), api)
        .expect("windows dpapi store");
    scope_store(app_data_dir, Arc::new(store))
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

fn complete_transfer(
    sender: &WindowsContractNode,
    receiver: &WindowsContractNode,
    transfer_id: u64,
) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"windows contract").unwrap();
    let share = share_path(&sender.core(), &source_path, transfer_id);
    let output = output_dir.path().to_string_lossy().to_string();
    let receiver_core = receiver.core();
    let ticket = share.ticket.clone();
    let handle = std::thread::spawn(move || {
        receiver_core.receive(ticket, output, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(&sender.core(), share.transfer_id);
    sender
        .core()
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();

    let started = Instant::now();
    let peer = receiver.core().status().endpoint_id.clone();
    loop {
        if sender
            .core()
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

fn reach_saved(alice: &WindowsContractNode, bob: &WindowsContractNode, transfer_id: u64) {
    let alice_id = alice.core().status().endpoint_id.clone();
    let bob_id = bob.core().status().endpoint_id.clone();
    complete_transfer(alice, bob, transfer_id);
    assert!(alice
        .core()
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());
    wait_for_relationship(
        &bob.core(),
        &alice_id,
        DeviceRelationshipState::PendingIncoming,
    );
    assert!(bob
        .core()
        .respond_to_device_pairing(alice_id, true)
        .unwrap());
    wait_for_relationship(&alice.core(), &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(
        &bob.core(),
        &alice.core().status().endpoint_id,
        DeviceRelationshipState::Saved,
    );
}

fn wait_for_pending_offer(core: &VnidropCore) -> crate::PendingTargetedOffer {
    let started = Instant::now();
    loop {
        if let Some(offer) = core.list_pending_targeted_offers().into_iter().next() {
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

#[test]
fn windows_dpapi_identity_survives_core_restart() {
    let mut node = WindowsContractNode::new();
    let first = node.core().status().endpoint_id.clone();
    assert!(!first.is_empty());
    let restarted = node.restart();
    assert_eq!(restarted.status().endpoint_id, first);
}

#[cfg(target_os = "windows")]
#[test]
fn real_windows_dpapi_init_preserves_identity() {
    let data_dir = tempfile::tempdir().unwrap();
    let path = data_dir.path().to_string_lossy().into_owned();
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let core = VnidropCore::initialize_with_limits_and_network_config(
        path.clone(),
        sink,
        CoreLimits::default(),
        CoreNetworkConfig::default(),
    )
    .expect("Windows core");
    let first = core.status().endpoint_id.clone();
    core.shutdown();
    drop(core);

    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let restarted = VnidropCore::initialize_with_limits_and_network_config(
        path,
        sink,
        CoreLimits::default(),
        CoreNetworkConfig::default(),
    )
    .expect("restarted Windows core");
    assert_eq!(restarted.status().endpoint_id, first);
    restarted.shutdown();
}

#[test]
fn public_api_exercises_complete_windows_saved_device_contract() {
    let mut alice = WindowsContractNode::new();
    let mut bob = WindowsContractNode::new();
    let alice_id = alice.core().status().endpoint_id.clone();
    let bob_id = bob.core().status().endpoint_id.clone();

    complete_transfer(&alice, &bob, 16_001);
    assert!(alice
        .core()
        .list_pairing_eligibilities()
        .unwrap()
        .iter()
        .any(|entry| entry.peer_endpoint_id == bob_id));

    assert!(alice
        .core()
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());
    wait_for_relationship(
        &bob.core(),
        &alice_id,
        DeviceRelationshipState::PendingIncoming,
    );
    assert!(bob
        .core()
        .respond_to_device_pairing(alice_id.clone(), true)
        .unwrap());
    wait_for_relationship(&alice.core(), &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(&bob.core(), &alice_id, DeviceRelationshipState::Saved);

    alice
        .core()
        .set_saved_device_label(bob_id.clone(), Some("Bob PC".to_string()))
        .unwrap();
    let listed = alice.core().list_saved_devices().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].endpoint_id, bob_id);
    assert_eq!(listed[0].local_label.as_deref(), Some("Bob PC"));

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"windows targeted payload").unwrap();

    let bob_core = bob.core();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, true)
            .unwrap()
    });
    let transfer = alice
        .core()
        .create_targeted_transfer(
            bob_id.clone(),
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap();
    let response = accept.join().unwrap();
    assert!(matches!(
        response,
        crate::TargetedOfferResponse::Approved { .. }
    ));
    assert_eq!(transfer.state, TargetedTransferState::Approved);

    // Interrupt via receiver restart, then resume without re-approval.
    let bob_core = bob.restart();
    let interrupted = bob_core
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .expect("durable transfer");
    assert!(matches!(
        interrupted.state,
        TargetedTransferState::Approved | TargetedTransferState::Interrupted
    ));
    let output = tempfile::tempdir().unwrap();
    bob_core
        .resume_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        b"windows targeted payload"
    );

    alice.core().forget_saved_device(bob_id.clone()).unwrap();
    assert!(alice.core().list_saved_devices().unwrap().is_empty());

    bob_core.block_device(alice_id.clone()).unwrap();
    assert_eq!(
        bob_core.list_blocked_devices().unwrap(),
        vec![alice_id.clone()]
    );
    bob_core.unblock_device(alice_id).unwrap();
    assert!(bob_core.list_blocked_devices().unwrap().is_empty());
    // Unblock does not restore forgotten relationships.
    assert!(alice.core().list_saved_devices().unwrap().is_empty());

    let _ = alice.restart();
}

#[test]
fn wrong_user_or_unavailable_identity_prevents_networking() {
    let data_dir = tempfile::tempdir().unwrap();
    let first_api = Arc::new(FakeWindowsDpapiApi::with_context(b"windows-user-a"));
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let store = windows_scoped_store(data_dir.path(), first_api);
    let core = VnidropCore::initialize_with_test_secret_store(
        data_dir.path().to_string_lossy().into_owned(),
        sink,
        store,
    )
    .unwrap();
    let endpoint = core.status().endpoint_id.clone();
    core.shutdown();
    drop(core);

    let wrong_user = Arc::new(FakeWindowsDpapiApi::with_context(b"windows-user-b"));
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let store = windows_scoped_store(data_dir.path(), wrong_user);
    let err = match VnidropCore::initialize_with_test_secret_store(
        data_dir.path().to_string_lossy().into_owned(),
        sink,
        store,
    ) {
        Ok(_) => panic!("wrong-user DPAPI context must not start networking"),
        Err(error) => error,
    };
    assert!(
        matches!(
            err,
            VnidropError::SecureStorageCorrupted { .. }
                | VnidropError::SecureStorageUnavailable { .. }
                | VnidropError::SecureStorageLocked { .. }
                | VnidropError::SecureStorageMissing { .. }
                | VnidropError::Initialization { .. }
        ),
        "unexpected error for wrong-user identity: {err:?}"
    );
    assert!(!endpoint.is_empty());

    let unavailable = Arc::new(FaultInjectingSecretStore::default());
    unavailable.fail_with(Some(ReferenceStoreFailure::Unavailable));
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let err = match VnidropCore::initialize_with_test_secret_store(
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
        sink,
        unavailable,
    ) {
        Ok(_) => panic!("unavailable identity store must not start networking"),
        Err(error) => error,
    };
    assert!(matches!(
        err,
        VnidropError::SecureStorageUnavailable { .. } | VnidropError::Initialization { .. }
    ));
}

#[test]
fn unavailable_relationship_secrets_disable_only_saved_device_behavior() {
    let alice = WindowsContractNode::new();
    let bob = WindowsContractNode::new();
    reach_saved(&alice, &bob, 16_010);

    alice
        .api
        .set_unavailable_for_handles_containing("relationship-grant");
    bob.api
        .set_unavailable_for_handles_containing("relationship-grant");

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("invite.txt");
    std::fs::write(&source_path, b"invitation still works").unwrap();
    let share = share_path(&alice.core(), &source_path, 16_011);
    let output_dir = tempfile::tempdir().unwrap();
    let output = output_dir.path().to_string_lossy().to_string();
    let receiver = bob.core();
    let ticket = share.ticket.clone();
    let handle =
        std::thread::spawn(move || receiver.receive(ticket, output, Some("receiver".to_string())));
    let request = wait_for_receiver_request(&alice.core(), share.transfer_id);
    alice
        .core()
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();
    assert_eq!(
        std::fs::read(output_dir.path().join("hello.txt")).unwrap(),
        b"invitation still works"
    );

    let targeted = alice.core().create_targeted_transfer(
        bob.core().status().endpoint_id,
        vec![targeted_source(&source_path)],
        Some("invite.txt".to_string()),
    );
    assert!(
        targeted.is_err(),
        "saved-device targeted transfer must fail closed when relationship secrets are unavailable"
    );
    let err = targeted.unwrap_err();
    assert!(
        matches!(
            err,
            VnidropError::SecureStorageUnavailable { .. }
                | VnidropError::SecureStorageCorrupted { .. }
                | VnidropError::SecureStorageMissing { .. }
                | VnidropError::SecureStorageLocked { .. }
                | VnidropError::Permission { .. }
                | VnidropError::Network { .. }
        ),
        "unexpected targeted failure: {err:?}"
    );
}

#[test]
fn event_ids_and_revisions_recover_authoritative_state_after_listener_restart() {
    let mut alice = WindowsContractNode::new();
    let bob = WindowsContractNode::new();
    reach_saved(&alice, &bob, 16_020);

    let live = alice.sink.events();
    assert!(!live.is_empty());
    let mut seen = HashSet::new();
    let mut revisions = Vec::new();
    for event in &live {
        assert!(seen.insert(event.id.clone()), "duplicate live event id");
        assert!(event.revision >= 1);
        revisions.push(event.revision);
    }
    revisions.sort_unstable();
    let unique = revisions.len();
    revisions.dedup();
    assert_eq!(revisions.len(), unique, "live revisions must be unique");

    // Duplicate delivery: same events observed twice must still dedupe by id.
    let mut merged = live.clone();
    merged.extend(live.iter().cloned());
    let mut deduped = HashSet::new();
    for event in &merged {
        deduped.insert((event.id.clone(), event.revision));
    }
    assert_eq!(deduped.len(), live.len());

    let before_restart = alice.core().list_events(None).unwrap();
    alice.restart();
    let authoritative = alice.core().list_events(None).unwrap();
    assert!(!authoritative.is_empty());
    assert!(
        authoritative.len() >= before_restart.len().saturating_sub(8),
        "restart must retain durable events for recovery"
    );
    let devices = alice.core().list_saved_devices().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].endpoint_id, bob.core().status().endpoint_id);
}

#[test]
fn public_bindings_omit_raw_secrets_and_generic_mutation() {
    // UniFFI only exports the typed public surface from api.rs / VnidropCore.
    // Secret custody types and test-only mutation helpers must stay crate-private.
    let exported = include_str!("../lib.rs");
    assert!(
        !exported.contains("SecretMaterial") && !exported.contains("SecretHandle"),
        "raw secret types must not be re-exported from the crate root"
    );
    assert!(
        !exported.contains("SecureSecretStore"),
        "secure secret store must not cross the public binding boundary"
    );

    let facade = include_str!("../runtime/facade.rs");
    for needle in [
        "fn execute_sql",
        "fn mutate_state",
        "fn put_secret",
        "fn load_secret",
        "SecretMaterial",
        "SecretHandle",
    ] {
        assert!(
            !facade.contains(needle),
            "public facade must not expose generic mutation / raw secrets ({needle})"
        );
    }

    // for_test helpers are cfg(test) only and never part of UniFFI export.
    assert!(facade.contains("cfg(test)"));
    assert!(facade.contains("for_test"));

    let api = include_str!("../api.rs");
    assert!(api.contains("struct SavedDevice"));
    assert!(api.contains("struct PairingEligibilitySummary"));
    assert!(
        !api.contains("grant_bytes")
            && !api.contains("private_key")
            && !api.contains("secret_material"),
        "public API records must not carry raw secret fields"
    );
}

#[test]
fn fake_windows_dpapi_wrong_context_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let handle = WindowsDpapiSecretStore::relationship_handle_for_test();
    let material = SecretMaterial::new(vec![0xa1; 32]).unwrap();
    let original = WindowsDpapiSecretStore::with_api(
        directory.path(),
        Arc::new(FakeWindowsDpapiApi::with_context(b"user-a")),
    )
    .unwrap();
    original.put(&handle, material).unwrap();

    let wrong = WindowsDpapiSecretStore::with_api(
        directory.path(),
        Arc::new(FakeWindowsDpapiApi::with_context(b"user-b")),
    )
    .unwrap();
    assert!(matches!(
        wrong.get(&handle),
        Err(SecureSecretStoreError::Corrupted)
    ));
}
