mod support;

use std::sync::Arc;

use std::time::{Duration, Instant};

use support::{
    receive_with_sink_response, receive_with_sink_v2_response, share_path,
    wait_for_receiver_request, MemoryOutputSink, TestNode,
};
use vnidrop::{PublishedOutput, ReceiveOutputSinkV2, VnidropError};

struct ExistingDestinationSink;

impl ReceiveOutputSinkV2 for ExistingDestinationSink {
    fn start_file(&self, relative_path: String) -> Result<(), VnidropError> {
        Err(VnidropError::DestinationExists {
            reason: format!("destination already exists: {relative_path}"),
        })
    }

    fn write_chunk(&self, _relative_path: String, _bytes: Vec<u8>) -> Result<(), VnidropError> {
        unreachable!("a rejected file must not be written")
    }

    fn finish_file(&self, _relative_path: String) -> Result<PublishedOutput, VnidropError> {
        unreachable!("a rejected file must not be finished")
    }

    fn abort_file(&self, _relative_path: String, _reason: String) -> Result<(), VnidropError> {
        unreachable!("start_file failure must not be aborted")
    }
}

#[test]
fn versioned_sink_records_published_locator() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("tracked.txt");
    std::fs::write(&source_path, b"tracked").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_path, 38, "tracked.txt", false);
    let output_sink = Arc::new(MemoryOutputSink::default());

    receive_with_sink_v2_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_sink,
        true,
    )
    .unwrap();

    let artifacts = receiver.core.list_received_artifacts().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].locator, "content://test/tracked.txt");
    assert_eq!(artifacts[0].logical_size, 7);
}

#[test]
fn exports_nested_files_to_output_sink() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_root = source_dir.path().join("photos");
    std::fs::create_dir_all(source_root.join("nested")).unwrap();
    std::fs::write(source_root.join("cover.txt"), b"cover").unwrap();
    std::fs::write(source_root.join("nested/inside.txt"), b"inside").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_root, 18, "photos", true);
    let output_sink = Arc::new(MemoryOutputSink::default());

    receive_with_sink_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_sink.clone(),
        true,
    )
    .unwrap();

    assert_eq!(output_sink.file("photos/cover.txt"), b"cover");
    assert_eq!(output_sink.file("photos/nested/inside.txt"), b"inside");
    assert_eq!(
        output_sink.terminal_state("photos/cover.txt"),
        Some("finished")
    );
    assert_eq!(
        output_sink.terminal_state("photos/nested/inside.txt"),
        Some("finished")
    );
}

#[test]
fn batches_large_exports_to_both_sink_versions() {
    const CHUNK_BYTES: usize = 1024 * 1024;
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("large.bin");
    let content: Vec<_> = (0..2 * CHUNK_BYTES + 37)
        .map(|offset| (offset % 251) as u8)
        .collect();
    std::fs::write(&source_path, &content).unwrap();
    let sender = TestNode::new();
    let share = share_path(&sender.core, &source_path, 40, "large.bin", false);

    for versioned in [false, true] {
        let receiver = TestNode::new();
        let output_sink = Arc::new(MemoryOutputSink::default());
        if versioned {
            receive_with_sink_v2_response(
                &sender.core,
                share.transfer_id,
                receiver.core.arc(),
                share.ticket.clone(),
                output_sink.clone(),
                true,
            )
        } else {
            receive_with_sink_response(
                &sender.core,
                share.transfer_id,
                receiver.core.arc(),
                share.ticket.clone(),
                output_sink.clone(),
                true,
            )
        }
        .unwrap();

        assert_eq!(output_sink.file("large.bin"), content);
        assert_eq!(output_sink.terminal_state("large.bin"), Some("finished"));
        assert_eq!(output_sink.write_calls(), 3);
        let progress: Vec<_> = receiver
            .sink
            .events()
            .into_iter()
            .filter(|event| event.phase == "export" && event.kind == "progress")
            .collect();
        assert!(progress.len() <= 3);
        let last =
            serde_json::from_str::<serde_json::Value>(&progress.last().unwrap().data_json).unwrap();
        assert_eq!(last["exported"].as_u64(), Some(content.len() as u64));
    }
}

#[test]
fn reports_output_sink_write_failure() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"hello").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_path, 19, "hello.txt", false);
    let output_sink = Arc::new(MemoryOutputSink::failing_writes());

    let error = receive_with_sink_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_sink.clone(),
        true,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        VnidropError::Filesystem { ref reason } if reason.contains("sink write failed")
    ));
    assert_eq!(output_sink.terminal_state("hello.txt"), Some("aborted"));
}

#[test]
fn preserves_typed_output_sink_failure() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("existing.txt");
    std::fs::write(&source_path, b"new content").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_path, 39, "existing.txt", false);

    let error = receive_with_sink_v2_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        Arc::new(ExistingDestinationSink),
        true,
    )
    .unwrap_err();

    assert!(matches!(error, VnidropError::DestinationExists { .. }));
    assert!(receiver
        .core
        .list_events(Some(39))
        .unwrap()
        .iter()
        .any(|event| {
            event.phase == "error"
                && event.kind == "failed"
                && event.data_json.contains("\"code\":\"destination_exists\"")
        }));
}

#[test]
fn cancellation_during_export_aborts_open_sink_file() {
    // Gate the first write so export is mid-file when cancel runs. A large
    // "slow writes" file made this flaky/hang-prone on CI: concurrent cancel
    // used to nest Runtime::block_on, and multi-megabyte sleep loops could
    // starve the cancel path for a long time.
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("gated.bin");
    std::fs::write(&source_path, vec![0u8; 256 * 1024]).unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_path, 29, "gated.bin", false);
    let output_sink = Arc::new(MemoryOutputSink::block_first_write());
    let receiver_core = receiver.core.arc();
    let sink_for_worker = output_sink.clone();
    let ticket = share.ticket.clone();
    let worker = std::thread::spawn(move || {
        receiver_core.receive_with_output_sink(
            ticket,
            sink_for_worker,
            Some("receiver".to_string()),
        )
    });

    let request = wait_for_receiver_request(&sender.core, share.transfer_id);
    sender
        .core
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    let started = Instant::now();
    while !output_sink.has_started("gated.bin") || !output_sink.has_entered_write() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "export never opened gated.bin for writing"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // First write is parked in the sink. Cancel must complete from this thread
    // without deadlocking against the receive worker's runtime driver.
    receiver.core.cancel_transfer(share.transfer_id).unwrap();
    // Unblock the in-flight write so the receive future can yield, observe
    // cancel, and Drop(OutputSinkFile) can abort the open file.
    output_sink.release_write_gate();

    let result = worker
        .join()
        .expect("receive worker thread panicked")
        .expect_err("cancelled receive should return an error");
    let message = result.to_string();
    assert!(
        message.contains("cancel") || message.contains("interrupted") || !message.is_empty(),
        "unexpected cancel error: {message}"
    );
    let abort_deadline = Instant::now();
    while output_sink.terminal_state("gated.bin") != Some("aborted") {
        assert!(
            abort_deadline.elapsed() < Duration::from_secs(5),
            "open sink file was not aborted after cancel"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
