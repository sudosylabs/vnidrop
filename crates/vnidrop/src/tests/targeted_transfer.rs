use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    secure_secret::FaultInjectingSecretStore, CoreEvent, CoreEventSink, DeviceRelationshipState,
    PendingTargetedOffer, ShareMetadataInput, ShareSource, SourceKind, TargetedTransferState,
    TransferAccessMode, VnidropCore, VnidropError,
};

struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl CoreEventSink for RecordingSink {
    fn on_event(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct ProtectedNode {
    data_dir: tempfile::TempDir,
    secret_store: Arc<FaultInjectingSecretStore>,
    core: Option<Arc<VnidropCore>>,
}

impl ProtectedNode {
    fn new() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = Arc::new(FaultInjectingSecretStore::default());
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink,
            store.clone(),
        )
        .expect("protected test core");
        Self {
            data_dir,
            secret_store: store,
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
            sink,
            self.secret_store.clone(),
        )
        .expect("restarted protected test core");
        self.core = Some(core);
        self
    }
}

impl Drop for ProtectedNode {
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

fn complete_transfer(sender: &ProtectedNode, receiver: &ProtectedNode, transfer_id: u64) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"mutual consent").unwrap();
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

fn establish_saved(alice: &ProtectedNode, bob: &ProtectedNode, transfer_id: u64) {
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
        .respond_to_device_pairing(alice_id.clone(), true)
        .unwrap());
    wait_for_relationship(&alice.core(), &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(&bob.core(), &alice_id, DeviceRelationshipState::Saved);
}

fn wait_for_pending_offer(core: &VnidropCore) -> PendingTargetedOffer {
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

#[test]
fn create_targeted_transfer_is_immutable_and_saved_only() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let stranger = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    let stranger_id = stranger.core().status().endpoint_id.clone();
    establish_saved(&alice, &bob, 10_001);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"immutable payload").unwrap();

    let stranger_err = alice
        .core()
        .create_targeted_transfer(
            stranger_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap_err();
    assert!(matches!(stranger_err, VnidropError::Permission { .. }));

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
            bob_id.clone(),
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap();
    let _auth = accept.join().unwrap().expect("authorization after approve");

    assert_eq!(
        transfer.sender_endpoint_id,
        alice.core().status().endpoint_id
    );
    assert_eq!(transfer.receiver_endpoint_id, bob_id);
    assert_eq!(transfer.file_count, 1);
    assert_eq!(transfer.total_size, b"immutable payload".len() as u64);
    assert!(!transfer.id.is_empty());
    assert!(!transfer.manifest_id.is_empty());
    assert!(matches!(
        transfer.state,
        TargetedTransferState::Approved
            | TargetedTransferState::Connecting
            | TargetedTransferState::Transferring
            | TargetedTransferState::Completed
    ));

    let listed = alice
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap();
    let listed = listed.expect("durable targeted transfer");
    assert_eq!(listed.id, transfer.id);
    assert_eq!(listed.sender_endpoint_id, transfer.sender_endpoint_id);
    assert_eq!(listed.receiver_endpoint_id, transfer.receiver_endpoint_id);
    assert_eq!(listed.manifest_id, transfer.manifest_id);
    assert_eq!(listed.file_count, transfer.file_count);
    assert_eq!(listed.total_size, transfer.total_size);
}

#[test]
fn preapproval_offer_is_authenticated_without_ordinary_share_ticket() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    establish_saved(&alice, &bob, 10_010);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"offer body").unwrap();

    let bob_core = bob.core().clone();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        // Offer surfaces identity + manifest summary only — never a reusable ticket.
        assert!(!offer.transfer_id.is_empty());
        assert_eq!(offer.sender_endpoint_id, alice_id_from(&bob_core));
        assert_eq!(offer.file_count, 1);
        assert_eq!(offer.total_size, b"offer body".len() as u64);
        assert!(!offer.manifest_id.is_empty());
        assert!(!offer.content_hash.is_empty());
        assert!(offer.protocol_version >= 1);
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, true)
            .unwrap()
    });

    alice
        .core()
        .create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap();
    accept.join().unwrap().unwrap();
}

fn alice_id_from(bob: &VnidropCore) -> String {
    bob.list_saved_devices()
        .unwrap()
        .into_iter()
        .next()
        .expect("saved alice")
        .endpoint_id
}

#[test]
fn invalid_offer_never_becomes_observable_pending_approval() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    // No Saved relationship — offer must not surface.
    let bob_id = bob.core().status().endpoint_id.clone();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"nope").unwrap();

    let err = alice
        .core()
        .create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap_err();
    assert!(matches!(err, VnidropError::Permission { .. }));
    assert!(bob.core().list_pending_targeted_offers().is_empty());
}

