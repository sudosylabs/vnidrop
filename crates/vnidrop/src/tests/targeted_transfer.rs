use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    secure_secret::FaultInjectingSecretStore, CoreEvent, CoreEventSink, CoreNetworkConfig,
    CoreRelayMode, DeviceRelationshipState, PendingTargetedOffer, PublishedOutput,
    ReceiveOutputSink, ReceiveOutputSinkV2, ReceivedLocatorKind, ShareMetadataInput, ShareSource,
    SourceKind, TargetedTransferState, TransferAccessMode, VnidropCore, VnidropError,
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
    network_config: CoreNetworkConfig,
    core: Option<Arc<VnidropCore>>,
}

impl ProtectedNode {
    fn new() -> Self {
        Self::with_network_config(CoreNetworkConfig::default())
    }

    fn with_network_config(network_config: CoreNetworkConfig) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = Arc::new(FaultInjectingSecretStore::default());
        let core = VnidropCore::initialize_with_test_secret_store_and_network(
            data_dir.path().to_string_lossy().into_owned(),
            sink,
            store.clone(),
            network_config.clone(),
        )
        .expect("protected test core");
        Self {
            data_dir,
            secret_store: store,
            network_config,
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
        let core = VnidropCore::initialize_with_test_secret_store_and_network(
            self.data_dir.path().to_string_lossy().into_owned(),
            sink,
            self.secret_store.clone(),
            self.network_config.clone(),
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
    let response = accept.join().unwrap();
    assert!(matches!(
        response,
        crate::TargetedOfferResponse::Approved { transfer_id }
            if transfer_id == transfer.id
    ));

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
    accept.join().unwrap();
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

    let _ = transfer_id.clone();
    // Without durable authorization, receive by id must fail — no content yet.
    let early_receive = bob.core().receive_targeted_transfer(
        transfer_id.clone(),
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(early_receive.is_err());

    *gate.lock().unwrap() = true;
    let response = accept.join().unwrap();
    assert!(matches!(
        response,
        crate::TargetedOfferResponse::Approved { transfer_id: id } if id == transfer_id
    ));
    create.join().unwrap().unwrap();

    let output = tempfile::tempdir().unwrap();
    bob.core()
        .receive_targeted_transfer(
            transfer_id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        payload
    );

    let charlie_output = tempfile::tempdir().unwrap();
    let leaked = charlie.core().receive_targeted_transfer(
        transfer_id,
        charlie_output.path().to_string_lossy().into_owned(),
    );
    assert!(
        leaked.is_err(),
        "another endpoint must not pull by the same transfer id"
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
fn protocol_ops_are_idempotent_for_stable_transfer_id() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_001);
    let transfer = approve_one(&alice, &bob, b"idempotent payload", "payload.txt");

    // Replaying approval returns AlreadySettled — no duplicate prompts.
    let again = bob
        .core()
        .respond_to_targeted_offer(transfer.id.clone(), true)
        .unwrap();
    assert_eq!(
        again,
        crate::TargetedOfferResponse::AlreadySettled {
            transfer_id: transfer.id.clone()
        }
    );

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
    let transfer = approve_one(&alice, &bob, b"resume me please", "payload.txt");

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
    let first = approve_one(&alice, &bob, b"first manifest", "a.txt");
    let second = approve_one(&alice, &bob, b"second manifest", "b.txt");
    assert_ne!(first.id, second.id);
    assert_ne!(first.manifest_id, second.manifest_id);
}

#[test]
fn cancel_revokes_access_and_stops_streaming() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_040);
    let transfer = approve_one(&alice, &bob, b"cancel me", "payload.txt");

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
        .receive_targeted_transfer(transfer.id, output.path().to_string_lossy().into_owned());
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
    let transfer = approve_one(&alice, &bob, b"delete me", "payload.txt");

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
        transfer.id.clone(),
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(
        receive.is_err(),
        "deleted transfer must not remain receivable by id"
    );

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
    let first = approve_one(&alice, &bob, b"alpha", "one.txt");
    let second = approve_one(&alice, &bob, b"beta-payload", "two.txt");
    assert_ne!(first.id, second.id);

    alice
        .core()
        .cancel_targeted_transfer(first.id.clone())
        .unwrap();
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(first.id.clone())
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
        .receive_targeted_transfer(
            second.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("two.txt")).unwrap(),
        b"beta-payload"
    );

    let cancelled_output = tempfile::tempdir().unwrap();
    assert!(bob
        .core()
        .receive_targeted_transfer(
            first.id,
            cancelled_output.path().to_string_lossy().into_owned(),
        )
        .is_err());
}

fn complete_targeted_roundtrip(alice: &ProtectedNode, bob: &ProtectedNode, transfer_id: u64) {
    establish_saved(alice, bob, transfer_id);
    let bob_id = bob.core().status().endpoint_id.clone();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    let payload = b"profile-honoring payload";
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
            Some("payload.txt".to_string()),
        )
        .unwrap();
    assert!(matches!(
        accept.join().unwrap(),
        crate::TargetedOfferResponse::Approved { .. }
    ));
    let output = tempfile::tempdir().unwrap();
    bob.core()
        .receive_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        payload
    );
    assert!(!transfer.id.is_empty());
}

