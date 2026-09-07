use std::time::Duration;

use crate::{
    api::{CoreEvent, TargetedTransferRole, TargetedTransferState},
    invitation::Repository,
    targeted_transfer::{self, TargetedTransferRow, TargetedTransferStore},
};

#[tokio::test]
async fn targeted_progress_waits_for_event_transaction_and_completes_durably() {
    let temp = tempfile::tempdir().unwrap();
    let pool = super::open_pool(temp.path()).await.unwrap();
    let invitation = Repository::from_pool(pool.clone());
    invitation.ensure_schema().await.unwrap();
    targeted_transfer::ensure_schema(&pool).await.unwrap();
    let targeted = TargetedTransferStore::new(pool.clone());
    targeted
        .insert(&TargetedTransferRow {
            id: "receive".to_string(),
            protocol_transfer_id: 1,
            sender_endpoint_id: "sender".to_string(),
            receiver_endpoint_id: "receiver".to_string(),
            manifest_id: "manifest".to_string(),
            content_hash: "content".to_string(),
            transfer_name: "payload".to_string(),
            file_count: 1,
            total_size: 8,
            verified_bytes: 0,
            blob_ticket: None,
            authorization_secret_handle: None,
            role: TargetedTransferRole::Receiver,
            state: TargetedTransferState::Transferring,
            created_at: 1,
            updated_at: 1,
        })
        .await
        .unwrap();
    let event = CoreEvent {
        id: "progress".to_string(),
        revision: 1,
        timestamp: 1,
        scope: "endpoint".to_string(),
        transfer_id: None,
        direction: None,
        phase: "targeted_transfer".to_string(),
        kind: "progress".to_string(),
        data_json: "{}".to_string(),
    };
    invitation.insert_event(&event, 10).await.unwrap();

    // Make SQLite contention fail immediately; queuing must happen in the pool.
    pool.set_connect_options(
        pool.connect_options()
            .as_ref()
            .clone()
            .busy_timeout(Duration::ZERO),
    );
    let mut connections = Vec::new();
    for _ in 0..pool.size() {
        let mut connection = pool.acquire().await.unwrap();
        sqlx::query("PRAGMA busy_timeout = 0")
            .execute(&mut *connection)
            .await
            .unwrap();
        connections.push(connection);
    }
    drop(connections);

    let mut event_transaction = pool.begin().await.unwrap();
    sqlx::query("UPDATE transfer_events SET revision = 2 WHERE id = 'progress'")
        .execute(&mut *event_transaction)
        .await
        .unwrap();
    let progress = targeted.advance_verified_bytes("receive", 8);
    tokio::pin!(progress);
    let before_commit = tokio::time::timeout(Duration::from_millis(100), &mut progress).await;
    event_transaction.commit().await.unwrap();
    assert!(
        before_commit.is_err(),
        "progress must wait for the event transaction, got {before_commit:?}"
    );
    assert!(tokio::time::timeout(Duration::from_secs(2), progress)
        .await
        .expect("progress must resume after the event transaction commits")
        .unwrap());

    targeted
        .complete_receiver_and_enqueue("receive", 8)
        .await
        .unwrap();
    let completed = targeted.get_row("receive").await.unwrap().unwrap();
    assert_eq!(completed.state, TargetedTransferState::Completed);
    assert_eq!(completed.verified_bytes, completed.total_size);
    let pending = targeted.list_pending_completions().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, completed.id);
    let events = invitation.list_events(None, 10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!((&events[0].id, events[0].revision), (&event.id, 2));
}
