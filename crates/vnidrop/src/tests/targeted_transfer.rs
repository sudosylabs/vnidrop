use std::{
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use iroh::{endpoint::presets, Endpoint};
use iroh_blobs::{get::request::get_hash_seq_and_sizes, ticket::BlobTicket};

use crate::{
    secure_secret::{FaultInjectingSecretStore, ReferenceStoreFailure},
    CoreEvent, CoreEventSink, CoreNetworkConfig, CoreRelayMode, DeviceRelationshipState,
    PendingTargetedOffer, PublishedOutput, ReceiveOutputSink, ReceiveOutputSinkV2,
    ReceivedLocatorKind, ShareMetadataInput, ShareSource, SourceKind, TargetedTransferState,
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
    network_config: CoreNetworkConfig,
    core: Option<Arc<VnidropCore>>,
    sink: Arc<RecordingSink>,
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
            sink.clone(),
            store.clone(),
            network_config.clone(),
        )
        .expect("protected test core");
        Self {
            data_dir,
            secret_store: store,
            network_config,
            core: Some(core),
            sink,
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
            sink.clone(),
            self.secret_store.clone(),
            self.network_config.clone(),
        )
        .expect("restarted protected test core");
        self.core = Some(core);
        self.sink = sink;
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
    let invitation_history_before = alice
        .core()
        .list_transfers()
        .unwrap()
        .into_iter()
        .map(|transfer| (transfer.local_id, transfer.ticket))
        .collect::<Vec<_>>();

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
        alice
            .core()
            .list_transfers()
            .unwrap()
            .into_iter()
            .map(|transfer| (transfer.local_id, transfer.ticket))
            .collect::<Vec<_>>(),
        invitation_history_before,
        "targeted payloads must not become invitation transfers"
    );

    assert_eq!(
        transfer.sender_endpoint_id,
        alice.core().status().endpoint_id
    );
    assert_eq!(transfer.receiver_endpoint_id, bob_id);
    assert_eq!(transfer.file_count, 1);
    assert_eq!(transfer.total_size, b"immutable payload".len() as u64);
    assert_eq!(transfer.transfer_name, "payload.txt");
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
    assert_eq!(listed.transfer_name, transfer.transfer_name);
    assert_eq!(listed.file_count, transfer.file_count);
    assert_eq!(listed.total_size, transfer.total_size);
}

