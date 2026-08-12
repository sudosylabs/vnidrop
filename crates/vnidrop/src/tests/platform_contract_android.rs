//! Android platform contract for the experimental saved-device foundation.
//!
//! These tests drive the public UniFFI surface through an
//! [`AndroidSecureSecretStore`] backed by an in-process Keystore fake so the
//! contract can run on host CI without a device or product UI.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use tempfile::TempDir;

use crate::{
    secure_secret::{
        android::{AndroidKeystore, AndroidSealedValue, AndroidSecureSecretStore},
        SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError,
    },
    CoreEvent, CoreEventSink, DeviceRelationshipState, PublishedOutput, ReceiveOutputSinkV2,
    ReceivedLocatorKind, ShareMetadataInput, ShareSource, SourceKind, TargetedTransferState,
    TransferAccessMode, VnidropCore, VnidropError,
};

#[derive(Default)]
struct FakeAndroidKeystore {
    keys: Mutex<HashMap<String, u8>>,
    locked: AtomicBool,
}

impl FakeAndroidKeystore {
    fn lock(&self) {
        self.locked.store(true, Ordering::SeqCst);
    }
}

impl AndroidKeystore for FakeAndroidKeystore {
    fn seal(
        &self,
        alias: &str,
        plaintext: &[u8],
    ) -> Result<AndroidSealedValue, SecureSecretStoreError> {
        if self.locked.load(Ordering::SeqCst) {
            return Err(SecureSecretStoreError::Locked);
        }
        let mask = 0xb3;
        self.keys.lock().unwrap().insert(alias.to_string(), mask);
        Ok(AndroidSealedValue {
            nonce: vec![9; 12],
            ciphertext: plaintext.iter().map(|byte| byte ^ mask).collect(),
        })
    }

    fn open(
        &self,
        alias: &str,
        sealed: &AndroidSealedValue,
    ) -> Result<Vec<u8>, SecureSecretStoreError> {
        if self.locked.load(Ordering::SeqCst) {
            return Err(SecureSecretStoreError::Locked);
        }
        let mask = *self
            .keys
            .lock()
            .unwrap()
            .get(alias)
            .ok_or(SecureSecretStoreError::Missing)?;
        Ok(sealed.ciphertext.iter().map(|byte| byte ^ mask).collect())
    }

    fn delete(&self, alias: &str) -> Result<(), SecureSecretStoreError> {
        if self.locked.load(Ordering::SeqCst) {
            return Err(SecureSecretStoreError::Locked);
        }
        self.keys.lock().unwrap().remove(alias);
        Ok(())
    }
}

/// Fails closed for relationship/targeted secrets while leaving identity usable.
struct RelationshipGatedStore {
    inner: Arc<dyn SecureSecretStore>,
    gate_relationship_secrets: AtomicBool,
}

impl RelationshipGatedStore {
    fn new(inner: Arc<dyn SecureSecretStore>) -> Self {
        Self {
            inner,
            gate_relationship_secrets: AtomicBool::new(false),
        }
    }

    fn disable_relationship_secrets(&self) {
        self.gate_relationship_secrets.store(true, Ordering::SeqCst);
    }

    fn check(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        if !self.gate_relationship_secrets.load(Ordering::SeqCst) {
            return Ok(());
        }
        let value = handle.as_str();
        if value.contains("/relationship-grant/") || value.contains("/targeted-authorization/") {
            return Err(SecureSecretStoreError::Unavailable);
        }
        Ok(())
    }
}

impl SecureSecretStore for RelationshipGatedStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        self.check(handle)?;
        self.inner.put(handle, material)
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        self.check(handle)?;
        self.inner.get(handle)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        self.check(handle)?;
        self.inner.delete(handle)
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        // Listing remains available so identity restart and orphan discovery work.
        self.inner.list_handles()
    }
}

struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl CoreEventSink for RecordingSink {
    fn on_event(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct AndroidContractNode {
    _no_backup: TempDir,
    data_dir: TempDir,
    keystore: Arc<FakeAndroidKeystore>,
    store: Arc<RelationshipGatedStore>,
    sink: Arc<RecordingSink>,
    core: Option<Arc<VnidropCore>>,
}

impl AndroidContractNode {
    fn new() -> Self {
        let no_backup = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();
        let keystore = Arc::new(FakeAndroidKeystore::default());
        let android_store =
            AndroidSecureSecretStore::new(no_backup.path(), keystore.clone()).unwrap();
        let store = Arc::new(RelationshipGatedStore::new(Arc::new(android_store)));
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store.clone(),
        )
        .expect("android-backed protected core");
        Self {
            _no_backup: no_backup,
            data_dir,
            keystore,
            store,
            sink,
            core: Some(core),
        }
    }

    fn core(&self) -> Arc<VnidropCore> {
        self.core.as_ref().expect("core alive").clone()
    }

    fn restart_with_sink(&mut self, sink: Arc<RecordingSink>) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        let core = VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            self.store.clone(),
        )
        .expect("restarted android-backed core");
        self.sink = sink;
        self.core = Some(core);
    }

    fn restart(&mut self) {
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        self.restart_with_sink(sink);
    }

    fn try_restart(&mut self) -> Result<(), VnidropError> {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        match VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            self.store.clone(),
        ) {
            Ok(core) => {
                self.sink = sink;
                self.core = Some(core);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

impl Drop for AndroidContractNode {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

fn share_path(core: &VnidropCore, source: &Path, transfer_id: u64) -> crate::ShareResult {
    core.share_files(
        vec![ShareSource {
            kind: SourceKind::Path,
            value: source.to_string_lossy().into_owned(),
            display_name: Some(
                source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            is_directory: false,
        }],
        ShareMetadataInput {
            transfer_id,
            transfer_name: Some(
                source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
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

fn complete_invitation_transfer(
    sender: &AndroidContractNode,
    receiver: &AndroidContractNode,
    transfer_id: u64,
    payload: &[u8],
) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, payload).unwrap();
    let share = share_path(&sender.core(), &source_path, transfer_id);
    let output_dir = output_dir.path().to_string_lossy().to_string();
    let receiver_core = receiver.core().clone();
    let ticket = share.ticket.clone();
    let handle = std::thread::spawn(move || {
        receiver_core.receive(ticket, output_dir, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(&sender.core(), share.transfer_id);
    sender
        .core()
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();

    if sender
        .core()
        .list_saved_devices()
        .unwrap()
        .iter()
        .any(|device| device.endpoint_id == receiver.core().status().endpoint_id)
    {
        return;
    }
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

fn establish_saved(alice: &AndroidContractNode, bob: &AndroidContractNode, transfer_id: u64) {
    let alice_id = alice.core().status().endpoint_id.clone();
    let bob_id = bob.core().status().endpoint_id.clone();
    complete_invitation_transfer(alice, bob, transfer_id, b"android contract consent");
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

fn approve_targeted(
    alice: &AndroidContractNode,
    bob: &AndroidContractNode,
    payload: &[u8],
    name: &str,
) -> crate::TargetedTransfer {
    let bob_id = bob.core().status().endpoint_id.clone();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join(name);
    std::fs::write(&source_path, payload).unwrap();

    let bob_core = bob.core().clone();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, true)
            .unwrap()
    });
    let transfer = alice
        .core()
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

#[test]
fn android_keystore_backed_identity_survives_core_restart() {
    let mut node = AndroidContractNode::new();
    let endpoint_id = node.core().status().endpoint_id.clone();
    assert!(!endpoint_id.is_empty());

    node.restart();
    assert_eq!(node.core().status().endpoint_id, endpoint_id);
}

#[test]
fn android_public_api_covers_saved_device_and_targeted_lifecycle() {
    let mut alice = AndroidContractNode::new();
    let mut bob = AndroidContractNode::new();
    let alice_id = alice.core().status().endpoint_id.clone();
    let bob_id = bob.core().status().endpoint_id.clone();

    establish_saved(&alice, &bob, 15_001);

    alice
        .core()
        .set_saved_device_label(bob_id.clone(), Some("Kitchen Tablet".to_string()))
        .unwrap();
    let saved = alice.core().list_saved_devices().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].endpoint_id, bob_id);
    assert_eq!(saved[0].local_label.as_deref(), Some("Kitchen Tablet"));

    let transfer = approve_targeted(&alice, &bob, b"android payload", "payload.txt");
    assert_eq!(transfer.receiver_endpoint_id, bob_id);
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .expect("receiver durable row")
            .state,
        TargetedTransferState::Approved
    );

    alice.restart();
    bob.restart();
    let output = tempfile::tempdir().unwrap();
    bob.core()
        .resume_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        b"android payload"
    );

    alice.core().forget_saved_device(bob_id.clone()).unwrap();
    assert!(alice.core().list_saved_devices().unwrap().is_empty());
    assert!(alice.core().list_device_relationships().unwrap().is_empty());

    bob.core().block_device(alice_id.clone()).unwrap();
    assert_eq!(
        bob.core().list_blocked_devices().unwrap(),
        vec![alice_id.clone()]
    );
    bob.core().unblock_device(alice_id).unwrap();
    assert!(bob.core().list_blocked_devices().unwrap().is_empty());
    // Alice already forgot; unblock on Bob must not restore Alice's local saved list.
    assert!(alice.core().list_saved_devices().unwrap().is_empty());
}

/// Host-side stand-in for MediaStore Downloads publish: durable Android locator, not a path dir.
#[derive(Default)]
struct AndroidMediaStoreSink {
    files: Mutex<HashMap<String, Vec<u8>>>,
    published: Mutex<HashMap<String, PublishedOutput>>,
}

impl AndroidMediaStoreSink {
    fn bytes(&self, relative_path: &str) -> Vec<u8> {
        self.files.lock().unwrap()[relative_path].clone()
    }

    fn published(&self, relative_path: &str) -> PublishedOutput {
        self.published.lock().unwrap()[relative_path].clone()
    }
}

impl ReceiveOutputSinkV2 for AndroidMediaStoreSink {
    fn start_file(&self, relative_path: String) -> Result<(), VnidropError> {
        self.files.lock().unwrap().insert(relative_path, Vec::new());
        Ok(())
    }

    fn write_chunk(&self, relative_path: String, bytes: Vec<u8>) -> Result<(), VnidropError> {
        self.files
            .lock()
            .unwrap()
            .get_mut(&relative_path)
            .expect("started")
            .extend(bytes);
        Ok(())
    }

    fn finish_file(&self, relative_path: String) -> Result<PublishedOutput, VnidropError> {
        let published = PublishedOutput {
            locator_kind: ReceivedLocatorKind::AndroidMediaStore,
            locator: format!("content://media/external/downloads/{relative_path}"),
        };
        self.published
            .lock()
            .unwrap()
            .insert(relative_path, published.clone());
        Ok(published)
    }

    fn abort_file(&self, relative_path: String, _reason: String) -> Result<(), VnidropError> {
        self.files.lock().unwrap().remove(&relative_path);
        self.published.lock().unwrap().remove(&relative_path);
        Ok(())
    }
}

#[test]
fn android_targeted_receive_and_resume_via_media_store_sink() {
    let alice = AndroidContractNode::new();
    let mut bob = AndroidContractNode::new();
    establish_saved(&alice, &bob, 15_101);

    let transfer = approve_targeted(&alice, &bob, b"media store payload", "download.txt");
    let sink = Arc::new(AndroidMediaStoreSink::default());
    bob.core()
        .receive_targeted_transfer_with_output_sink_v2(transfer.id.clone(), sink.clone())
        .unwrap();
    assert_eq!(sink.bytes("download.txt"), b"media store payload");
    let published = sink.published("download.txt");
    assert_eq!(
        published.locator_kind,
        ReceivedLocatorKind::AndroidMediaStore
    );
    assert!(published.locator.starts_with("content://media/"));
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );

    let transfer2 = approve_targeted(&alice, &bob, b"resume via sink", "resume.txt");
    bob.restart();
    let resume_sink = Arc::new(AndroidMediaStoreSink::default());
    bob.core()
        .resume_targeted_transfer_with_output_sink_v2(transfer2.id.clone(), resume_sink.clone())
        .unwrap();
    assert_eq!(resume_sink.bytes("resume.txt"), b"resume via sink");
    assert_eq!(
        resume_sink.published("resume.txt").locator_kind,
        ReceivedLocatorKind::AndroidMediaStore
    );
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer2.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );
}

#[test]
fn android_targeted_sink_failure_marks_transfer_interrupted() {
    let alice = AndroidContractNode::new();
    let bob = AndroidContractNode::new();
    establish_saved(&alice, &bob, 15_102);
    let transfer = approve_targeted(&alice, &bob, b"will fail", "fail.txt");

    struct FailingMediaStoreSink;
    impl ReceiveOutputSinkV2 for FailingMediaStoreSink {
        fn start_file(&self, _relative_path: String) -> Result<(), VnidropError> {
            Ok(())
        }
        fn write_chunk(&self, _relative_path: String, _bytes: Vec<u8>) -> Result<(), VnidropError> {
            Err(VnidropError::Filesystem {
                reason: "media store write failed".to_string(),
            })
        }
        fn finish_file(&self, _relative_path: String) -> Result<PublishedOutput, VnidropError> {
            unreachable!("write failed")
        }
        fn abort_file(&self, _relative_path: String, _reason: String) -> Result<(), VnidropError> {
            Ok(())
        }
    }

    let err = bob
        .core()
        .receive_targeted_transfer_with_output_sink_v2(
            transfer.id.clone(),
            Arc::new(FailingMediaStoreSink),
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            VnidropError::Transfer { .. } | VnidropError::Filesystem { .. }
        ),
        "expected transfer/filesystem failure, got {err:?}"
    );
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Interrupted
    );
}

#[test]
fn locked_or_invalidated_identity_blocks_restart() {
    let mut alice = AndroidContractNode::new();
    assert!(!alice.core().status().endpoint_id.is_empty());

    alice.keystore.lock();
    let locked = alice
        .try_restart()
        .expect_err("locked identity must fail closed");
    assert!(
        matches!(locked, VnidropError::SecureStorageLocked { .. }),
        "expected SecureStorageLocked, got {locked:?}"
    );

    // Invalidated Keystore material (keys wiped) is a distinct closed failure.
    let mut bob = AndroidContractNode::new();
    assert!(!bob.core().status().endpoint_id.is_empty());
    bob.keystore.keys.lock().unwrap().clear();
    let missing = bob
        .try_restart()
        .expect_err("invalidated identity must fail closed");
    assert!(
        matches!(
            missing,
            VnidropError::SecureStorageMissing { .. }
                | VnidropError::SecureStorageCorrupted { .. }
                | VnidropError::SecureStorageUnavailable { .. }
        ),
        "expected missing/corrupted/unavailable identity, got {missing:?}"
    );
}

#[test]
fn unavailable_relationship_secrets_disable_only_saved_device_paths() {
    let alice = AndroidContractNode::new();
    let bob = AndroidContractNode::new();
    establish_saved(&alice, &bob, 15_011);
    let bob_id = bob.core().status().endpoint_id.clone();
    let endpoint_before = alice.core().status().endpoint_id.clone();
    alice.store.disable_relationship_secrets();

    // Invitation transfers only need the already-loaded endpoint identity.
    complete_invitation_transfer(&alice, &bob, 15_012, b"invitation still works");
    assert_eq!(alice.core().status().endpoint_id, endpoint_before);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("x.txt");
    std::fs::write(&source_path, b"x").unwrap();
    let targeted = alice.core().create_targeted_transfer(
        bob_id,
        vec![targeted_source(&source_path)],
        Some("x.txt".to_string()),
    );
    assert!(
        targeted.is_err(),
        "targeted transfers require usable relationship secrets"
    );

    let stranger = AndroidContractNode::new();
    complete_invitation_transfer(&alice, &stranger, 15_013, b"new eligibility");
    let alice_id = alice.core().status().endpoint_id.clone();
    let stranger_id = stranger.core().status().endpoint_id.clone();
    // Eligibility may still start a pairing attempt; grant minting must fail closed.
    assert!(alice
        .core()
        .request_saved_device_pairing(stranger_id.clone())
        .unwrap());
    let accept = stranger
        .core()
        .respond_to_device_pairing(alice_id.clone(), true);
    assert!(
        matching_pairing_failure(&accept),
        "consent must not mint grants when relationship secrets are unavailable: {accept:?}"
    );
    assert!(
        alice
            .core()
            .list_saved_devices()
            .unwrap()
            .iter()
            .all(|device| device.endpoint_id != stranger_id),
        "pairing must not reach Saved without relationship secrets"
    );
    assert!(
        stranger
            .core()
            .list_saved_devices()
            .unwrap()
            .iter()
            .all(|device| device.endpoint_id != alice_id),
        "peer must not reach Saved without relationship secrets"
    );
}

fn matching_pairing_failure(result: &Result<bool, VnidropError>) -> bool {
    match result {
        Ok(false) => true,
        Err(VnidropError::SecureStorageUnavailable { .. })
        | Err(VnidropError::SecureStorageMissing { .. })
        | Err(VnidropError::SecureStorageLocked { .. })
        | Err(VnidropError::SecureStorageCorrupted { .. })
        | Err(VnidropError::Permission { .. })
        | Err(VnidropError::InvalidInput { .. })
        | Err(VnidropError::Internal { .. })
        | Err(VnidropError::Transfer { .. }) => true,
        Ok(true) => false,
        Err(_) => true,
    }
}

#[test]
fn event_ids_and_revisions_recover_authoritative_state_after_listener_restart() {
    let mut alice = AndroidContractNode::new();
    let bob = AndroidContractNode::new();
    establish_saved(&alice, &bob, 15_020);
    alice
        .core()
        .set_saved_device_label(
            bob.core().status().endpoint_id.clone(),
            Some("Desk Phone".to_string()),
        )
        .unwrap();

    let before = alice.core().list_events(None).unwrap();
    assert!(!before.is_empty());
    let mut seen = HashSet::new();
    let mut max_revision = 0_u64;
    for event in &before {
        assert!(
            seen.insert(event.id.clone()),
            "live event ids must be unique"
        );
        assert!(event.revision >= 1);
        max_revision = max_revision.max(event.revision);
    }

    // Simulate at-least-once delivery with duplicates, then drop the listener.
    let mut recovered_ids = HashSet::new();
    let mut recovered_revision = 0_u64;
    for event in before.iter().chain(before.iter()) {
        if recovered_ids.insert(event.id.clone()) {
            recovered_revision = recovered_revision.max(event.revision);
        }
    }
    assert_eq!(recovered_ids.len(), seen.len());
    assert_eq!(recovered_revision, max_revision);

    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    alice.restart_with_sink(sink.clone());

    let after = alice.core().list_events(None).unwrap();
    assert!(!after.is_empty());
    let durable_ids: HashSet<_> = after.iter().map(|event| event.id.clone()).collect();
    assert!(
        recovered_ids.is_subset(&durable_ids) || durable_ids.is_subset(&recovered_ids),
        "durable event history must remain reconcilable by stable id"
    );
    let durable_max = after.iter().map(|event| event.revision).max().unwrap_or(0);
    assert!(durable_max >= recovered_revision);

    let saved = alice.core().list_saved_devices().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].local_label.as_deref(), Some("Desk Phone"));
}

