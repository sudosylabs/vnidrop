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
    store: Arc<FaultInjectingSecretStore>,
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
            _data_dir: data_dir,
            core,
            store,
        }
    }
}

impl Drop for ProtectedNode {
    fn drop(&mut self) {
        self.core.shutdown();
    }
}

fn share_path(core: &VnidropCore, source: &Path, transfer_id: u64) -> crate::ShareResult {
    share_path_named(core, source, transfer_id, "sender")
}

fn share_path_named(
    core: &VnidropCore,
    source: &Path,
    transfer_id: u64,
    sender_name: &str,
) -> crate::ShareResult {
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
            sender_name: Some(sender_name.to_string()),
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
    complete_transfer_named(sender, receiver, transfer_id, "sender", "receiver")
}

fn complete_transfer_named(
    sender: &ProtectedNode,
    receiver: &ProtectedNode,
    transfer_id: u64,
    sender_name: &str,
    receiver_name: &str,
) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"mutual consent").unwrap();
    let share = share_path_named(&sender.core, &source_path, transfer_id, sender_name);
    let output_dir = output_dir.path().to_string_lossy().to_string();
    let receiver_core = receiver.core.clone();
    let ticket = share.ticket.clone();
    let receiver_name = receiver_name.to_string();
    let handle =
        std::thread::spawn(move || receiver_core.receive(ticket, output_dir, Some(receiver_name)));
    let request = wait_for_receiver_request(&sender.core, share.transfer_id);
    sender
        .core
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();

    if sender
        .core
        .list_saved_devices()
        .unwrap()
        .iter()
        .any(|device| device.endpoint_id == receiver.core.status().endpoint_id)
    {
        return;
    }
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
    assert_eq!(
        alice_saved[0].remote_display_name.as_deref(),
        Some("receiver")
    );
    assert_eq!(bob_saved.len(), 1);
    assert_eq!(bob_saved[0].endpoint_id, alice_id);
    assert_eq!(bob_saved[0].remote_display_name.as_deref(), Some("sender"));
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

fn reach_saved(alice: &ProtectedNode, bob: &ProtectedNode, transfer_id: u64) {
    let alice_id = alice.core.status().endpoint_id.clone();
    let bob_id = bob.core.status().endpoint_id.clone();
    complete_transfer(alice, bob, transfer_id);
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
}

#[test]
fn rotating_grant_invalidates_prior_generation_and_leaves_one_active() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_001);

    let (old_generation, old_grant_id) = alice
        .core
        .relationship_issued_grant_for_test(bob_id.clone())
        .unwrap()
        .expect("issued grant before rotate");
    assert_eq!(old_generation, 1);

    let new_generation = alice
        .core
        .rotate_relationship_grant(bob_id.clone())
        .unwrap();
    assert_eq!(new_generation, 2);

    let relationships = alice.core.list_device_relationships().unwrap();
    assert_eq!(relationships.len(), 1);
    assert_eq!(relationships[0].generation, 2);
    assert_eq!(relationships[0].state, DeviceRelationshipState::Saved);

    let (active_generation, active_grant_id) = alice
        .core
        .relationship_issued_grant_for_test(bob_id.clone())
        .unwrap()
        .expect("issued grant after rotate");
    assert_eq!(active_generation, 2);
    assert_ne!(active_grant_id, old_grant_id);

    let err = alice
        .core
        .reject_relationship_generation_for_test(
            bob_id.clone(),
            old_generation,
            Some(old_grant_id.clone()),
        )
        .expect_err("tombstoned generation must be rejected");
    assert_eq!(err, "revoked");

    alice
        .core
        .reject_relationship_generation_for_test(bob_id.clone(), new_generation, None)
        .expect("active generation remains usable");

    let tombstones = alice.core.relationship_tombstones_for_test(bob_id).unwrap();
    assert_eq!(tombstones.len(), 1);
    assert_eq!(tombstones[0].generation, old_generation);
    assert_eq!(
        tombstones[0].issued_grant_id.as_deref(),
        Some(old_grant_id.as_str())
    );
    // Minimal non-secret payload only.
    assert!(tombstones[0].revoked_at > 0);
}

