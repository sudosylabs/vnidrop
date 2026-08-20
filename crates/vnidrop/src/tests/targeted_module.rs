use std::sync::{Arc, Mutex};

use crate::{
    api::{TargetedTransferRole, TargetedTransferState},
    targeted_transfer::{TargetedTransferModule, TargetedTransferRow},
    util::now_ms,
};

fn row(id: &str, role: TargetedTransferRole, state: TargetedTransferState) -> TargetedTransferRow {
    let now = now_ms();
    TargetedTransferRow {
        id: id.to_string(),
        protocol_transfer_id: id.bytes().map(u64::from).sum(),
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
        role,
        state,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn peer_cancel_is_linearized_before_cleanup_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let stores = crate::persistence::open_all(temp.path()).await.unwrap();
    stores
        .targeted
        .insert(&row(
            "transfer",
            TargetedTransferRole::Receiver,
            TargetedTransferState::Approved,
        ))
        .await
        .unwrap();
    let cleaned = Arc::new(Mutex::new(Vec::new()));
    let module = TargetedTransferModule::for_test(stores.targeted.clone(), cleaned.clone());

    assert!(!module
        .cancel_from_peer("intruder", "transfer")
        .await
        .unwrap());
    assert_eq!(
        module.get_row("transfer").await.unwrap().unwrap().state,
        TargetedTransferState::Approved
    );

    assert!(module.cancel_from_peer("sender", "transfer").await.unwrap());
    assert!(module.cancel_from_peer("sender", "transfer").await.unwrap());
    assert_eq!(
        module.get_row("transfer").await.unwrap().unwrap().state,
        TargetedTransferState::Cancelled
    );
    assert_eq!(cleaned.lock().unwrap().as_slice(), ["transfer"]);
}

#[tokio::test]
async fn recovery_concentrates_in_flight_interruption() {
    let temp = tempfile::tempdir().unwrap();
    let stores = crate::persistence::open_all(temp.path()).await.unwrap();
    for (id, state) in [
        ("connecting", TargetedTransferState::Connecting),
        ("transferring", TargetedTransferState::Transferring),
        ("approved", TargetedTransferState::Approved),
    ] {
        stores
            .targeted
            .insert(&row(id, TargetedTransferRole::Receiver, state))
            .await
            .unwrap();
    }
    let module =
        TargetedTransferModule::for_test(stores.targeted, Arc::new(Mutex::new(Vec::new())));

    let mut interrupted = module.recover_in_flight().await.unwrap();
    interrupted.sort();
    assert_eq!(interrupted, ["connecting", "transferring"]);
    assert_eq!(
        module.get_row("approved").await.unwrap().unwrap().state,
        TargetedTransferState::Approved
    );
}