#[test]
fn targeted_transfer_honors_automatic_network_profile() {
    let alice = ProtectedNode::with_network_config(CoreNetworkConfig {
        mode: CoreRelayMode::Automatic,
        relay_urls: Vec::new(),
    });
    let bob = ProtectedNode::with_network_config(CoreNetworkConfig {
        mode: CoreRelayMode::Automatic,
        relay_urls: Vec::new(),
    });
    complete_targeted_roundtrip(&alice, &bob, 12_001);
}

#[test]
fn targeted_transfer_honors_strict_custom_and_direct_fallback_profiles() {
    // Loopback HTTP relays are the supported custom-relay development path.
    let relay = start_loopback_relay();
    let urls = vec![relay.url.clone()];
    for mode in [
        CoreRelayMode::StrictCustom,
        CoreRelayMode::CustomWithDirectFallback,
    ] {
        let config = CoreNetworkConfig {
            mode,
            relay_urls: urls.clone(),
        };
        let alice = ProtectedNode::with_network_config(config.clone());
        let bob = ProtectedNode::with_network_config(config);
        let transfer_id = match mode {
            CoreRelayMode::StrictCustom => 12_010,
            CoreRelayMode::CustomWithDirectFallback => 12_011,
            _ => unreachable!(),
        };
        complete_targeted_roundtrip(&alice, &bob, transfer_id);
    }
}

#[test]
fn targeted_transfer_local_only_uses_direct_reachability_without_relays() {
    let config = CoreNetworkConfig {
        mode: CoreRelayMode::LocalOnly,
        relay_urls: Vec::new(),
    };
    let alice = ProtectedNode::with_network_config(config.clone());
    let bob = ProtectedNode::with_network_config(config);
    assert!(!alice.core().status().addr.contains("iroh.link"));
    assert!(!bob.core().status().addr.contains("iroh.link"));
    complete_targeted_roundtrip(&alice, &bob, 12_020);
}

#[test]
fn mismatched_relay_profiles_are_typed_and_never_reinterpreted_as_ordinary_share() {
    let relay = start_loopback_relay();
    let alice = ProtectedNode::with_network_config(CoreNetworkConfig {
        mode: CoreRelayMode::Automatic,
        relay_urls: Vec::new(),
    });
    let bob = ProtectedNode::with_network_config(CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: vec![relay.url.clone()],
    });
    establish_saved(&alice, &bob, 12_030);
    let bob_id = bob.core().status().endpoint_id.clone();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"incompatible profiles").unwrap();

    let err = alice
        .core()
        .create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap_err();
    assert!(
        matches!(err, VnidropError::RelayPolicyIncompatible { .. }),
        "expected relay-policy incompatibility, got {err:?}"
    );
    assert!(bob.core().list_pending_targeted_offers().is_empty());
    let failed = alice
        .core()
        .list_targeted_transfers()
        .unwrap()
        .into_iter()
        .find(|entry| matches!(entry.state, TargetedTransferState::Failed));
    assert!(
        failed.is_some(),
        "failed targeted transfer must remain targeted, not an ordinary share"
    );
}

#[test]
fn protocol_floor_rejects_silent_downgrade_with_typed_error() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 12_040);
    let alice_id = alice.core().status().endpoint_id.clone();
    let bob_id = bob.core().status().endpoint_id.clone();
    bob.core()
        .force_relationship_protocol_floor_for_test(alice_id, 2)
        .unwrap();

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"downgrade attempt").unwrap();
    let err = alice
        .core()
        .create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap_err();
    assert!(
        matches!(err, VnidropError::ProtocolIncompatible { .. }),
        "expected protocol incompatibility, got {err:?}"
    );
    assert!(bob.core().list_pending_targeted_offers().is_empty());
}

#[test]
fn incompatible_network_or_protocol_peers_keep_ordinary_invitation_flow() {
    let relay = start_loopback_relay();
    // Profiles that cannot complete a targeted transfer can still use ordinary shares.
    let alice = ProtectedNode::with_network_config(CoreNetworkConfig {
        mode: CoreRelayMode::Automatic,
        relay_urls: Vec::new(),
    });
    let bob = ProtectedNode::with_network_config(CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: vec![relay.url.clone()],
    });
    complete_transfer(&alice, &bob, 12_050);
    assert!(alice
        .core()
        .list_pairing_eligibilities()
        .unwrap()
        .iter()
        .any(|entry| entry.peer_endpoint_id == bob.core().status().endpoint_id));

    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("invite.txt");
    std::fs::write(&source_path, b"ordinary invitation still works").unwrap();
    let share = alice
        .core()
        .share_files(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source_path.to_string_lossy().into_owned(),
                display_name: Some("invite.txt".to_string()),
                is_directory: false,
            }],
            ShareMetadataInput {
                transfer_id: 12_051,
                transfer_name: Some("invite".to_string()),
                sender_name: Some("alice".to_string()),
                access_mode: TransferAccessMode::Public,
            },
        )
        .unwrap();
    bob.core()
        .receive(
            share.ticket,
            output_dir.path().to_string_lossy().into_owned(),
            Some("bob".to_string()),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output_dir.path().join("invite.txt")).unwrap(),
        b"ordinary invitation still works"
    );
}

