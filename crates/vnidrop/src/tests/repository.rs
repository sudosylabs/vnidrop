use crate::{
    api::{CoreEvent, ReceivedLocatorKind},
    repository::{
        PendingDeliveryReceiptInsert, ReceivedArtifactInsert, ReceiverRequestInsert, Repository,
        TransferUpsert,
    },
    transfer_state::{ReceiverRequestStatus, TransferDirection, TransferStatus},
};

fn transfer(
    transfer_id: u64,
    direction: TransferDirection,
    status: TransferStatus,
) -> TransferUpsert<'static> {
    TransferUpsert {
        transfer_id,
        peer_id: None,
        direction,
        status,
        transfer_name: Some("demo"),
        content_hash: Some("hash"),
        ticket: Some("ticket"),
        file_count: 1,
        total_size: 12,
        access_mode: "approval_required",
    }
}

#[tokio::test]
async fn received_artifacts_survive_history_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .start_receive(transfer(
            91,
            TransferDirection::Receive,
            TransferStatus::Receiving,
        ))
        .await
        .unwrap();
    let local_id = repository.transfer_local_id(91).await.unwrap();
    repository
        .record_received_artifact(ReceivedArtifactInsert {
            transfer_local_id: &local_id,
            protocol_transfer_id: 91,
            relative_path: "folder/file.txt",
            locator_kind: ReceivedLocatorKind::FilesystemPath,
            locator: "/tmp/folder/file.txt",
            logical_size: 12,
        })
        .await
        .unwrap();
    repository
        .transition_transfer_status(91, TransferStatus::Receiving, TransferStatus::Done)
        .await
        .unwrap();

    assert_eq!(repository.delete_receive_history().await.unwrap(), 1);
    let artifacts = repository.list_received_artifacts().await.unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].transfer_local_id, local_id);
    assert_eq!(artifacts[0].logical_size, 12);
}

#[tokio::test]
async fn persists_transfers_and_events_across_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    assert_eq!(repository.schema_version().await.unwrap(), 7);
    repository
        .insert_transfer(transfer(
            7,
            TransferDirection::Send,
            TransferStatus::Sharing,
        ))
        .await
        .unwrap();

    let shares = repository.list_active_shares().await.unwrap();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].transfer_id, 7);
    assert_eq!(shares[0].content_hash, "hash");
    assert_eq!(shares[0].ticket.as_deref(), Some("ticket"));
    assert_eq!(shares[0].access_mode, "approval_required");

    repository
        .insert_event(
            &CoreEvent {
                id: "event-1".to_string(),
                timestamp: 10,
                scope: "transfer".to_string(),
                transfer_id: Some(7),
                direction: Some("send".to_string()),
                phase: "ticket".to_string(),
                kind: "created".to_string(),
                data_json: "{}".to_string(),
            },
            500,
        )
        .await
        .unwrap();

    let transfers = repository.list_transfers().await.unwrap();
    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].transfer_name.as_deref(), Some("demo"));

    let events = repository.list_events(Some(7), 500).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "created");

    drop(repository);
    let reopened = Repository::open(temp.path()).await.unwrap();
    let transfers = reopened.list_transfers().await.unwrap();
    assert_eq!(transfers.len(), 1);
    let events = reopened.list_events(Some(7), 500).await.unwrap();
    assert_eq!(events[0].id, "event-1");
}