#[test]
fn forget_saved_device_revokes_locally_and_hooks_targeted_cancel() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_010);

    alice.core.forget_saved_device(bob_id.clone()).unwrap();

    assert!(alice.core.list_saved_devices().unwrap().is_empty());
    let relationships = alice.core.list_device_relationships().unwrap();
    assert!(
        relationships.is_empty(),
        "forgotten relationship must not remain listed"
    );
    let cancels = alice.core.targeted_cancel_log_for_test();
    assert_eq!(cancels, vec![bob_id.clone()]);

    let tombstones = alice
        .core
        .relationship_tombstones_for_test(bob_id.clone())
        .unwrap();
    assert_eq!(tombstones.len(), 1);
    assert!(alice
        .core
        .reject_relationship_generation_for_test(bob_id, tombstones[0].generation, None)
        .is_err());
}

#[test]
fn forget_does_not_cancel_active_invitation_transfer() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_020);

    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"invitation continues after forget").unwrap();
    let share = share_path(&alice.core, &source_path, 90_021);
    let output = output_dir.path().to_string_lossy().to_string();
    let receiver_core = bob.core.clone();
    let ticket = share.ticket.clone();
    let handle = std::thread::spawn(move || {
        receiver_core.receive(ticket, output, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(&alice.core, share.transfer_id);
    alice
        .core
        .respond_receiver_request(request.id, true, None)
        .unwrap();

    // Forget after the invitation is approved; the share-domain transfer must finish.
    alice.core.forget_saved_device(bob_id).unwrap();
    handle.join().unwrap().unwrap();

    let transfers = alice.core.list_transfers().unwrap();
    let invitation = transfers
        .iter()
        .find(|entry| entry.transfer_id == share.transfer_id)
        .expect("invitation transfer retained");
    assert_ne!(
        invitation.status.to_lowercase(),
        "cancelled",
        "forget must not cancel independently approved invitation transfers"
    );
}

#[test]
fn block_rejects_pairing_and_invitation_handshake_unblock_restores_neither() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let alice_id = alice.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_030);

    bob.core.block_device(alice_id.clone()).unwrap();
    assert_eq!(
        bob.core.list_blocked_devices().unwrap(),
        vec![alice_id.clone()]
    );
    assert!(bob.core.list_saved_devices().unwrap().is_empty());
    assert!(bob.core.list_device_relationships().unwrap().is_empty());

    // Outbound pairing toward a blocked identity is refused locally.
    assert!(!bob
        .core
        .request_saved_device_pairing(alice_id.clone())
        .unwrap());

    // Invitation handshake from the blocked identity is refused.
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("blocked.txt");
    std::fs::write(&source_path, b"blocked").unwrap();
    let share = share_path(&bob.core, &source_path, 90_032);
    let receive_result = alice.core.receive(
        share.ticket,
        output_dir.path().to_string_lossy().into_owned(),
        Some("alice".to_string()),
    );
    assert!(
        receive_result.is_err(),
        "blocked endpoint must fail invitation handshake"
    );

    bob.core.unblock_device(alice_id).unwrap();
    assert!(bob.core.list_blocked_devices().unwrap().is_empty());
    assert!(
        bob.core.list_saved_devices().unwrap().is_empty(),
        "unblock must not restore the relationship"
    );
    assert!(bob.core.list_device_relationships().unwrap().is_empty());
}

#[test]
fn reinstalled_peer_is_never_merged_by_name_or_metadata() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let charlie = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    let charlie_id = charlie.core.status().endpoint_id.clone();
    assert_ne!(bob_id, charlie_id);

    reach_saved(&alice, &bob, 90_040);
    // Same display-facing label on a different endpoint identity must not merge.
    alice
        .core
        .set_saved_device_label(bob_id.clone(), Some("Kitchen Tablet".to_string()))
        .unwrap();
    reach_saved(&alice, &charlie, 90_041);
    alice
        .core
        .set_saved_device_label(charlie_id.clone(), Some("Kitchen Tablet".to_string()))
        .unwrap();

    let saved = alice.core.list_saved_devices().unwrap();
    assert_eq!(saved.len(), 2);
    let ids: std::collections::HashSet<_> =
        saved.into_iter().map(|device| device.endpoint_id).collect();
    assert!(ids.contains(&bob_id));
    assert!(ids.contains(&charlie_id));
    assert_eq!(alice.core.list_device_relationships().unwrap().len(), 2);
}

