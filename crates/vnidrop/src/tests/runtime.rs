use std::{sync::Arc, time::Duration};

use iroh_blobs::{
    provider::{
        events::{RequestUpdate, TransferCompleted},
        TransferStats,
    },
    Hash,
};

use crate::{
    repository::{PendingDeliveryReceiptInsert, Repository, TransferUpsert},
    runtime::{consume_request_updates, RequestStreamOutcome},
    transfer_state::{TransferDirection, TransferStatus},
    CoreEvent, CoreEventSink, VnidropCore, VnidropError,
};

struct TestSink;

impl CoreEventSink for TestSink {
    fn on_event(&self, _event: CoreEvent) {}
}

#[test]
fn provider_request_stream_distinguishes_success_from_silent_abort() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let (completed_tx, completed_rx) = irpc::channel::mpsc::channel(1);
        completed_tx
            .send(RequestUpdate::Completed(TransferCompleted {
                stats: Box::new(TransferStats {
                    payload_bytes_sent: 5,
                    other_bytes_sent: 0,
                    other_bytes_read: 0,
                    duration: Duration::ZERO,
                }),
            }))
            .await
            .unwrap();
        drop(completed_tx);
        assert_eq!(
            consume_request_updates(completed_rx, |_| {}).await,
            RequestStreamOutcome::TerminalUpdateReceived
        );

        let (aborted_tx, aborted_rx) = irpc::channel::mpsc::channel::<RequestUpdate>(1);
        drop(aborted_tx);
        assert_eq!(
            consume_request_updates(aborted_rx, |_| {}).await,
            RequestStreamOutcome::Aborted
        );
    });
}

#[test]
fn initializes_and_reports_endpoint() {
    let temp = tempfile::tempdir().unwrap();
    let core = VnidropCore::initialize(
        temp.path().to_string_lossy().to_string(),
        Arc::new(TestSink),
    )
    .unwrap();

    assert!(!core.status().endpoint_id.is_empty());
    core.shutdown();
}

#[test]
fn invalid_receive_ticket_is_typed_and_persisted_as_event() {
    let temp = tempfile::tempdir().unwrap();
    let core = VnidropCore::initialize(
        temp.path().to_string_lossy().to_string(),
        Arc::new(TestSink),
    )
    .unwrap();

    let error = core
        .receive(
            "not-a-ticket".to_string(),
            temp.path().to_string_lossy().to_string(),
            None,
        )
        .unwrap_err();
    assert!(matches!(error, VnidropError::Ticket { .. }));

    let events = core.list_events(None).unwrap();
    assert!(events
        .iter()
        .any(|event| event.phase == "error" && event.kind == "invalid-ticket"));
    core.shutdown();
}

#[test]
fn startup_recovers_interrupted_transfer_and_persists_event() {
    let temp = tempfile::tempdir().unwrap();
    let preparation_runtime = tokio::runtime::Runtime::new().unwrap();
    preparation_runtime.block_on(async {
        let repository = Repository::open(temp.path()).await.unwrap();
        repository
            .insert_transfer(TransferUpsert {
                transfer_id: 91,
                peer_id: None,
                direction: TransferDirection::Receive,
                status: TransferStatus::Receiving,
                transfer_name: Some("interrupted"),
                content_hash: Some("hash"),
                ticket: None,
                file_count: 1,
                total_size: 5,
                access_mode: "approval_required",
            })
            .await
            .unwrap();
    });
    drop(preparation_runtime);

    let core = VnidropCore::initialize(
        temp.path().to_string_lossy().to_string(),
        Arc::new(TestSink),
    )
    .unwrap();
    let transfer = core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == 91)
        .unwrap();
    assert_eq!(transfer.status, "failed");

    let events = core.list_events(Some(91)).unwrap();
    assert!(events.iter().any(|event| {
        event.phase == "recovery"
            && event.kind == "interrupted-transfer-failed"
            && event.data_json.contains("receiving")
    }));
    core.shutdown();
}

#[test]
fn startup_processes_persisted_delivery_receipts() {
    let temp = tempfile::tempdir().unwrap();
    let preparation_runtime = tokio::runtime::Runtime::new().unwrap();
    preparation_runtime.block_on(async {
        let repository = Repository::open(temp.path()).await.unwrap();
        repository
            .start_receive(TransferUpsert {
                transfer_id: 94,
                peer_id: None,
                direction: TransferDirection::Receive,
                status: TransferStatus::Receiving,
                transfer_name: Some("completed receive"),
                content_hash: Some("hash"),
                ticket: None,
                file_count: 1,
                total_size: 5,
                access_mode: "approval_required",
            })
            .await
            .unwrap();
        repository
            .complete_receive_with_pending_receipt(PendingDeliveryReceiptInsert {
                local_transfer_id: 94,
                sender_blob_ticket: "invalid-ticket",
                request_id: "request-94",
                sender_transfer_id: 49,
                token: "receipt-token",
                failure_reason: None,
            })
            .await
            .unwrap();
    });
    drop(preparation_runtime);

    let core = VnidropCore::initialize(
        temp.path().to_string_lossy().to_string(),
        Arc::new(TestSink),
    )
    .unwrap();
    let started = std::time::Instant::now();
    loop {
        if core.list_events(Some(94)).unwrap().iter().any(|event| {
            event.phase == "delivery"
                && event.kind == "receipt-rejected"
                && event.data_json.contains("invalid-sender-ticket")
        }) {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "startup did not process the persisted delivery receipt"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    core.shutdown();
}

#[test]
fn startup_fails_persisted_share_when_root_blob_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let preparation_runtime = tokio::runtime::Runtime::new().unwrap();
    preparation_runtime.block_on(async {
        let repository = Repository::open(temp.path()).await.unwrap();
        let missing_hash = Hash::new([42; 32]).to_string();
        repository
            .insert_transfer(TransferUpsert {
                transfer_id: 92,
                peer_id: None,
                direction: TransferDirection::Send,
                status: TransferStatus::Sharing,
                transfer_name: Some("missing blob"),
                content_hash: Some(&missing_hash),
                ticket: Some("ticket"),
                file_count: 1,
                total_size: 5,
                access_mode: "approval_required",
            })
            .await
            .unwrap();
    });
    drop(preparation_runtime);

    let core = VnidropCore::initialize(
        temp.path().to_string_lossy().to_string(),
        Arc::new(TestSink),
    )
    .unwrap();
    let transfer = core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == 92)
        .unwrap();
    assert_eq!(transfer.status, "failed");
    assert_eq!(core.status().active_shares, 0);
    assert!(core.list_events(Some(92)).unwrap().iter().any(|event| {
        event.phase == "recovery" && event.kind == "share-root-missing-or-corrupt"
    }));
    core.shutdown();
}