#[test]
fn identity_reset_cancels_targeted_authorization_bound_to_the_lost_endpoint() {
    let mut alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id;
    establish_saved(&alice, &bob, 10_002);
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("identity-bound.txt");
    std::fs::write(&source_path, b"identity-bound authorization").unwrap();
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
            bob_id,
            vec![targeted_source(&source_path)],
            Some("identity-bound.txt".to_string()),
        )
        .unwrap();
    assert!(matches!(
        accept.join().unwrap(),
        crate::TargetedOfferResponse::Approved { .. }
    ));

    alice.core.take().unwrap().shutdown();
    let endpoint_handle = alice.secret_store.endpoint_identity_handle_for_test();
    alice.secret_store.remove_for_test(&endpoint_handle);
    let recovered = VnidropCore::reset_unrecoverable_identity_with_test_secret_store(
        alice.data_dir.path().to_string_lossy().into_owned(),
        alice.sink.clone(),
        alice.secret_store.clone(),
    )
    .unwrap();

    let snapshot = recovered
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.state, TargetedTransferState::Cancelled);
    assert!(recovered
        .targeted_blob_ticket_for_test(transfer.id)
        .is_err());
    assert!(recovered.list_saved_devices().unwrap().is_empty());
    recovered.shutdown();
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
    let alice_invitation_count = alice.core().list_transfers().unwrap().len();
    let bob_invitation_count = bob.core().list_transfers().unwrap().len();
    let alice_invitation_requests = alice
        .core()
        .list_receiver_requests(10_020)
        .unwrap()
        .into_iter()
        .map(|request| (request.id, request.status, request.completed_at))
        .collect::<Vec<_>>();
    let bob_invitation_requests = bob
        .core()
        .list_receiver_requests(10_020)
        .unwrap()
        .into_iter()
        .map(|request| (request.id, request.status, request.completed_at))
        .collect::<Vec<_>>();
    let alice_eligibility_count = alice.core().list_pairing_eligibilities().unwrap().len();
    let bob_eligibility_count = bob.core().list_pairing_eligibilities().unwrap().len();
    let bob_artifacts_before = bob.core().list_received_artifacts().unwrap();
    let alice_delivery_events = alice
        .core()
        .list_events(None)
        .unwrap()
        .iter()
        .filter(|event| event.phase == "delivery")
        .count();
    let bob_delivery_events = bob
        .core()
        .list_events(None)
        .unwrap()
        .iter()
        .filter(|event| event.phase == "delivery")
        .count();

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
    let completion_started = Instant::now();
    loop {
        if alice
            .core()
            .get_targeted_transfer(transfer_id.clone())
            .unwrap()
            .is_some_and(|row| row.state == TargetedTransferState::Completed)
        {
            break;
        }
        assert!(
            completion_started.elapsed() < Duration::from_secs(10),
            "sender never durably acknowledged targeted completion"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        alice.core().list_transfers().unwrap().len(),
        alice_invitation_count
    );
    assert_eq!(
        bob.core().list_transfers().unwrap().len(),
        bob_invitation_count
    );
    assert_eq!(
        alice.core().list_pairing_eligibilities().unwrap().len(),
        alice_eligibility_count
    );
    assert_eq!(
        bob.core().list_pairing_eligibilities().unwrap().len(),
        bob_eligibility_count
    );
    assert_eq!(
        alice
            .core()
            .list_receiver_requests(10_020)
            .unwrap()
            .into_iter()
            .map(|request| (request.id, request.status, request.completed_at))
            .collect::<Vec<_>>(),
        alice_invitation_requests,
        "targeted receive must not create sender invitation approval requests"
    );
    assert_eq!(
        bob.core()
            .list_receiver_requests(10_020)
            .unwrap()
            .into_iter()
            .map(|request| (request.id, request.status, request.completed_at))
            .collect::<Vec<_>>(),
        bob_invitation_requests,
        "targeted receive must not create receiver invitation requests"
    );
    assert_eq!(
        bob.core().list_received_artifacts().unwrap(),
        bob_artifacts_before
    );
    assert_eq!(
        alice
            .core()
            .list_events(None)
            .unwrap()
            .iter()
            .filter(|event| event.phase == "delivery")
            .count(),
        alice_delivery_events,
        "targeted completion must not emit invitation delivery receipts"
    );
    assert_eq!(
        bob.core()
            .list_events(None)
            .unwrap()
            .iter()
            .filter(|event| event.phase == "delivery")
            .count(),
        bob_delivery_events,
        "targeted receive must not emit invitation delivery events"
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
fn unrelated_endpoint_cannot_discover_or_approve_another_receivers_offer() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let charlie = ProtectedNode::new();
    let bob_id = bob.core().status().endpoint_id.clone();
    establish_saved(&alice, &bob, 10_024);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("private.txt");
    std::fs::write(&source_path, b"only Bob may approve").unwrap();
    let alice_core = alice.core().clone();
    let create = std::thread::spawn(move || {
        alice_core.create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("private.txt".to_string()),
        )
    });
    let offer = wait_for_pending_offer(&bob.core());

    assert!(charlie.core().list_pending_targeted_offers().is_empty());
    assert!(charlie.core().list_targeted_transfers().unwrap().is_empty());
    let forged = charlie
        .core()
        .respond_to_targeted_offer(offer.transfer_id.clone(), true);
    assert!(
        forged.is_err(),
        "an unrelated endpoint must not approve Bob's offer"
    );
    assert!(charlie.core().list_pending_targeted_offers().is_empty());
    assert!(charlie.core().list_targeted_transfers().unwrap().is_empty());
    assert_eq!(
        bob.core().list_pending_targeted_offers(),
        vec![offer.clone()]
    );

    bob.core()
        .respond_to_targeted_offer(offer.transfer_id, false)
        .unwrap();
    assert!(create.join().unwrap().is_err());
}

