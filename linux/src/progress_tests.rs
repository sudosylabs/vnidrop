use super::*;
use serde_json::json;
use vnidrop::TransferAccessMode;

fn event(revision: u64, phase: &str, kind: &str, data: Value) -> CoreEvent {
    CoreEvent {
        id: revision.to_string(),
        revision,
        timestamp: revision as i64,
        scope: "transfer".into(),
        transfer_id: Some(7),
        direction: Some("send".into()),
        phase: phase.into(),
        kind: kind.into(),
        data_json: data.to_string(),
    }
}

fn request(endpoint: &str) -> ReceiverRequest {
    ReceiverRequest {
        id: "request".into(),
        transfer_id: 7,
        remote_endpoint_id: endpoint.into(),
        transfer_name: "Files".into(),
        receiver_name: None,
        receiver_device_name: None,
        app_version: String::new(),
        status: "accepted".into(),
        reason: None,
        requested_at: 0,
        responded_at: None,
        completed_at: None,
    }
}

#[test]
fn receiver_progress_uses_fingerprints_and_separates_interleaved_connections_and_peers() {
    let fingerprint = format!("<redacted:{}>", &blake3::hash(b"receiver-a").to_hex()[..8]);
    let events = vec![
        event(
            6,
            "transfer",
            "progress",
            json!({"endpoint_id":"receiver-b","connection_id":3,"request_id":1,"end_offset":100}),
        ),
        event(
            5,
            "transfer",
            "progress",
            json!({"endpoint_id":fingerprint,"connection_id":2,"request_id":1,"end_offset":25}),
        ),
        event(
            4,
            "transfer",
            "completed",
            json!({"endpoint_id":fingerprint,"connection_id":1,"request_id":1}),
        ),
        event(
            3,
            "transfer",
            "started",
            json!({"endpoint_id":fingerprint,"connection_id":2,"request_id":1,"size":50}),
        ),
        event(
            2,
            "transfer",
            "started",
            json!({"endpoint_id":"receiver-b","connection_id":3,"request_id":1,"size":100}),
        ),
        event(
            1,
            "transfer",
            "started",
            json!({"endpoint_id":fingerprint,"connection_id":1,"request_id":1,"size":50}),
        ),
    ];
    assert_eq!(
        for_receiver(&events, &request("receiver-a"), 100),
        Some(Progress {
            label: "progress_sending",
            bytes: Some(75),
            total: Some(100)
        })
    );
    assert_eq!(
        for_receiver(&events, &request("receiver-b"), 100)
            .unwrap()
            .fraction(),
        Some(1.0)
    );
    assert_eq!(for_receiver(&events, &request("other"), 100), None);
    let mut later = request("receiver-a");
    later.requested_at = 7;
    assert_eq!(for_receiver(&events, &later, 100), None);
}

#[test]
fn missing_endpoint_maps_through_connection_and_aborted_streams_do_not_erase_other_progress() {
    let events = vec![
        event(
            5,
            "transfer",
            "aborted",
            json!({"connection_id":1,"request_id":1}),
        ),
        event(
            4,
            "transfer",
            "progress",
            json!({"connection_id":1,"request_id":2,"end_offset":30}),
        ),
        event(
            3,
            "transfer",
            "started",
            json!({"connection_id":1,"request_id":2,"size":60}),
        ),
        event(
            2,
            "transfer",
            "started",
            json!({"connection_id":1,"request_id":1,"size":40}),
        ),
        event(
            1,
            "provider",
            "client-connected",
            json!({"connection_id":1,"endpoint_id":"receiver"}),
        ),
    ];
    assert_eq!(
        for_receiver(&events, &request("receiver"), 100)
            .unwrap()
            .bytes,
        Some(30)
    );
    let mut aborted = vec![event(
        6,
        "transfer",
        "aborted",
        json!({"connection_id":1,"request_id":2}),
    )];
    aborted.extend(events);
    assert_eq!(
        for_receiver(&aborted, &request("receiver"), 100),
        Some(Progress {
            label: "progress_interrupted",
            bytes: None,
            total: None
        })
    );
    let mut completed = request("receiver");
    completed.status = "completed".into();
    assert_eq!(for_receiver(&aborted, &completed, 100), None);
}

#[test]
fn receiving_progress_handles_phases_unknown_totals_and_durable_terminal_state() {
    let mut transfer = StoredTransfer {
        local_id: "receive:7".into(),
        transfer_id: 7,
        peer_id: None,
        direction: "receive".into(),
        status: "receiving".into(),
        transfer_name: None,
        content_hash: None,
        ticket: None,
        file_count: 1,
        total_size: 0,
        access_mode: TransferAccessMode::ApprovalRequired,
        created_at: 0,
        updated_at: 0,
    };
    let mut receiving = event(1, "download", "progress", json!({"downloaded": 20}));
    receiving.direction = Some("receive".into());
    assert_eq!(
        for_transfer(&[receiving.clone()], &transfer)
            .unwrap()
            .fraction(),
        None
    );
    transfer.total_size = 100;
    assert_eq!(
        for_transfer(&[receiving.clone()], &transfer)
            .unwrap()
            .fraction(),
        Some(0.2)
    );
    receiving.phase = "export".into();
    receiving.data_json = json!({"exported":"30","file_size":60}).to_string();
    assert_eq!(
        for_transfer(&[receiving.clone()], &transfer),
        Some(Progress {
            label: "progress_saving",
            bytes: Some(30),
            total: Some(60)
        })
    );
    transfer.status = "done".into();
    assert_eq!(for_transfer(&[receiving], &transfer), None);
}