#[tokio::test]
async fn receive_completion_persists_delivery_receipt_until_recorded() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .start_receive(transfer(
            93,
            TransferDirection::Receive,
            TransferStatus::Receiving,
        ))
        .await
        .unwrap();
    repository
        .complete_receive_with_pending_receipt(PendingDeliveryReceiptInsert {
            local_transfer_id: 93,
            sender_blob_ticket: "blob-ticket",
            request_id: "request-93",
            sender_transfer_id: 39,
            token: "receipt-token",
            failure_reason: None,
        })
        .await
        .unwrap();

    let transfer = repository
        .list_transfers()
        .await
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == 93)
        .unwrap();
    assert_eq!(transfer.status, "done");
    drop(repository);

    let reopened = Repository::open(temp.path()).await.unwrap();
    let pending = reopened.list_pending_delivery_receipts().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].local_transfer_id, 93);
    assert_eq!(pending[0].sender_transfer_id, 39);
    assert_eq!(pending[0].request_id, "request-93");
    assert_eq!(pending[0].token, "receipt-token");
    reopened
        .delete_pending_delivery_receipt("request-93")
        .await
        .unwrap();
    assert!(reopened
        .list_pending_delivery_receipts()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn receiver_request_can_only_be_resolved_once() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_receiver_request(ReceiverRequestInsert {
            id: "request-1",
            transfer_id: 77,
            remote_endpoint_id: "node-a",
            transfer_name: "demo",
            receiver_name: Some("receiver"),
            receiver_device_name: Some("phone"),
            app_version: "0.1.0",
        })
        .await
        .unwrap();
    repository
        .update_receiver_request_status("request-1", ReceiverRequestStatus::Accepted, None)
        .await
        .unwrap();
    repository
        .set_receiver_receipt_token("request-1", "token-hash")
        .await
        .unwrap();
    repository
        .complete_receiver_delivery("request-1", 77, "node-a", "token-hash")
        .await
        .unwrap();
    repository
        .complete_receiver_delivery("request-1", 77, "node-a", "token-hash")
        .await
        .unwrap();
    assert!(repository
        .complete_receiver_delivery("request-1", 77, "node-b", "token-hash")
        .await
        .is_err());

    assert!(repository
        .update_receiver_request_status("request-1", ReceiverRequestStatus::Refused, Some("late"),)
        .await
        .is_err());
    assert!(repository
        .update_receiver_request_status("missing", ReceiverRequestStatus::Accepted, None)
        .await
        .is_err());

    let requests = repository.list_receiver_requests(77).await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].status, "completed");
    assert_eq!(requests[0].receiver_name.as_deref(), Some("receiver"));
    assert!(requests[0].responded_at.is_some());
    assert!(requests[0].completed_at.is_some());
}

#[tokio::test]
async fn authenticated_delivery_failure_marks_accepted_receiver_failed() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_receiver_request(ReceiverRequestInsert {
            id: "request-failed",
            transfer_id: 78,
            remote_endpoint_id: "node-a",
            transfer_name: "demo",
            receiver_name: Some("receiver"),
            receiver_device_name: None,
            app_version: "0.1.0",
        })
        .await
        .unwrap();
    repository
        .update_receiver_request_status("request-failed", ReceiverRequestStatus::Accepted, None)
        .await
        .unwrap();
    repository
        .set_receiver_receipt_token("request-failed", "token-hash")
        .await
        .unwrap();

    repository
        .fail_receiver_delivery(
            "request-failed",
            78,
            "node-a",
            "token-hash",
            "destination_exists",
        )
        .await
        .unwrap();
    repository
        .fail_receiver_delivery(
            "request-failed",
            78,
            "node-a",
            "token-hash",
            "destination_exists",
        )
        .await
        .unwrap();
    assert!(repository
        .fail_receiver_delivery(
            "request-failed",
            78,
            "node-b",
            "token-hash",
            "destination_exists",
        )
        .await
        .is_err());

    let requests = repository.list_receiver_requests(78).await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].status, "failed");
    assert_eq!(requests[0].reason.as_deref(), Some("destination_exists"));
}

#[tokio::test]
async fn startup_expiration_is_idempotent_for_pending_requests() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_receiver_request(ReceiverRequestInsert {
            id: "pending-1",
            transfer_id: 78,
            remote_endpoint_id: "node-a",
            transfer_name: "demo",
            receiver_name: None,
            receiver_device_name: None,
            app_version: "0.1.0",
        })
        .await
        .unwrap();

    assert_eq!(
        repository
            .expire_pending_receiver_requests("restart")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repository
            .expire_pending_receiver_requests("restart")
            .await
            .unwrap(),
        0
    );
    let requests = repository.list_receiver_requests(78).await.unwrap();
    assert_eq!(requests[0].status, "expired");
    assert_eq!(requests[0].reason.as_deref(), Some("restart"));
}