#[test]
fn unrelated_endpoint_cannot_fetch_a_leaked_targeted_blob_ticket() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 10_025);
    let transfer = approve_one(&alice, &bob, b"private payload", "payload.txt");
    let (protocol_transfer_id, leaked) = alice
        .core()
        .targeted_blob_ticket_for_test(transfer.id)
        .unwrap();
    let collision_source_dir = tempfile::tempdir().unwrap();
    let collision_source = collision_source_dir.path().join("public.txt");
    std::fs::write(&collision_source, b"unrelated public payload").unwrap();
    let collision = alice.core().share_files(
        vec![targeted_source(&collision_source)],
        ShareMetadataInput {
            transfer_id: protocol_transfer_id,
            transfer_name: Some("public.txt".to_string()),
            sender_name: Some("sender".to_string()),
            access_mode: TransferAccessMode::Public,
        },
    );
    assert!(
        collision.is_err(),
        "an invitation transfer must not reuse a targeted ACL identity"
    );
    let ticket = BlobTicket::from_str(&leaked).unwrap();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        let attacker = Endpoint::builder(presets::Minimal).bind().await.unwrap();
        let connection = attacker
            .connect(ticket.addr().clone(), iroh_blobs::ALPN)
            .await
            .unwrap();
        let result = get_hash_seq_and_sizes(
            &connection,
            &ticket.hash_and_format().hash,
            1024 * 1024 * 32,
            None,
        )
        .await;
        assert!(
            result.is_err(),
            "a leaked targeted blob ticket must not authorize another endpoint"
        );
        attacker.close().await;
    });
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
fn approved_authorization_delivery_retries_after_sender_restart() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_000);
    alice
        .core()
        .suppress_targeted_authorization_delivery_for_test(true);

    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"durable authorization").unwrap();
    let bob_id = bob.core().status().endpoint_id.clone();
    let alice_core = alice.core();
    let create = std::thread::spawn(move || {
        alice_core.create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
    });
    let offer = wait_for_pending_offer(&bob.core());
    let bob_core = bob.core();
    let offer_id = offer.transfer_id.clone();
    let accept = std::thread::spawn(move || bob_core.respond_to_targeted_offer(offer_id, true));

    let transfer = create
        .join()
        .unwrap()
        .expect("sender remains approved while delivery is pending");
    assert_eq!(transfer.state, TargetedTransferState::Approved);
    assert!(bob.core().list_targeted_transfers().unwrap().is_empty());

    let alice = alice.restart();
    let response = accept
        .join()
        .unwrap()
        .expect("sender restart redelivers durable authorization");
    assert!(matches!(
        response,
        crate::TargetedOfferResponse::Approved { transfer_id } if transfer_id == transfer.id
    ));
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Approved
    );
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Approved
    );
}

#[test]
fn accepted_intent_survives_receiver_and_sender_restart_until_delivery() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_000);
    alice
        .core()
        .suppress_targeted_authorization_delivery_for_test(true);
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"restart consent").unwrap();
    let bob_id = bob.core().status().endpoint_id;
    let alice_core = alice.core();
    let create = std::thread::spawn(move || {
        alice_core.create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
    });
    let offer = wait_for_pending_offer(&bob.core());
    bob.core()
        .accept_targeted_offer_without_waiting_for_test(offer.transfer_id)
        .unwrap();
    let transfer = create.join().unwrap().unwrap();
    let bob = bob.restart();
    let alice = alice.restart();
    let started = Instant::now();
    loop {
        if bob
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .is_some_and(|row| row.state == TargetedTransferState::Approved)
        {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Approved
    );
}

#[test]
fn restart_reconciles_targeted_authorization_orphaned_before_domain_commit() {
    let node = ProtectedNode::new();
    node.core()
        .create_orphaned_targeted_authorization_for_test()
        .unwrap();
    assert_eq!(
        node.core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        1
    );
    let node = node.restart();
    assert_eq!(
        node.core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        0
    );
}

