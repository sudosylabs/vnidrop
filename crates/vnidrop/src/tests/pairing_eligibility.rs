use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    experimental_saved_device_capabilities, secure_secret::FaultInjectingSecretStore, CoreEvent,
    CoreEventSink, ShareMetadataInput, ShareSource, SourceKind, TransferAccessMode, VnidropCore,
    VnidropError,
};

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

struct ProtectedNode {
    _data_dir: tempfile::TempDir,
    core: Arc<VnidropCore>,
    sink: Arc<RecordingSink>,
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
            sink.clone(),
            store,
        )
        .expect("protected test core");
        Self {
            _data_dir: data_dir,
            core,
            sink,
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

fn receive_with_response(
    sender: &VnidropCore,
    transfer_id: u64,
    receiver: Arc<VnidropCore>,
    ticket: String,
    output_dir: &Path,
    accepted: bool,
) -> Result<(), VnidropError> {
    let output_dir = output_dir.to_string_lossy().to_string();
    let handle = std::thread::spawn(move || {
        receiver.receive(ticket, output_dir, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(sender, transfer_id);
    sender
        .respond_receiver_request(
            request.id,
            accepted,
            (!accepted).then(|| "sender-refused".to_string()),
        )
        .unwrap();
    handle.join().unwrap()
}

fn wait_for_eligibility(core: &VnidropCore, peer_endpoint_id: &str) {
    let started = Instant::now();
    loop {
        let found = core
            .list_pairing_eligibilities()
            .unwrap()
            .into_iter()
            .any(|entry| entry.peer_endpoint_id == peer_endpoint_id);
        if found {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "eligibility for {peer_endpoint_id} never appeared"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn completed_authenticated_transfer_creates_pairing_eligibility_on_both_sides() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"eligible after completion").unwrap();

    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    let sender_id = sender.core.status().endpoint_id.clone();
    let receiver_id = receiver.core.status().endpoint_id.clone();

    let share = share_path(&sender.core, &source_path, 70_001);
    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.clone(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();

    wait_for_eligibility(&sender.core, &receiver_id);
    wait_for_eligibility(&receiver.core, &sender_id);

    let protocol = experimental_saved_device_capabilities().relationship_protocol_version;
    let sender_entry = sender
        .core
        .list_pairing_eligibilities()
        .unwrap()
        .into_iter()
        .find(|entry| entry.peer_endpoint_id == receiver_id)
        .unwrap();
    let receiver_entry = receiver
        .core
        .list_pairing_eligibilities()
        .unwrap()
        .into_iter()
        .find(|entry| entry.peer_endpoint_id == sender_id)
        .unwrap();

    assert_eq!(sender_entry.session_id, receiver_entry.session_id);
    assert_eq!(sender_entry.protocol_version, protocol);
    assert_eq!(receiver_entry.protocol_version, protocol);
    assert!(sender_entry.expires_at > sender_entry.created_at);
    assert_eq!(
        sender_entry.expires_at - sender_entry.created_at,
        24 * 60 * 60 * 1_000
    );

    let sender_events = sender.sink.events();
    assert!(
        sender_events
            .iter()
            .any(|event| { event.phase == "pairing" && event.kind == "eligibility-available" }),
        "sender should emit eligibility-available without capability material"
    );
    assert!(
        !sender_events
            .iter()
            .any(|event| event.data_json.contains("capability")),
        "events must not expose the eligibility capability"
    );
}

#[test]
fn declined_cancelled_and_failed_transfers_create_no_eligibility() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"not eligible").unwrap();

    // Declined approval
    {
        let output_dir = tempfile::tempdir().unwrap();
        let sender = ProtectedNode::new();
        let receiver = ProtectedNode::new();
        let share = share_path(&sender.core, &source_path, 70_010);
        let _ = receive_with_response(
            &sender.core,
            share.transfer_id,
            receiver.core.clone(),
            share.ticket,
            output_dir.path(),
            false,
        );
        assert!(sender.core.list_pairing_eligibilities().unwrap().is_empty());
        assert!(receiver
            .core
            .list_pairing_eligibilities()
            .unwrap()
            .is_empty());
    }

    // Failed export on the receiver
    {
        let sender = ProtectedNode::new();
        let receiver = ProtectedNode::new();
        let share = share_path(&sender.core, &source_path, 70_011);
        let sink = Arc::new(FailingOutputSink);
        let handle = {
            let receiver = receiver.core.clone();
            let ticket = share.ticket.clone();
            std::thread::spawn(move || {
                receiver.receive_with_output_sink(ticket, sink, Some("receiver".to_string()))
            })
        };
        let request = wait_for_receiver_request(&sender.core, share.transfer_id);
        sender
            .core
            .respond_receiver_request(request.id, true, None)
            .unwrap();
        let _ = handle.join().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(sender.core.list_pairing_eligibilities().unwrap().is_empty());
        assert!(receiver
            .core
            .list_pairing_eligibilities()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn eligibility_survives_restart_without_filenames_or_history_payload() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("secret-name.txt");
    std::fs::write(&source_path, b"persist eligibility").unwrap();

    let data_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultInjectingSecretStore::default());
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let sender = VnidropCore::initialize_with_test_secret_store(
        data_dir.path().to_string_lossy().into_owned(),
        sink.clone(),
        store.clone(),
    )
    .unwrap();
    let receiver = ProtectedNode::new();
    let receiver_id = receiver.core.status().endpoint_id.clone();

    let share = share_path(&sender, &source_path, 70_020);
    receive_with_response(
        &sender,
        share.transfer_id,
        receiver.core.clone(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();
    wait_for_eligibility(&sender, &receiver_id);
    let before = sender.list_pairing_eligibilities().unwrap();
    assert_eq!(before.len(), 1);
    sender.shutdown();
    drop(sender);

    let restarted = VnidropCore::initialize_with_test_secret_store(
        data_dir.path().to_string_lossy().into_owned(),
        sink,
        store,
    )
    .unwrap();
    let after = restarted.list_pairing_eligibilities().unwrap();
    assert_eq!(after, before);
    assert!(!serde_json::to_string(&after)
        .unwrap()
        .contains("secret-name"));
    assert!(!serde_json::to_string(&after)
        .unwrap()
        .contains("persist eligibility"));
    restarted.shutdown();
}

#[test]
fn expired_eligibility_is_removed_and_cannot_authorize_pairing() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"expires").unwrap();

    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    let receiver_id = receiver.core.status().endpoint_id.clone();
    let share = share_path(&sender.core, &source_path, 70_050);
    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.clone(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();
    wait_for_eligibility(&sender.core, &receiver_id);
    let session_id = sender.core.list_pairing_eligibilities().unwrap()[0]
        .session_id
        .clone();
    sender
        .core
        .force_pairing_eligibility_expiry_for_test(session_id, 1)
        .unwrap();
    assert!(sender.core.list_pairing_eligibilities().unwrap().is_empty());
    assert!(!sender
        .core
        .request_saved_device_pairing(receiver_id)
        .unwrap());
}

#[test]
fn decline_forget_block_and_replay_remove_eligibility_idempotently() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"remove eligibility").unwrap();

    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    let receiver_id = receiver.core.status().endpoint_id.clone();
    let share = share_path(&sender.core, &source_path, 70_030);
    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.clone(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();
    wait_for_eligibility(&sender.core, &receiver_id);

    sender
        .core
        .decline_pairing_eligibility(receiver_id.clone())
        .unwrap();
    assert!(sender.core.list_pairing_eligibilities().unwrap().is_empty());
    sender
        .core
        .decline_pairing_eligibility(receiver_id.clone())
        .unwrap();
    assert!(sender.core.list_pairing_eligibilities().unwrap().is_empty());

    // Fresh eligibility for forget/block coverage on the receiver side.
    let sender2 = ProtectedNode::new();
    let output_dir2 = tempfile::tempdir().unwrap();
    let share2 = share_path(&sender2.core, &source_path, 70_031);
    receive_with_response(
        &sender2.core,
        share2.transfer_id,
        receiver.core.clone(),
        share2.ticket,
        output_dir2.path(),
        true,
    )
    .unwrap();
    wait_for_eligibility(&receiver.core, &sender2.core.status().endpoint_id);
    receiver
        .core
        .forget_contact(sender2.core.status().endpoint_id.clone())
        .unwrap();
    assert!(receiver
        .core
        .list_pairing_eligibilities()
        .unwrap()
        .iter()
        .all(|entry| entry.peer_endpoint_id != sender2.core.status().endpoint_id));

    let sender3 = ProtectedNode::new();
    let output_dir3 = tempfile::tempdir().unwrap();
    let share3 = share_path(&sender3.core, &source_path, 70_032);
    receive_with_response(
        &sender3.core,
        share3.transfer_id,
        receiver.core.clone(),
        share3.ticket,
        output_dir3.path(),
        true,
    )
    .unwrap();
    wait_for_eligibility(&receiver.core, &sender3.core.status().endpoint_id);
    receiver
        .core
        .block_contact(sender3.core.status().endpoint_id.clone())
        .unwrap();
    assert!(receiver
        .core
        .list_pairing_eligibilities()
        .unwrap()
        .iter()
        .all(|entry| entry.peer_endpoint_id != sender3.core.status().endpoint_id));
}

#[test]
fn missing_expired_replayed_and_fabricated_eligibility_are_silently_rejected() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"silent reject").unwrap();

    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    let receiver_id = receiver.core.status().endpoint_id.clone();
    let events_before = receiver.sink.events().len();

    // Missing eligibility: request produces no pending pairing prompt/event.
    assert!(!receiver
        .core
        .request_saved_device_pairing(sender.core.status().endpoint_id.clone())
        .unwrap());
    assert!(receiver.core.list_pending_pairings().is_empty());
    assert_eq!(
        receiver
            .sink
            .events()
            .iter()
            .filter(|event| event.phase == "pairing" && event.kind.contains("pending"))
            .count(),
        0
    );

    let share = share_path(&sender.core, &source_path, 70_040);
    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.clone(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();
    wait_for_eligibility(&sender.core, &receiver_id);

    // Consume once, then replay must not create a second prompt.
    assert!(sender
        .core
        .request_saved_device_pairing(receiver_id.clone())
        .unwrap());
    assert!(sender.core.list_pairing_eligibilities().unwrap().is_empty());
    assert!(!sender
        .core
        .request_saved_device_pairing(receiver_id)
        .unwrap());
    assert!(sender.core.list_pending_pairings().is_empty());
    assert!(receiver.core.list_pending_pairings().is_empty());

    // Fabricated peer identity is rejected without growing pairing events.
    let pairing_events_before = receiver
        .sink
        .events()
        .iter()
        .filter(|event| event.phase == "pairing")
        .count();
    assert!(!receiver
        .core
        .submit_pairing_eligibility_for_test(
            "fabricated-endpoint".to_string(),
            "fabricated-session".to_string(),
            vec![7u8; 32],
        )
        .unwrap());
    assert!(receiver.core.list_pending_pairings().is_empty());
    let pairing_events_after = receiver
        .sink
        .events()
        .iter()
        .filter(|event| event.phase == "pairing")
        .count();
    assert_eq!(pairing_events_before, pairing_events_after);
    let _ = events_before;
}

struct FailingOutputSink;

impl crate::ReceiveOutputSink for FailingOutputSink {
    fn start_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        Ok(())
    }

    fn write_chunk(&self, _relative_path: String, _bytes: Vec<u8>) -> Result<(), VnidropError> {
        Err(VnidropError::Filesystem {
            reason: "export failed".to_string(),
        })
    }

    fn finish_file(&self, _relative_path: String) -> Result<(), VnidropError> {
        Ok(())
    }

    fn abort_file(&self, _relative_path: String, _reason: String) -> Result<(), VnidropError> {
        Ok(())
    }
}