#[test]
fn android_public_surface_omits_raw_secrets_and_generic_mutation() {
    // Only the UniFFI-exported façade is binding-visible; cfg(test) helpers above it are not.
    let facade = include_str!("../runtime/facade.rs");
    let export_start = facade
        .find("#[uniffi::export]")
        .expect("UniFFI export block");
    let api = include_str!("../api.rs");
    for source in [&facade[export_start..], api] {
        for forbidden in [
            "SecretMaterial",
            "SecretHandle",
            "SecureSecretStore",
            "execute_sql",
            "mutate_state",
            "raw_secret",
            "set_raw_state",
            "iroh.secret",
        ] {
            assert!(
                !source.contains(forbidden),
                "public UniFFI modules must not expose {forbidden}"
            );
        }
    }

    let candidates = [
        Path::new(
            "shared/build/generated/uniffi/commonMain/kotlin/uniffi/vnidrop/vnidrop.common.kt",
        ),
        Path::new(
            "../shared/build/generated/uniffi/commonMain/kotlin/uniffi/vnidrop/vnidrop.common.kt",
        ),
        Path::new(
            "../../shared/build/generated/uniffi/commonMain/kotlin/uniffi/vnidrop/vnidrop.common.kt",
        ),
    ];
    if let Some(path) = candidates.iter().find(|path| path.exists()) {
        let kotlin = std::fs::read_to_string(path).unwrap();
        for forbidden in [
            "SecretMaterial",
            "SecretHandle",
            "SecureSecretStore",
            "executeSql",
            "mutateState",
            "rawSecret",
            "setRawState",
            "iroh.secret",
        ] {
            assert!(
                !kotlin.contains(forbidden),
                "generated Kotlin bindings at {} must not expose {forbidden}",
                path.display()
            );
        }
        assert!(
            kotlin.contains("initializeWithExperimentalSavedDevices"),
            "experimental Android init must remain on the public binding surface"
        );
        assert!(
            kotlin.contains("SavedDevice"),
            "saved-device models must remain visible without secret escape hatches"
        );
    }
}