#[tokio::test]
async fn concurrent_approval_responses_have_single_winner() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_receiver_request(ReceiverRequestInsert {
            id: "race-1",
            transfer_id: 79,
            remote_endpoint_id: "node-a",
            transfer_name: "demo",
            receiver_name: None,
            receiver_device_name: None,
            app_version: "0.1.0",
        })
        .await
        .unwrap();

    let accepted_repository = repository.clone();
    let refused_repository = repository.clone();
    let (accepted, refused) = tokio::join!(
        accepted_repository.update_receiver_request_status(
            "race-1",
            ReceiverRequestStatus::Accepted,
            None,
        ),
        refused_repository.update_receiver_request_status(
            "race-1",
            ReceiverRequestStatus::Refused,
            Some("race"),
        ),
    );
    assert_ne!(accepted.is_ok(), refused.is_ok());
    let requests = repository.list_receiver_requests(79).await.unwrap();
    assert!(matches!(
        requests[0].status.as_str(),
        "accepted" | "refused"
    ));
}

#[tokio::test]
async fn conditional_transition_rejects_stale_state() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(transfer(
            81,
            TransferDirection::Send,
            TransferStatus::Sharing,
        ))
        .await
        .unwrap();

    let error = repository
        .transition_transfer_status(81, TransferStatus::Receiving, TransferStatus::Done)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("expected one matching transfer"));
    assert_eq!(
        repository.list_transfers().await.unwrap()[0].status,
        "sharing"
    );
}

#[tokio::test]
async fn repeated_terminal_transition_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(transfer(
            88,
            TransferDirection::Send,
            TransferStatus::Importing,
        ))
        .await
        .unwrap();

    repository
        .transition_transfer_status(88, TransferStatus::Importing, TransferStatus::Failed)
        .await
        .unwrap();
    repository
        .transition_transfer_status(88, TransferStatus::Importing, TransferStatus::Failed)
        .await
        .unwrap();
    assert_eq!(
        repository.list_transfers().await.unwrap()[0].status,
        "failed"
    );
}

#[tokio::test]
async fn duplicate_transfer_does_not_overwrite_existing_record() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(transfer(
            82,
            TransferDirection::Send,
            TransferStatus::Sharing,
        ))
        .await
        .unwrap();

    assert!(repository
        .insert_transfer(transfer(
            82,
            TransferDirection::Receive,
            TransferStatus::Receiving,
        ))
        .await
        .is_err());
    let stored = repository.list_transfers().await.unwrap().remove(0);
    assert_eq!(stored.direction, "send");
    assert_eq!(stored.status, "sharing");
}

#[tokio::test]
async fn recovery_fails_only_interrupted_states() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(transfer(
            83,
            TransferDirection::Send,
            TransferStatus::Importing,
        ))
        .await
        .unwrap();
    repository
        .insert_transfer(transfer(
            84,
            TransferDirection::Receive,
            TransferStatus::Receiving,
        ))
        .await
        .unwrap();
    repository
        .insert_transfer(transfer(
            85,
            TransferDirection::Send,
            TransferStatus::Sharing,
        ))
        .await
        .unwrap();

    let recovered = repository.recover_interrupted_transfers().await.unwrap();
    assert_eq!(recovered.len(), 2);
    assert_eq!(recovered[0].transfer_id, 83);
    assert_eq!(recovered[0].previous_status, TransferStatus::Importing);
    assert_eq!(recovered[1].transfer_id, 84);
    assert_eq!(recovered[1].previous_status, TransferStatus::Receiving);

    let transfers = repository.list_transfers().await.unwrap();
    assert_eq!(
        transfers
            .iter()
            .find(|transfer| transfer.transfer_id == 83)
            .unwrap()
            .status,
        "failed"
    );
    assert_eq!(
        transfers
            .iter()
            .find(|transfer| transfer.transfer_id == 84)
            .unwrap()
            .status,
        "failed"
    );
    assert_eq!(
        transfers
            .iter()
            .find(|transfer| transfer.transfer_id == 85)
            .unwrap()
            .status,
        "sharing"
    );
    assert!(repository
        .recover_interrupted_transfers()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn share_completion_is_conditional_and_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(TransferUpsert {
            transfer_id: 86,
            peer_id: None,
            direction: TransferDirection::Send,
            status: TransferStatus::Importing,
            transfer_name: Some("pending"),
            content_hash: None,
            ticket: None,
            file_count: 0,
            total_size: 0,
            access_mode: "approval_required",
        })
        .await
        .unwrap();

    repository
        .complete_share_import(TransferUpsert {
            transfer_id: 86,
            peer_id: None,
            direction: TransferDirection::Send,
            status: TransferStatus::Sharing,
            transfer_name: Some("complete"),
            content_hash: Some("final-hash"),
            ticket: Some("final-ticket"),
            file_count: 2,
            total_size: 24,
            access_mode: "approval_required",
        })
        .await
        .unwrap();

    let stored = repository.list_transfers().await.unwrap().remove(0);
    assert_eq!(stored.status, "sharing");
    assert_eq!(stored.transfer_name.as_deref(), Some("complete"));
    assert_eq!(stored.content_hash.as_deref(), Some("final-hash"));
    assert_eq!(stored.ticket.as_deref(), Some("final-ticket"));
    assert_eq!(stored.file_count, 2);
    assert_eq!(stored.total_size, 24);

    assert!(repository
        .complete_share_import(transfer(
            86,
            TransferDirection::Send,
            TransferStatus::Sharing,
        ))
        .await
        .is_err());
}