#[test]
fn map_offer_refuse_reasons_stay_typed() {
    use crate::targeted_transfer::protocol::map_offer_refuse_reason;

    assert!(matches!(
        map_offer_refuse_reason("relay-policy-incompatible"),
        VnidropError::RelayPolicyIncompatible { .. }
    ));
    assert!(matches!(
        map_offer_refuse_reason("protocol-incompatible"),
        VnidropError::ProtocolIncompatible { .. }
    ));
    assert!(matches!(
        map_offer_refuse_reason("unauthenticated"),
        VnidropError::Permission { .. }
    ));
}

struct LoopbackRelay {
    url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LoopbackRelay {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn start_loopback_relay() -> LoopbackRelay {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vnidrop-ticket12-relay")
            .build()
            .unwrap();
        runtime.block_on(async move {
            let relay = iroh_relay::server::RelayConfig::new((std::net::Ipv4Addr::LOCALHOST, 0));
            let mut config = iroh_relay::server::ServerConfig::default();
            config.relay = Some(relay);
            let server = match iroh_relay::server::Server::spawn(config).await {
                Ok(server) => server,
                Err(error) => {
                    ready_tx.send(Err(error.to_string())).ok();
                    return;
                }
            };
            let url = format!("http://{}", server.http_addr().expect("http addr"));
            ready_tx.send(Ok(url)).ok();
            let _ = shutdown_rx.await;
            let _ = server.shutdown().await;
        });
    });
    let url = ready_rx.recv().unwrap().expect("relay started");
    LoopbackRelay {
        url,
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
    }
}

#[derive(Default)]
struct MemoryOutputSink {
    files: Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

impl MemoryOutputSink {
    fn file(&self, relative_path: &str) -> Vec<u8> {
        self.files.lock().unwrap()[relative_path].clone()
    }
}

impl ReceiveOutputSink for MemoryOutputSink {
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

    fn finish_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        Ok(())
    }

    fn abort_file(&self, relative_path: String, _reason: String) -> Result<(), VnidropError> {
        self.files.lock().unwrap().remove(&relative_path);
        Ok(())
    }
}

impl ReceiveOutputSinkV2 for MemoryOutputSink {
    fn start_file(&self, relative_path: String) -> Result<(), VnidropError> {
        ReceiveOutputSink::start_file(self, relative_path)
    }

    fn write_chunk(&self, relative_path: String, bytes: Vec<u8>) -> Result<(), VnidropError> {
        ReceiveOutputSink::write_chunk(self, relative_path, bytes)
    }

    fn finish_file(&self, relative_path: String) -> Result<PublishedOutput, VnidropError> {
        ReceiveOutputSink::finish_file(self, relative_path.clone())?;
        Ok(PublishedOutput {
            locator_kind: ReceivedLocatorKind::FilesystemPath,
            locator: format!("memory://{relative_path}"),
        })
    }

    fn abort_file(&self, relative_path: String, reason: String) -> Result<(), VnidropError> {
        ReceiveOutputSink::abort_file(self, relative_path, reason)
    }
}

#[test]
fn targeted_receive_and_resume_through_output_sinks() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_070);
    let transfer = approve_one(&alice, &bob, b"sink payload", "sink.txt");

    let sink = Arc::new(MemoryOutputSink::default());
    bob.core()
        .receive_targeted_transfer_with_output_sink(transfer.id.clone(), sink.clone())
        .unwrap();
    assert_eq!(sink.file("sink.txt"), b"sink payload");
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );

    let transfer2 = approve_one(&alice, &bob, b"resume sink", "resume.txt");
    let bob = bob.restart();
    let sink_v2 = Arc::new(MemoryOutputSink::default());
    bob.core()
        .resume_targeted_transfer_with_output_sink_v2(transfer2.id.clone(), sink_v2.clone())
        .unwrap();
    assert_eq!(sink_v2.file("resume.txt"), b"resume sink");
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
fn decline_returns_typed_declined_outcome() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    establish_saved(&alice, &bob, 11_080);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"no thanks").unwrap();

    let bob_core = bob.core().clone();
    let decline = std::thread::spawn(move || {
        let offer = wait_for_pending_offer(&bob_core);
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, false)
            .unwrap()
    });
    let create_err = alice
        .core()
        .create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
        .unwrap_err();
    assert!(matches!(create_err, VnidropError::Permission { .. }));
    assert_eq!(
        decline.join().unwrap(),
        crate::TargetedOfferResponse::Declined
    );
}