#[test]
fn terminal_receiver_secret_cleanup_retries_while_relationship_is_saved() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_005);
    let transfer = approve_one(&alice, &bob, b"cleanup boundary", "payload.txt");
    assert_eq!(
        bob.core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        1
    );
    bob.secret_store
        .fail_with(Some(ReferenceStoreFailure::Unavailable));
    assert!(bob
        .core()
        .cancel_targeted_transfer(transfer.id.clone())
        .is_err());
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    bob.secret_store.fail_with(None);
    let bob = bob.restart();
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    assert_eq!(
        bob.core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        0
    );
}

#[test]
fn forgetting_saved_receiver_keeps_sender_denied_when_secret_delete_retries() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_006);
    let transfer = approve_one(&alice, &bob, b"sender cleanup boundary", "payload.txt");
    assert_eq!(
        alice
            .core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        1
    );
    let protected_values_before_forget = alice.secret_store.stored_value_count_for_test();

    alice
        .secret_store
        .fail_with(Some(ReferenceStoreFailure::Unavailable));
    assert!(alice
        .core()
        .forget_saved_device(bob.core().status().endpoint_id)
        .is_err());
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    assert!(alice
        .core()
        .targeted_blob_ticket_for_test(transfer.id.clone())
        .is_err());
    assert_eq!(
        alice.secret_store.stored_value_count_for_test(),
        protected_values_before_forget,
        "failed secure deletion must leave retryable protected material"
    );

    alice.secret_store.fail_with(None);
    let alice = alice.restart();
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    assert!(alice
        .core()
        .targeted_blob_ticket_for_test(transfer.id)
        .is_err());
    assert_eq!(
        alice
            .core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        0
    );
    assert!(
        alice.secret_store.stored_value_count_for_test() < protected_values_before_forget,
        "restart reconciliation must delete orphaned protected material"
    );
}

#[test]
fn cancel_after_accepted_receiver_restart_revokes_durable_consent() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_003);
    alice
        .core()
        .suppress_targeted_authorization_delivery_for_test(true);
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"revoked consent").unwrap();
    let bob_id = bob.core().status().endpoint_id;
    let alice_core = alice.core();
    let create = std::thread::spawn(move || {
        alice_core.create_targeted_transfer(
            bob_id,
            vec![targeted_source(&source_path)],
            Some("payload.txt".to_string()),
        )
    });
    let offer = wait_for_pending_offer(&bob.core());
    bob.core()
        .accept_targeted_offer_without_waiting_for_test(offer.transfer_id.clone())
        .unwrap();
    let transfer = create.join().unwrap().unwrap();
    let bob = bob.restart();
    bob.core()
        .cancel_targeted_transfer(offer.transfer_id)
        .unwrap();
    let alice = alice.restart();
    let started = Instant::now();
    while alice
        .core()
        .targeted_authorization_delivery_attempts_for_test()
        == 0
    {
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(bob.core().list_targeted_transfers().unwrap().is_empty());
    assert_ne!(
        alice
            .core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );
}

#[test]
fn restart_never_restores_targeted_access_for_a_persisted_block() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_001);
    let transfer = approve_one(&alice, &bob, b"blocked after crash", "payload.txt");
    let bob_id = bob.core().status().endpoint_id;
    alice
        .core()
        .persist_block_without_cleanup_for_test(bob_id)
        .unwrap();

    let alice = alice.restart();
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    assert!(!alice
        .core()
        .targeted_payload_is_registered_for_test(transfer.id.clone())
        .unwrap());
    let output = tempfile::tempdir().unwrap();
    assert!(bob
        .core()
        .receive_targeted_transfer(transfer.id, output.path().to_string_lossy().into_owned(),)
        .is_err());
    assert!(std::fs::read_dir(output.path()).unwrap().next().is_none());
}