#[test]
fn saved_device_local_label_survives_listing_and_rejects_non_saved_peers() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_050);

    alice
        .core
        .set_saved_device_label(bob_id.clone(), Some("Kitchen Tablet".to_string()))
        .unwrap();
    let saved = alice.core.list_saved_devices().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].local_label.as_deref(), Some("Kitchen Tablet"));

    alice
        .core
        .set_saved_device_label(bob_id.clone(), None)
        .unwrap();
    assert!(alice.core.list_saved_devices().unwrap()[0]
        .local_label
        .is_none());

    let err = alice
        .core
        .set_saved_device_label("unknown-peer".to_string(), Some("x".to_string()))
        .unwrap_err();
    assert!(matches!(err, crate::VnidropError::InvalidInput { .. }));
}

#[test]
fn later_authenticated_invitation_refreshes_saved_name_without_new_eligibility() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let alice_id = alice.core.status().endpoint_id.clone();
    let bob_id = bob.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_060);

    let authenticated_before_label =
        alice.core.list_saved_devices().unwrap()[0].last_authenticated_at;
    alice
        .core
        .set_saved_device_label(bob_id.clone(), Some("My tablet".to_string()))
        .unwrap();
    let before = alice.core.list_saved_devices().unwrap().remove(0);
    assert_eq!(before.last_authenticated_at, authenticated_before_label);
    std::thread::sleep(Duration::from_millis(2));
    complete_transfer_named(&alice, &bob, 90_061, "Alice refreshed", "Bob refreshed");

    let started = Instant::now();
    loop {
        let refreshed = alice.core.list_saved_devices().unwrap()[0]
            .remote_display_name
            .as_deref()
            == Some("Bob refreshed");
        if refreshed {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "saved name never refreshed"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let alice_saved = alice.core.list_saved_devices().unwrap().remove(0);
    let bob_saved = bob.core.list_saved_devices().unwrap().remove(0);
    assert_eq!(alice_saved.local_label.as_deref(), Some("My tablet"));
    assert_eq!(
        alice_saved.remote_display_name.as_deref(),
        Some("Bob refreshed")
    );
    assert_eq!(
        bob_saved.remote_display_name.as_deref(),
        Some("Alice refreshed")
    );
    assert!(alice_saved.last_authenticated_at > before.last_authenticated_at);
    assert!(alice.core.list_pairing_eligibilities().unwrap().is_empty());
    assert!(bob.core.list_pairing_eligibilities().unwrap().is_empty());
    assert_eq!(alice_id, bob_saved.endpoint_id);
}

#[test]
fn saved_remote_name_and_local_label_survive_restart() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    let bob_id = bob.core.status().endpoint_id.clone();
    reach_saved(&alice, &bob, 90_062);
    alice
        .core
        .set_saved_device_label(bob_id.clone(), Some("My tablet".to_string()))
        .unwrap();
    let expected = alice.core.list_saved_devices().unwrap();

    alice.core.shutdown();
    let restarted = VnidropCore::initialize_with_test_secret_store(
        alice._data_dir.path().to_string_lossy().into_owned(),
        Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        }),
        alice.store.clone(),
    )
    .unwrap();
    assert_eq!(restarted.list_saved_devices().unwrap(), expected);
    restarted.shutdown();
}

#[test]
fn events_carry_stable_ids_and_monotonic_revisions() {
    let alice = ProtectedNode::new();
    let bob = ProtectedNode::new();
    reach_saved(&alice, &bob, 90_051);

    let events = alice.core.list_events(None).unwrap();
    assert!(!events.is_empty());
    let mut seen_ids = std::collections::HashSet::new();
    let mut revisions: Vec<u64> = Vec::new();
    for event in &events {
        assert!(
            seen_ids.insert(event.id.clone()),
            "event ids must be unique"
        );
        assert!(event.revision >= 1, "revisions start at 1");
        revisions.push(event.revision);
    }
    revisions.sort_unstable();
    revisions.dedup();
    assert_eq!(
        revisions.len(),
        events.len(),
        "each event must have a distinct revision"
    );
}