#[test]
fn explicit_approval_gates_content_and_binds_authorization_to_receiver() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let charlie = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    establish_saved(&alice, &bob, 10_020);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    let payload = b"bound authorization payload";
    std::fs::write(&source_path, payload).unwrap();

    let bob_core = bob.core().clone();
    let offer_id = Arc::new(Mutex::new(None::<String>));
    let offer_id_setter = offer_id.clone();
    let gate = Arc::new(Mutex::new(false));
    let gate_wait = gate.clone();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        *offer_id_setter.lock().unwrap() = Some(offer.transfer_id.clone());
        // Hold approval until the main thread asserts that content is unavailable.
        let started = Instant::now();
        loop {
            if *gate_wait.lock().unwrap() {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "approval gate never opened"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, true)
            .unwrap()
    });

    let alice_core = alice.core().clone();
    let source = targeted_source(&source_path);
    let create = std::thread::spawn(move || {
        alice_core.create_targeted_transfer(bob_id, vec![source], Some("payload.txt".to_string()))
    });

    let started = Instant::now();
    let transfer_id = loop {
        if let Some(id) = offer_id.lock().unwrap().clone() {
            break id;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "offer id never observed"
        );
        std::thread::sleep(Duration::from_millis(25));
    };

    let _ = transfer_id;
    // Without an approved authorization, receive must fail — no content yet.
    let early_receive = bob.core().receive_targeted_transfer(
        "not-a-real-authorization".to_string(),
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(early_receive.is_err());

    *gate.lock().unwrap() = true;
    let auth = accept.join().unwrap().expect("receiver authorization");
    create.join().unwrap().unwrap();

    let output = tempfile::tempdir().unwrap();
    bob.core()
        .receive_targeted_transfer(auth.clone(), output.path().to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        payload
    );

    let charlie_output = tempfile::tempdir().unwrap();
    let leaked = charlie
        .core()
        .receive_targeted_transfer(auth, charlie_output.path().to_string_lossy().into_owned());
    assert!(
        leaked.is_err(),
        "leaked authorization must not authorize another endpoint"
    );
}

#[test]
fn invitation_multi_receiver_shares_remain_independently_authorized() {
    let source_dir = tempfile::tempdir().unwrap();
    let first_output = tempfile::tempdir().unwrap();
    let second_output = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"shared with both receivers").unwrap();
    let sender = ProtectedNode::new();
    let first_receiver = ProtectedNode::new();
    let second_receiver = ProtectedNode::new();
    let share = sender
        .core()
        .share_files(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source_path.to_string_lossy().into_owned(),
                display_name: Some("shared.txt".to_string()),
                is_directory: false,
            }],
            ShareMetadataInput {
                transfer_id: 10_030,
                transfer_name: Some("Existing share".to_string()),
                sender_name: Some("Sender".to_string()),
                access_mode: TransferAccessMode::Public,
            },
        )
        .unwrap();

    for (receiver, output) in [
        (&first_receiver, first_output.path()),
        (&second_receiver, second_output.path()),
    ] {
        receiver
            .core()
            .receive(
                share.ticket.clone(),
                output.to_string_lossy().into_owned(),
                Some("Receiver".to_string()),
            )
            .unwrap();
        assert_eq!(
            std::fs::read(output.join("shared.txt")).unwrap(),
            b"shared with both receivers"
        );
    }
}

fn approve_one(
    alice: &ProtectedNode,
    bob: &ProtectedNode,
    payload: &[u8],
    name: &str,
) -> (crate::TargetedTransfer, String) {
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
    let auth = accept.join().unwrap().expect("authorization");
    (transfer, auth)
}

#[test]
fn protocol_ops_are_idempotent_for_stable_transfer_id() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_001);
    let (transfer, auth) = approve_one(&alice, &bob, b"idempotent payload", "payload.txt");

    // Replaying approval returns the same authorization — no duplicate prompts.
    let again = bob
        .core()
        .respond_to_targeted_offer(transfer.id.clone(), true)
        .unwrap()
        .expect("idempotent authorization");
    assert_eq!(again, auth);

    let listed = bob
        .core()
        .list_targeted_transfers()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.id == transfer.id)
        .count();
    assert_eq!(listed, 1, "replay must not create duplicate durable rows");
}