#[test]
fn corrupt_restored_target_does_not_strand_later_valid_target() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_002);
    let corrupt = approve_one(&alice, &bob, b"corrupt", "corrupt.txt");
    let valid = approve_one(&alice, &bob, b"valid", "valid.txt");
    alice
        .core()
        .corrupt_targeted_content_hash_for_test(corrupt.id.clone())
        .unwrap();

    let alice = alice.restart();
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(corrupt.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Failed
    );
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(valid.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Approved
    );
    assert!(alice
        .core()
        .targeted_payload_is_registered_for_test(valid.id)
        .unwrap());
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
fn authorization_replay_after_receiver_commit_and_restart_is_stored() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 110_004);
    let transfer = approve_one(&alice, &bob, b"lost stored response", "payload.txt");
    let bob = bob.restart();

    assert!(alice
        .core()
        .redeliver_targeted_authorization_for_test(transfer.id.clone())
        .unwrap());
    let receiver = bob
        .core()
        .get_targeted_transfer(transfer.id)
        .unwrap()
        .unwrap();
    assert_eq!(receiver.state, TargetedTransferState::Approved);
    assert_eq!(
        bob.core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        1,
        "idempotent replay must not create another protected secret"
    );
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
    let sender_completed = alice
        .core()
        .get_targeted_transfer(completed.id)
        .unwrap()
        .unwrap();
    assert_eq!(sender_completed.state, TargetedTransferState::Completed);
    let started = Instant::now();
    while alice
        .core()
        .targeted_payload_is_registered_for_test(sender_completed.id.clone())
        .unwrap()
    {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "completed sender payload access was not released"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn completion_retries_after_receiver_restart_without_failing_published_receive() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_025);
    let transfer = approve_one(&alice, &bob, b"eventual completion", "payload.txt");
    bob.core().suppress_targeted_completion_for_test(true);

    let output = tempfile::tempdir().unwrap();
    bob.core()
        .receive_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("payload.txt")).unwrap(),
        b"eventual completion"
    );
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );
    assert_eq!(
        alice
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Approved
    );

    let bob = bob.restart();
    let started = Instant::now();
    loop {
        if alice
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .is_some_and(|entry| entry.state == TargetedTransferState::Completed)
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "completion outbox was not retried after restart"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );
}

#[test]
fn targeted_path_receive_preserves_no_overwrite_and_resumes_elsewhere() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_026);
    let transfer = approve_one(&alice, &bob, b"new payload", "payload.txt");
    let occupied = tempfile::tempdir().unwrap();
    std::fs::write(occupied.path().join("payload.txt"), b"keep me").unwrap();

    assert!(bob
        .core()
        .receive_targeted_transfer(
            transfer.id.clone(),
            occupied.path().to_string_lossy().into_owned(),
        )
        .is_err());
    assert_eq!(
        std::fs::read(occupied.path().join("payload.txt")).unwrap(),
        b"keep me"
    );
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Interrupted
    );
    let interrupted = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(interrupted.verified_bytes, interrupted.total_size);

    let bob = bob.restart();
    let restored = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(restored.verified_bytes, interrupted.verified_bytes);
    let clean = tempfile::tempdir().unwrap();
    bob.core()
        .resume_targeted_transfer(transfer.id, clean.path().to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(
        std::fs::read(clean.path().join("payload.txt")).unwrap(),
        b"new payload"
    );
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
    assert_eq!(
        alice
            .core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        1
    );

    alice
        .core()
        .cancel_targeted_transfer(transfer.id.clone())
        .unwrap();
    assert_eq!(
        alice
            .core()
            .targeted_authorization_handle_count_for_test()
            .unwrap(),
        0,
        "sender cancellation must clean protected authorization custody"
    );
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
fn delete_keeps_durable_denial_and_retries_secure_secret_cleanup() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_051);
    let transfer = approve_one(&alice, &bob, b"delete retry", "payload.txt");

    bob.secret_store
        .fail_with(Some(ReferenceStoreFailure::Unavailable));
    assert!(bob
        .core()
        .delete_targeted_transfer(transfer.id.clone())
        .is_err());
    let denied = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(denied.state, TargetedTransferState::Deleted);

    bob.secret_store.fail_with(None);
    bob.core()
        .delete_targeted_transfer(transfer.id.clone())
        .unwrap();
    let retried = bob
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        retried, denied,
        "private cleanup must not mutate the public snapshot"
    );
    assert!(bob
        .core()
        .resume_targeted_transfer(
            transfer.id,
            tempfile::tempdir()
                .unwrap()
                .path()
                .to_string_lossy()
                .into_owned(),
        )
        .is_err());
}

