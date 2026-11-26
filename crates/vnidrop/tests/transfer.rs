mod support;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use support::{receive_with_response, share_path, CoreGuard, RecordingSink, TestNode};
use vnidrop::{CoreEvent, CoreLimits, VnidropError};

fn wait_for_sender_transfer_event(sender: &TestNode, transfer_id: u64, kind: &str) -> CoreEvent {
    let started = Instant::now();
    loop {
        if let Some(event) = sender.sink.events().into_iter().find(|event| {
            event.transfer_id == Some(transfer_id)
                && event.direction.as_deref() == Some("send")
                && event.phase == "transfer"
                && event.kind == kind
        }) {
            return event;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for sender transfer event {kind}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_receiver_status(
    sender: &TestNode,
    transfer_id: u64,
    status: &str,
) -> vnidrop::ReceiverRequest {
    let started = Instant::now();
    loop {
        if let Some(request) = sender
            .core
            .list_receiver_requests(transfer_id)
            .unwrap()
            .into_iter()
            .find(|request| request.status == status)
        {
            return request;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for receiver status {status}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn transfers_file_between_two_cores() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"hello from vnidrop").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();

    let share = share_path(&sender.core, &source_path, 7, "hello.txt", false);
    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(output_dir.path().join("hello.txt")).unwrap(),
        b"hello from vnidrop"
    );
    let received = receiver
        .core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == 7)
        .unwrap();
    assert!(!received.local_id.is_empty());
    assert_eq!(
        received.peer_id.as_deref(),
        Some(sender.core.status().endpoint_id.as_str())
    );
    let artifacts = receiver.core.list_received_artifacts().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].relative_path, "hello.txt");
    assert_eq!(
        artifacts[0].logical_size,
        b"hello from vnidrop".len() as u64
    );
    assert_eq!(
        artifacts[0].locator,
        output_dir.path().join("hello.txt").to_string_lossy()
    );
    let completed = wait_for_sender_transfer_event(&sender, share.transfer_id, "completed");
    assert!(completed.data_json.contains("\"connection_id\":"));
    assert!(completed.data_json.contains("\"request_id\":"));
    // Production events redact endpoint ids; typed APIs remain the source of truth.
    assert!(!completed
        .data_json
        .contains(receiver.core.status().endpoint_id.as_str()));
    let requests = sender
        .core
        .list_receiver_requests(share.transfer_id)
        .unwrap();
    let key = vnidrop::receiver_progress_key(requests[0].remote_endpoint_id.clone());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&completed.data_json).unwrap()["endpoint_id"],
        key
    );

    assert!(!sender.core.delete_terminal_transfer_history().unwrap());
    assert_eq!(sender.core.list_transfers().unwrap()[0].status, "sharing");
    assert!(receiver.core.delete_terminal_transfer_history().unwrap());
    assert!(receiver.core.list_transfers().unwrap().is_empty());
    assert_eq!(receiver.core.list_received_artifacts().unwrap(), artifacts);
    receiver.core.delete_receive_history().unwrap();
    assert_eq!(receiver.core.list_received_artifacts().unwrap(), artifacts);
}

#[test]
fn receive_rejects_oversized_collection_metadata_before_download() {
    let source = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(source.path(), b"small payload").unwrap();
    let sender = TestNode::new();
    let receiver_dir = tempfile::tempdir().unwrap();
    let receiver_sink = Arc::new(RecordingSink::default());
    let receiver = CoreGuard::start_with_limits(
        receiver_dir.path(),
        receiver_sink.clone(),
        CoreLimits {
            max_path_bytes: 16,
            ..CoreLimits::default()
        },
    );
    let output_dir = tempfile::tempdir().unwrap();
    let share = share_path(
        &sender.core,
        source.path(),
        17,
        "a-long-file-name-that-exceeds-the-receivers-path-limit-by-a-wide-margin.txt",
        false,
    );

    let error = receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.arc(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("remote collection metadata"),
        "{error}"
    );
    let transfers = receiver.list_transfers().unwrap();
    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].status, "failed");
    assert!(!receiver_sink
        .events()
        .iter()
        .any(|event| { event.phase == "download" && event.kind == "found-collection" }));
    assert_eq!(std::fs::read_dir(output_dir.path()).unwrap().count(), 0);
}

#[test]
fn transfers_directory_between_two_cores() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_root = source_dir.path().join("photos");
    std::fs::create_dir_all(source_root.join("nested")).unwrap();
    std::fs::write(source_root.join("cover.txt"), b"cover").unwrap();
    std::fs::write(source_root.join("nested/inside.txt"), b"inside").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();

    let share = share_path(&sender.core, &source_root, 8, "photos", true);
    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(output_dir.path().join("photos/cover.txt")).unwrap(),
        b"cover"
    );
    assert_eq!(
        std::fs::read(output_dir.path().join("photos/nested/inside.txt")).unwrap(),
        b"inside"
    );
}

#[test]
fn receive_refuses_to_overwrite_existing_destination() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("existing.txt");
    let output_path = output_dir.path().join("existing.txt");
    std::fs::write(&source_path, b"new content").unwrap();
    std::fs::write(&output_path, b"keep content").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_path, 27, "existing.txt", false);

    let error = receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap_err();
    assert!(matches!(error, VnidropError::DestinationExists { .. }));
    assert_eq!(std::fs::read(&output_path).unwrap(), b"keep content");
    assert!(std::fs::read_dir(output_dir.path())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".part")));
    let transfer = receiver
        .core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == 27)
        .unwrap();
    assert_eq!(transfer.status, "failed");
    assert!(receiver
        .core
        .list_events(Some(27))
        .unwrap()
        .iter()
        .any(|event| {
            event.phase == "error"
                && event.kind == "failed"
                && event.data_json.contains("\"code\":\"destination_exists\"")
        }));
    let failed = wait_for_receiver_status(&sender, share.transfer_id, "failed");
    assert_eq!(failed.reason.as_deref(), Some("destination_exists"));
    assert!(sender.sink.events().iter().any(|event| {
        event.transfer_id == Some(share.transfer_id)
            && event.phase == "delivery"
            && event.kind == "receiver-failed"
    }));
}

#[test]
fn receive_accepts_the_senders_exact_payload_limit() {
    let fixtures = tempfile::tempdir().unwrap();
    let open = |name| {
        CoreGuard::start_with_limits(
            &fixtures.path().join(name),
            Arc::new(RecordingSink::default()),
            CoreLimits {
                max_total_bytes: 4096,
                ..CoreLimits::default()
            },
        )
    };
    let sender = open("sender");
    let receiver = open("receiver");
    let file = fixtures.path().join("sample");
    std::fs::write(&file, vec![0u8; 4096]).unwrap();
    let share = share_path(&sender, &file, 17, "sample", false);
    sender
        .set_transfer_access_mode(17, vnidrop::TransferAccessMode::Public)
        .unwrap();
    let target = fixtures.path().join("out");
    receiver
        .receive(share.ticket, target.to_string_lossy().to_string(), None)
        .unwrap();
    assert_eq!(receiver.list_transfers().unwrap()[0].status, "done");
    assert_eq!(
        std::fs::read(target.join("sample")).unwrap(),
        vec![0u8; 4096]
    );
}