#[tokio::test]
async fn injected_write_failure_preserves_previous_state() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(TransferUpsert {
            transfer_id: 87,
            peer_id: None,
            direction: TransferDirection::Send,
            status: TransferStatus::Importing,
            transfer_name: Some("pending"),
            content_hash: None,
            ticket: None,
            file_count: 0,
            total_size: 0,
            access_mode: "approval_required",
        })
        .await
        .unwrap();
    repository.fail_next_write();

    assert!(repository
        .complete_share_import(transfer(
            87,
            TransferDirection::Send,
            TransferStatus::Sharing,
        ))
        .await
        .is_err());
    let stored = repository.list_transfers().await.unwrap().remove(0);
    assert_eq!(stored.status, "importing");
    assert_eq!(stored.content_hash, None);
    assert_eq!(stored.ticket, None);
}

#[tokio::test]
async fn migrates_schema_v2_identity_without_losing_transfer() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("vnidrop.sqlite3");
    let options = SqliteConnectOptions::from_str("sqlite://")
        .unwrap()
        .filename(&database)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query(
        r#"
        CREATE TABLE transfers (
            transfer_id INTEGER PRIMARY KEY,
            direction TEXT NOT NULL,
            status TEXT NOT NULL,
            transfer_name TEXT,
            content_hash TEXT,
            ticket TEXT,
            file_count INTEGER NOT NULL DEFAULT 0,
            total_size INTEGER NOT NULL DEFAULT 0,
            access_mode TEXT NOT NULL DEFAULT 'approval_required',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO transfers (
            transfer_id, direction, status, transfer_name, content_hash, ticket,
            file_count, total_size, access_mode, created_at, updated_at
        ) VALUES (7, 'send', 'stopped', 'legacy', 'hash', 'ticket', 1, 12,
                  'approval_required', 10, 11)
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("PRAGMA user_version = 2")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let repository = Repository::open(temp.path()).await.unwrap();
    assert_eq!(repository.schema_version().await.unwrap(), 7);
    let stored = repository.list_transfers().await.unwrap().remove(0);
    assert_eq!(stored.transfer_id, 7);
    assert_eq!(stored.local_id, "legacy-7-send");
    assert_eq!(stored.transfer_name.as_deref(), Some("legacy"));
    assert_eq!(stored.ticket.as_deref(), Some("ticket"));
    assert_eq!(stored.peer_id, None);
}

#[tokio::test]
async fn event_reads_respect_configured_history_limit() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    for sequence in 0..3 {
        repository
            .insert_event(
                &CoreEvent {
                    id: format!("event-{sequence}"),
                    timestamp: sequence,
                    scope: "endpoint".to_string(),
                    transfer_id: None,
                    direction: None,
                    phase: "test".to_string(),
                    kind: "generated".to_string(),
                    data_json: "{}".to_string(),
                },
                2,
            )
            .await
            .unwrap();
    }

    let events = repository.list_events(None, 2).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, "event-2");
    assert_eq!(events[1].id, "event-1");
}

#[tokio::test]
async fn deleting_transfer_removes_related_history_transactionally() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(transfer(
            88,
            TransferDirection::Send,
            TransferStatus::Stopped,
        ))
        .await
        .unwrap();
    repository
        .insert_receiver_request(ReceiverRequestInsert {
            id: "request-delete",
            transfer_id: 88,
            remote_endpoint_id: "receiver",
            transfer_name: "demo",
            receiver_name: None,
            receiver_device_name: None,
            app_version: "1.0",
        })
        .await
        .unwrap();
    repository
        .insert_event(
            &CoreEvent {
                id: "event-delete".to_string(),
                timestamp: 1,
                scope: "transfer".to_string(),
                transfer_id: Some(88),
                direction: Some("send".to_string()),
                phase: "test".to_string(),
                kind: "created".to_string(),
                data_json: "{}".to_string(),
            },
            500,
        )
        .await
        .unwrap();

    repository.delete_transfer(88).await.unwrap();

    assert!(repository.list_transfers().await.unwrap().is_empty());
    assert!(repository
        .list_events(Some(88), 500)
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .list_receiver_requests(88)
        .await
        .unwrap()
        .is_empty());
    assert!(repository.delete_transfer(88).await.is_err());
}