#[test]
fn sender_delete_revokes_receiver_bound_payload_access() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_055);
    let transfer = approve_one(&alice, &bob, b"delete at sender", "payload.txt");

    alice
        .core()
        .delete_targeted_transfer(transfer.id.clone())
        .unwrap();
    let deleted = alice
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(deleted.state, TargetedTransferState::Deleted);

    let output = tempfile::tempdir().unwrap();
    let receive = bob
        .core()
        .receive_targeted_transfer(transfer.id, output.path().to_string_lossy().into_owned());
    assert!(
        receive.is_err(),
        "sender deletion must revoke the receiver's provider access"
    );
}

#[test]
fn concurrent_independent_transfers_between_same_devices_are_isolated() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_060);

    // Design allows only one unresolved offer per sender; approve sequentially,
    // then overlap the independent pulls.
    let first = approve_one(&alice, &bob, b"alpha", "one.txt");
    let second = approve_one(&alice, &bob, b"beta-payload", "two.txt");
    assert_ne!(first.id, second.id);

    let first_sink = Arc::new(GatedSink::default());
    let second_sink = Arc::new(GatedSink::default());
    let first_core = bob.core();
    let first_id = first.id.clone();
    let first_thread_sink = first_sink.clone();
    let first_receive = std::thread::spawn(move || {
        first_core.receive_targeted_transfer_with_output_sink(first_id, first_thread_sink)
    });
    let second_core = bob.core();
    let second_id = second.id.clone();
    let second_thread_sink = second_sink.clone();
    let second_receive = std::thread::spawn(move || {
        second_core.receive_targeted_transfer_with_output_sink(second_id, second_thread_sink)
    });
    first_sink.wait_until_entered();
    second_sink.wait_until_entered();

    bob.core()
        .cancel_targeted_transfer(first.id.clone())
        .unwrap();
    first_sink.release();
    second_sink.release();
    assert!(first_receive.join().unwrap().is_err());
    second_receive.join().unwrap().unwrap();
    assert_eq!(
        bob.core()
            .get_targeted_transfer(first.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
    assert_eq!(
        bob.core()
            .get_targeted_transfer(second.id.clone())
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );
    assert_eq!(
        first_sink.aborts.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        second_sink
            .finishes
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
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

#[derive(Default)]
struct FailingFinishSink {
    starts: std::sync::atomic::AtomicUsize,
    finishes: std::sync::atomic::AtomicUsize,
    aborts: std::sync::atomic::AtomicUsize,
}

#[derive(Default)]
struct GatedSink {
    gate: (Mutex<bool>, std::sync::Condvar),
    entered: std::sync::atomic::AtomicBool,
    finishes: std::sync::atomic::AtomicUsize,
    aborts: std::sync::atomic::AtomicUsize,
}

impl GatedSink {
    fn wait_until_entered(&self) {
        let started = Instant::now();
        while !self.entered.load(std::sync::atomic::Ordering::SeqCst) {
            assert!(started.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn release(&self) {
        let (lock, wake) = &self.gate;
        *lock.lock().unwrap() = true;
        wake.notify_all();
    }
}

impl ReceiveOutputSink for GatedSink {
    fn start_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        Ok(())
    }

    fn write_chunk(&self, _relative_path: String, _bytes: Vec<u8>) -> Result<(), VnidropError> {
        self.entered
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let (lock, wake) = &self.gate;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = wake.wait(released).unwrap();
        }
        Ok(())
    }

    fn finish_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        self.finishes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn abort_file(&self, _relative_path: String, _reason: String) -> Result<(), VnidropError> {
        self.aborts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

impl ReceiveOutputSinkV2 for GatedSink {
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

impl ReceiveOutputSink for FailingFinishSink {
    fn start_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        self.starts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn write_chunk(&self, _relative_path: String, _bytes: Vec<u8>) -> Result<(), VnidropError> {
        Ok(())
    }

    fn finish_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        self.finishes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(VnidropError::filesystem(anyhow::anyhow!("finish failed")))
    }

    fn abort_file(&self, _relative_path: String, _reason: String) -> Result<(), VnidropError> {
        self.aborts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

impl ReceiveOutputSinkV2 for FailingFinishSink {
    fn start_file(&self, relative_path: String) -> Result<(), VnidropError> {
        ReceiveOutputSink::start_file(self, relative_path)
    }

    fn write_chunk(&self, relative_path: String, bytes: Vec<u8>) -> Result<(), VnidropError> {
        ReceiveOutputSink::write_chunk(self, relative_path, bytes)
    }

    fn finish_file(&self, relative_path: String) -> Result<PublishedOutput, VnidropError> {
        ReceiveOutputSink::finish_file(self, relative_path)?;
        unreachable!()
    }

    fn abort_file(&self, relative_path: String, reason: String) -> Result<(), VnidropError> {
        ReceiveOutputSink::abort_file(self, relative_path, reason)
    }
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
fn targeted_sink_finish_failure_is_the_only_terminal_callback() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_075);

    for use_v2 in [false, true] {
        let transfer = approve_one(&alice, &bob, b"sink failure", "sink.txt");
        let sink = Arc::new(FailingFinishSink::default());
        let result = if use_v2 {
            bob.core()
                .receive_targeted_transfer_with_output_sink_v2(transfer.id, sink.clone())
        } else {
            bob.core()
                .receive_targeted_transfer_with_output_sink(transfer.id, sink.clone())
        };
        assert!(result.is_err());
        assert_eq!(sink.starts.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(sink.finishes.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(sink.aborts.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}

#[test]
fn targeted_receive_rejects_a_concurrent_second_pull() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_076);
    let transfer = approve_one(&alice, &bob, b"concurrent pull", "sink.txt");
    let sink = Arc::new(GatedSink::default());
    let bob_core = bob.core();
    let transfer_id = transfer.id.clone();
    let sink_for_thread = sink.clone();
    let receive = std::thread::spawn(move || {
        bob_core.receive_targeted_transfer_with_output_sink(transfer_id, sink_for_thread)
    });
    sink.wait_until_entered();
    let second = bob.core().resume_targeted_transfer(
        transfer.id,
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(matches!(
        second,
        Err(VnidropError::InvalidTransition { .. })
    ));
    sink.release();
    receive.join().unwrap().unwrap();
}

#[test]
fn targeted_cancel_while_waiting_for_transfer_slot_never_publishes() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_077);
    let transfer = approve_one(&alice, &bob, b"slot-starved payload", "queued.txt");
    let release_slots = bob.core().hold_all_transfer_slots_for_test();
    let output = tempfile::tempdir().unwrap();
    let output_path = output.path().to_string_lossy().into_owned();
    let bob_core = bob.core();
    let transfer_id = transfer.id.clone();
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
    let receive = std::thread::spawn(move || {
        let result = bob_core.receive_targeted_transfer(transfer_id, output_path);
        finished_tx.send(result).unwrap();
    });

    let started = Instant::now();
    loop {
        let state = bob
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .state;
        if state == TargetedTransferState::Transferring {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "targeted receive never queued behind transfer limiter"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    bob.core()
        .cancel_targeted_transfer(transfer.id.clone())
        .unwrap();
    let receive_result = finished_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("queued targeted receive must observe cancel before a slot is released");
    assert!(receive_result.is_err());
    let _ = release_slots.send(());
    receive.join().unwrap();
    assert!(!output.path().join("queued.txt").exists());
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
}

#[test]
fn targeted_cancel_aborts_each_sink_exactly_once() {
    for use_v2 in [false, true] {
        let alice = ProtectedNode::new();
        let bob = ProtectedNode::new();
        establish_saved(&alice, &bob, if use_v2 { 11_078 } else { 11_077 });
        let transfer = approve_one(&alice, &bob, b"cancel sink", "sink.txt");
        let sink = Arc::new(GatedSink::default());
        let bob_core = bob.core();
        let transfer_id = transfer.id.clone();
        let sink_for_thread = sink.clone();
        let receive = std::thread::spawn(move || {
            if use_v2 {
                bob_core.receive_targeted_transfer_with_output_sink_v2(transfer_id, sink_for_thread)
            } else {
                bob_core.receive_targeted_transfer_with_output_sink(transfer_id, sink_for_thread)
            }
        });
        sink.wait_until_entered();
        bob.core().cancel_targeted_transfer(transfer.id).unwrap();
        sink.release();
        assert!(receive.join().unwrap().is_err());
        assert_eq!(sink.finishes.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(sink.aborts.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

#[test]
fn forgetting_saved_sender_aborts_active_targeted_receive() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_079);
    let transfer = approve_one(&alice, &bob, b"forget during receive", "sink.txt");
    let sink = Arc::new(GatedSink::default());
    let bob_core = bob.core();
    let transfer_id = transfer.id.clone();
    let receive_sink = sink.clone();
    let receive = std::thread::spawn(move || {
        bob_core.receive_targeted_transfer_with_output_sink(transfer_id, receive_sink)
    });
    sink.wait_until_entered();

    bob.core()
        .forget_saved_device(alice.core().status().endpoint_id)
        .unwrap();
    sink.release();
    assert!(receive.join().unwrap().is_err());
    assert_eq!(sink.finishes.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(sink.aborts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
    );
}

#[test]
fn sender_cancel_stops_online_receiver_before_publish() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_080);
    let transfer = approve_one(&alice, &bob, b"sender cancel", "sink.txt");
    let sink = Arc::new(GatedSink::default());
    let bob_core = bob.core();
    let transfer_id = transfer.id.clone();
    let receive_sink = sink.clone();
    let receive = std::thread::spawn(move || {
        bob_core.receive_targeted_transfer_with_output_sink(transfer_id, receive_sink)
    });
    sink.wait_until_entered();

    alice
        .core()
        .cancel_targeted_transfer(transfer.id.clone())
        .unwrap();
    sink.release();
    assert!(receive.join().unwrap().is_err());
    assert_eq!(sink.finishes.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(sink.aborts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        bob.core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Cancelled
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

#[test]
fn approval_secret_failure_keeps_durable_consent_retryable() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    establish_saved(&alice, &bob, 11_081);
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("secret-failure.txt");
    std::fs::write(&source_path, b"secret failure").unwrap();
    let sender = alice.core();
    let receiver_id = bob.core().status().endpoint_id;
    let create = std::thread::spawn(move || {
        sender.create_targeted_transfer(
            receiver_id,
            vec![targeted_source(&source_path)],
            Some("secret-failure.txt".to_string()),
        )
    });
    let offer = wait_for_pending_offer(&bob.core());
    bob.secret_store
        .fail_with(Some(ReferenceStoreFailure::Unavailable));
    bob.core()
        .accept_targeted_offer_without_waiting_for_test(offer.transfer_id.clone())
        .unwrap();
    let transfer = create.join().unwrap().unwrap();
    let started = Instant::now();
    while alice
        .core()
        .targeted_authorization_delivery_attempts_for_test()
        == 0
    {
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(bob
        .core()
        .get_targeted_transfer(offer.transfer_id.clone())
        .unwrap()
        .is_none());
    bob.secret_store.fail_with(None);
    let started = Instant::now();
    loop {
        if bob
            .core()
            .get_targeted_transfer(offer.transfer_id.clone())
            .unwrap()
            .is_some_and(|row| row.state == TargetedTransferState::Approved)
        {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(transfer.state, TargetedTransferState::Approved);
}