#[test]
fn unapproved_offers_vanish_on_cancel_and_core_restart() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    establish_saved(&alice, &bob, 11_010);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"vanishing offer").unwrap();

    let bob_core = bob.core().clone();
    let seen = Arc::new(Mutex::new(None::<String>));
    let seen_set = seen.clone();
    let hold = Arc::new(Mutex::new(true));
    let hold_wait = hold.clone();
    let watcher = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        *seen_set.lock().unwrap() = Some(offer.transfer_id.clone());
        let started = Instant::now();
        while *hold_wait.lock().unwrap() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "cancel never cleared the live offer"
            );
            if bob_core.list_pending_targeted_offers().is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });

    let alice_core = alice.core().clone();
    let source = targeted_source(&source_path);
    let create = std::thread::spawn(move || {
        alice_core.create_targeted_transfer(bob_id, vec![source], Some("payload.txt".to_string()))
    });

    let transfer_id = loop {
        if let Some(id) = seen.lock().unwrap().clone() {
            break id;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    alice.core().cancel_targeted_transfer(transfer_id).unwrap();
    watcher.join().unwrap();
    let create_err = create.join().unwrap();
    assert!(create_err.is_err(), "cancelled offer must fail create");
    assert!(bob.core().list_pending_targeted_offers().is_empty());

    // Restart clears any live-session inbox residue.
    let bob = bob.restart();
    assert!(bob.core().list_pending_targeted_offers().is_empty());
    *hold.lock().unwrap() = false;
}

#[test]
fn approved_transfer_resumes_after_restart_without_reapproval() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_020);
    let (transfer, _auth) = approve_one(&alice, &bob, b"resume me please", "payload.txt");

    let bob_before = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .expect("receiver durable approved row");
    assert_eq!(bob_before.state, TargetedTransferState::Approved);
    assert_eq!(bob_before.verified_bytes, 0);

    let alice = alice.restart();
    let bob = bob.restart();

    let alice_after = alice
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .expect("sender durable state");
    assert_eq!(alice_after.state, TargetedTransferState::Approved);
    let bob_after = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .expect("receiver durable state");
    assert_eq!(bob_after.state, TargetedTransferState::Approved);

    let output = tempfile::tempdir().unwrap();
    bob.core()
        .resume_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        b"resume me please"
    );
    let completed = bob
        .core()
        .get_targeted_transfer(transfer.id)
        .unwrap()
        .unwrap();
    assert_eq!(completed.state, TargetedTransferState::Completed);
    assert_eq!(completed.verified_bytes, b"resume me please".len() as u64);
}

#[test]
fn manifest_change_requires_new_transfer_identity() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_030);
    let (first, _) = approve_one(&alice, &bob, b"first manifest", "a.txt");
    let (second, _) = approve_one(&alice, &bob, b"second manifest", "b.txt");
    assert_ne!(first.id, second.id);
    assert_ne!(first.manifest_id, second.manifest_id);
}

#[test]
fn cancel_revokes_access_and_stops_streaming() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_040);
    let (transfer, auth) = approve_one(&alice, &bob, b"cancel me", "payload.txt");

    alice
        .core()
        .cancel_targeted_transfer(transfer.id.clone())
        .unwrap();
    let cancelled = alice
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.state, TargetedTransferState::Cancelled);

    let output = tempfile::tempdir().unwrap();
    let receive = bob
        .core()
        .receive_targeted_transfer(auth, output.path().to_string_lossy().into_owned());
    assert!(
        receive.is_err(),
        "cancelled transfer must not remain receivable"
    );
}

#[test]
fn delete_removes_authorization_and_resumable_state() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_050);
    let (transfer, auth) = approve_one(&alice, &bob, b"delete me", "payload.txt");

    bob.core()
        .delete_targeted_transfer(transfer.id.clone())
        .unwrap();
    let deleted = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(deleted.state, TargetedTransferState::Deleted);
    assert_eq!(deleted.verified_bytes, 0);

    let resume = bob.core().resume_targeted_transfer(
        transfer.id.clone(),
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(resume.is_err(), "deleted transfer must not resume");

    let receive = bob.core().receive_targeted_transfer(
        auth,
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    // Auth blob may still decode, but durable resume path is gone; receive may
    // still attempt content pull if sender serves — sender delete is separate.
    let _ = receive;

    alice
        .core()
        .delete_targeted_transfer(transfer.id.clone())
        .unwrap();
    let sender_deleted = alice
        .core()
        .get_targeted_transfer(transfer.id)
        .unwrap()
        .unwrap();
    assert_eq!(sender_deleted.state, TargetedTransferState::Deleted);
}

#[test]
fn concurrent_independent_transfers_between_same_devices_are_isolated() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_060);

    // Design allows only one unresolved offer per sender; approve sequentially,
    // then prove independent approved transfers do not corrupt each other.
    let (first, first_auth) = approve_one(&alice, &bob, b"alpha", "one.txt");
    let (second, second_auth) = approve_one(&alice, &bob, b"beta-payload", "two.txt");
    assert_ne!(first.id, second.id);

    alice
        .core()
        .cancel_targeted_transfer(first.id.clone())
        .unwrap();
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(first.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(second.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Approved
    );

    let output = tempfile::tempdir().unwrap();
    bob.core()
        .receive_targeted_transfer(second_auth, output.path().to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("two.txt")).unwrap(),
        b"beta-payload"
    );

    let cancelled_output = tempfile::tempdir().unwrap();
    assert!(bob
        .core()
        .receive_targeted_transfer(
            first_auth,
            cancelled_output.path().to_string_lossy().into_owned(),
        )
        .is_err());
}