#[tokio::test]
async fn deleting_receive_history_only_removes_terminal_receives_and_dependants() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let records = [
        (100, TransferDirection::Receive, TransferStatus::Done),
        (101, TransferDirection::Receive, TransferStatus::Failed),
        (102, TransferDirection::Receive, TransferStatus::Cancelled),
        (103, TransferDirection::Receive, TransferStatus::Receiving),
        (104, TransferDirection::Send, TransferStatus::Done),
        (105, TransferDirection::Send, TransferStatus::Sharing),
    ];

    for (transfer_id, direction, status) in records {
        repository
            .insert_transfer(transfer(transfer_id, direction, status))
            .await
            .unwrap();
        let request_id = format!("request-{transfer_id}");
        repository
            .insert_receiver_request(ReceiverRequestInsert {
                id: &request_id,
                transfer_id,
                remote_endpoint_id: "receiver",
                transfer_name: "demo",
                receiver_name: None,
                receiver_device_name: None,
                app_version: "1.0",
            })
            .await
            .unwrap();
        repository
            .insert_event(
                &CoreEvent {
                    id: format!("event-{transfer_id}"),
                    timestamp: transfer_id as i64,
                    scope: "transfer".to_string(),
                    transfer_id: Some(transfer_id),
                    direction: Some(direction.as_str().to_string()),
                    phase: "test".to_string(),
                    kind: "created".to_string(),
                    data_json: "{}".to_string(),
                },
                500,
            )
            .await
            .unwrap();
    }

    assert_eq!(repository.delete_receive_history().await.unwrap(), 3);

    let remaining = repository.list_transfers().await.unwrap();
    assert_eq!(remaining.len(), 3);
    for transfer_id in [103, 104, 105] {
        assert!(remaining
            .iter()
            .any(|transfer| transfer.transfer_id == transfer_id));
        assert_eq!(
            repository
                .list_receiver_requests(transfer_id)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            repository
                .list_events(Some(transfer_id), 500)
                .await
                .unwrap()
                .len(),
            1
        );
    }
    for transfer_id in [100, 101, 102] {
        assert!(repository
            .list_receiver_requests(transfer_id)
            .await
            .unwrap()
            .is_empty());
        assert!(repository
            .list_events(Some(transfer_id), 500)
            .await
            .unwrap()
            .is_empty());
    }
    assert_eq!(repository.delete_receive_history().await.unwrap(), 0);
}

#[tokio::test]
async fn receive_history_mid_transaction_failure_preserves_all_related_rows() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    repository
        .insert_transfer(transfer(
            106,
            TransferDirection::Receive,
            TransferStatus::Done,
        ))
        .await
        .unwrap();
    repository
        .insert_receiver_request(ReceiverRequestInsert {
            id: "request-preserved",
            transfer_id: 106,
            remote_endpoint_id: "receiver",
            transfer_name: "demo",
            receiver_name: None,
            receiver_device_name: None,
            app_version: "1.0",
        })
        .await
        .unwrap();
    repository
        .insert_event(
            &CoreEvent {
                id: "event-preserved".to_string(),
                timestamp: 1,
                scope: "transfer".to_string(),
                transfer_id: Some(106),
                direction: Some("receive".to_string()),
                phase: "test".to_string(),
                kind: "created".to_string(),
                data_json: "{}".to_string(),
            },
            500,
        )
        .await
        .unwrap();
    repository.fail_receive_history_after_dependants();

    assert!(repository.delete_receive_history().await.is_err());
    assert_eq!(repository.list_transfers().await.unwrap().len(), 1);
    assert_eq!(
        repository.list_receiver_requests(106).await.unwrap().len(),
        1
    );
    assert_eq!(
        repository.list_events(Some(106), 500).await.unwrap().len(),
        1
    );
}
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
