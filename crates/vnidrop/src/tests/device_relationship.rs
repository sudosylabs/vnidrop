use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    secure_secret::FaultInjectingSecretStore, CoreEvent, CoreEventSink, DeviceRelationshipState,
    ShareMetadataInput, ShareSource, SourceKind, TransferAccessMode, VnidropCore,
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
    _data_dir: tempfile::TempDir,
    core: Arc<VnidropCore>,
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
            store,
        )
        .expect("protected test core");
        Self {
            _data_dir: data_dir,
            core,
        }
    }
}

impl Drop for ProtectedNode {
    fn drop(&mut self) {
        self.core.shutdown();
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
    let share = share_path(&sender.core, &source_path, transfer_id);
    let output_dir = output_dir.path().to_string_lossy().to_string();
    let receiver_core = receiver.core.clone();
    let ticket = share.ticket.clone();
    let handle = std::thread::spawn(move || {
        receiver_core.receive(ticket, output_dir, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(&sender.core, share.transfer_id);
    sender
        .core
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();

    let started = Instant::now();
    let peer = receiver.core.status().endpoint_id.clone();
    loop {
        if sender
            .core
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

#[test]
fn mutual_consent_reaches_saved_after_both_grants_and_acknowledgement() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let alice_id = alice.core.status().endpoint_id.clone();
    let bob_id = bob.core.status().endpoint_id.clone();

    complete_transfer(&alice, &bob, 80_001);

    assert!(alice
        .core
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());

    wait_for_relationship(
        &alice.core,
        &bob_id,
        DeviceRelationshipState::PendingOutgoing,
    );
    wait_for_relationship(
        &bob.core,
        &alice_id,
        DeviceRelationshipState::PendingIncoming,
    );
    assert!(
        alice.core.list_saved_devices().unwrap().is_empty(),
        "pending outgoing must not surface as a saved device"
    );
    assert!(
        bob.core.list_saved_devices().unwrap().is_empty(),
        "pending incoming must not surface as a saved device"
    );

    assert!(bob
        .core
        .respond_to_device_pairing(alice_id.clone(), true)
        .unwrap());

    wait_for_relationship(&alice.core, &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(&bob.core, &alice_id, DeviceRelationshipState::Saved);

    let alice_saved = alice.core.list_saved_devices().unwrap();
    let bob_saved = bob.core.list_saved_devices().unwrap();
    assert_eq!(alice_saved.len(), 1);
    assert_eq!(alice_saved[0].endpoint_id, bob_id);
    assert_eq!(bob_saved.len(), 1);
    assert_eq!(bob_saved[0].endpoint_id, alice_id);
}

#[test]
fn declining_pending_incoming_consumes_eligibility_and_never_saves() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let alice_id = alice.core.status().endpoint_id.clone();
    let bob_id = bob.core.status().endpoint_id.clone();
    complete_transfer(&alice, &bob, 80_010);

    assert!(alice
        .core
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());
    wait_for_relationship(
        &bob.core,
        &alice_id,
        DeviceRelationshipState::PendingIncoming,
    );

    assert!(bob
        .core
        .respond_to_device_pairing(alice_id.clone(), false)
        .unwrap());
    assert!(bob.core.list_saved_devices().unwrap().is_empty());
    assert!(alice.core.list_saved_devices().unwrap().is_empty());
    assert!(bob
        .core
        .list_pairing_eligibilities()
        .unwrap()
        .iter()
        .all(|entry| entry.peer_endpoint_id != alice_id));
    // Declined eligibility cannot prompt again without a new qualifying transfer.
    assert!(!alice.core.request_saved_device_pairing(bob_id).unwrap());
}

#[test]
fn repeated_consent_is_idempotent_and_does_not_duplicate_saved_rows() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let alice_id = alice.core.status().endpoint_id.clone();
    let bob_id = bob.core.status().endpoint_id.clone();
    complete_transfer(&alice, &bob, 80_020);

    assert!(alice
        .core
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());
    wait_for_relationship(
        &bob.core,
        &alice_id,
        DeviceRelationshipState::PendingIncoming,
    );
    assert!(bob
        .core
        .respond_to_device_pairing(alice_id.clone(), true)
        .unwrap());
    wait_for_relationship(&alice.core, &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(&bob.core, &alice_id, DeviceRelationshipState::Saved);

    assert!(bob.core.respond_to_device_pairing(alice_id, true).unwrap());
    assert_eq!(alice.core.list_saved_devices().unwrap().len(), 1);
    assert_eq!(bob.core.list_saved_devices().unwrap().len(), 1);
    assert_eq!(alice.core.list_device_relationships().unwrap().len(), 1);
    assert_eq!(bob.core.list_device_relationships().unwrap().len(), 1);
}

#[test]
fn simultaneous_initiation_merges_into_one_relationship_per_side() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let alice_id = alice.core.status().endpoint_id.clone();
    let bob_id = bob.core.status().endpoint_id.clone();
    complete_transfer(&alice, &bob, 80_030);

    let alice_core = alice.core.clone();
    let bob_core = bob.core.clone();
    let bob_id_clone = bob_id.clone();
    let alice_id_clone = alice_id.clone();
    let alice_handle =
        std::thread::spawn(move || alice_core.request_saved_device_pairing(bob_id_clone));
    let bob_handle =
        std::thread::spawn(move || bob_core.request_saved_device_pairing(alice_id_clone));
    let alice_ok = alice_handle.join().unwrap().unwrap();
    let bob_ok = bob_handle.join().unwrap().unwrap();
    assert!(alice_ok || bob_ok);

    wait_for_relationship(&alice.core, &bob_id, DeviceRelationshipState::Saved);
    wait_for_relationship(&bob.core, &alice_id, DeviceRelationshipState::Saved);
    assert_eq!(alice.core.list_device_relationships().unwrap().len(), 1);
    assert_eq!(bob.core.list_device_relationships().unwrap().len(), 1);
    assert_eq!(alice.core.list_saved_devices().unwrap().len(), 1);
    assert_eq!(bob.core.list_saved_devices().unwrap().len(), 1);
}

#[test]
fn request_timeout_leaves_recoverable_pending_not_saved() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    complete_transfer(&alice, &bob, 80_040);

    // Shut down Bob so Alice's pairing request cannot complete on the wire.
    bob.core.shutdown();

    assert!(alice
        .core
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());
    wait_for_relationship(
        &alice.core,
        &bob_id,
        DeviceRelationshipState::PendingOutgoing,
    );
    assert!(
        alice.core.list_saved_devices().unwrap().is_empty(),
        "timed-out pairing must not surface as saved"
    );
}
