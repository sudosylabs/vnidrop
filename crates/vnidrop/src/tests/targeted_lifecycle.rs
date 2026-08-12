use std::{
    path::Path,
    str::FromStr,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

use iroh::{endpoint::presets, Endpoint};
use iroh_blobs::{get::request::get_hash_seq_and_sizes, ticket::BlobTicket};

use crate::{
    secure_secret::FaultInjectingSecretStore, CoreEvent, CoreEventSink, DeviceRelationshipState,
    PendingTargetedOffer, ShareMetadataInput, ShareSource, SourceKind, TargetedTransferState,
    TransferAccessMode, VnidropCore,
};

struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl CoreEventSink for RecordingSink {
    fn on_event(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct EventGateSink {
    kind: &'static str,
    observed: mpsc::SyncSender<CoreEvent>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl CoreEventSink for EventGateSink {
    fn on_event(&self, event: CoreEvent) {
        if event.phase == "targeted_transfer" && event.kind == self.kind {
            self.observed.send(event).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
    }
}

struct ProtectedNode {
    data_dir: tempfile::TempDir,
    secret_store: Arc<FaultInjectingSecretStore>,
    core: Option<Arc<VnidropCore>>,
}

impl ProtectedNode {
    fn new() -> Self {
        Self::with_sink(Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        }))
    }

    fn with_sink(sink: Arc<dyn CoreEventSink>) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let secret_store = Arc::new(FaultInjectingSecretStore::default());
        let core = VnidropCore::initialize_with_test_secret_store(
            data_dir.path().to_string_lossy().into_owned(),
            sink,
            secret_store.clone(),
        )
        .unwrap();
        Self {
            data_dir,
            secret_store,
            core: Some(core),
        }
    }

    fn core(&self) -> Arc<VnidropCore> {
        self.core.as_ref().unwrap().clone()
    }

    fn restart(mut self) -> Self {
        self.core.take().unwrap().shutdown();
        let core = VnidropCore::initialize_with_test_secret_store(
            self.data_dir.path().to_string_lossy().into_owned(),
            Arc::new(RecordingSink {
                events: Mutex::new(Vec::new()),
            }),
            self.secret_store.clone(),
        )
        .unwrap();
        self.core = Some(core);
        self
    }
}

impl Drop for ProtectedNode {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

fn source(path: &Path) -> ShareSource {
    ShareSource {
        kind: SourceKind::Path,
        value: path.to_string_lossy().into_owned(),
        display_name: Some(path.file_name().unwrap().to_string_lossy().into_owned()),
        is_directory: false,
    }
}

fn wait_for_request(core: &VnidropCore, transfer_id: u64) -> crate::ReceiverRequest {
    let started = Instant::now();
    loop {
        if let Some(request) = core
            .list_receiver_requests(transfer_id)
            .unwrap()
            .into_iter()
            .find(|request| request.status == "requested")
        {
            return request;
        }
        assert!(started.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn establish_saved(sender: &ProtectedNode, receiver: &ProtectedNode, transfer_id: u64) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let path = source_dir.path().join("pair.txt");
    std::fs::write(&path, b"pair").unwrap();
    let share = sender
        .core()
        .share_files(
            vec![source(&path)],
            ShareMetadataInput {
                transfer_id,
                transfer_name: Some("pair.txt".to_string()),
                sender_name: Some("sender".to_string()),
                access_mode: TransferAccessMode::ApprovalRequired,
            },
        )
        .unwrap();
    let receiver_core = receiver.core();
    let output = output_dir.path().to_string_lossy().into_owned();
    let receive = std::thread::spawn(move || {
        receiver_core.receive(share.ticket, output, Some("receiver".to_string()))
    });
    let request = wait_for_request(&sender.core(), transfer_id);
    sender
        .core()
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    receive.join().unwrap().unwrap();

    let sender_id = sender.core().status().endpoint_id;
    let receiver_id = receiver.core().status().endpoint_id;
    let started = Instant::now();
    while !sender
        .core()
        .list_pairing_eligibilities()
        .unwrap()
        .iter()
        .any(|entry| entry.peer_endpoint_id == receiver_id)
    {
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(sender
        .core()
        .request_saved_device_pairing(receiver_id.clone())
        .unwrap());
    let started = Instant::now();
    while !receiver
        .core()
        .list_device_relationships()
        .unwrap()
        .iter()
        .any(|row| {
            row.remote_endpoint_id == sender_id
                && row.state == DeviceRelationshipState::PendingIncoming
        })
    {
        assert!(started.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(25));
    }
    receiver
        .core()
        .respond_to_device_pairing(sender_id.clone(), true)
        .unwrap();
    for (node, peer) in [(sender, receiver_id), (receiver, sender_id)] {
        let started = Instant::now();
        while !node
            .core()
            .list_saved_devices()
            .unwrap()
            .iter()
            .any(|device| device.endpoint_id == peer)
        {
            assert!(started.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

fn wait_for_offer(core: &VnidropCore) -> PendingTargetedOffer {
    let started = Instant::now();
    loop {
        if let Some(offer) = core.list_pending_targeted_offers().into_iter().next() {
            return offer;
        }
        assert!(started.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn approve(
    sender: &ProtectedNode,
    receiver: &ProtectedNode,
    name: &str,
    payload: &[u8],
) -> crate::TargetedTransfer {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, payload).unwrap();
    let receiver_core = receiver.core();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_offer(&receiver_core);
        receiver_core.respond_to_targeted_offer(offer.transfer_id, true)
    });
    let transfer = sender
        .core()
        .create_targeted_transfer(
            receiver.core().status().endpoint_id,
            vec![source(&path)],
            Some(name.to_string()),
        )
        .unwrap();
    accept.join().unwrap().unwrap();
    transfer
}

#[test]
fn snapshot_name_round_trips_through_create_get_list_and_restart() {
    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    establish_saved(&sender, &receiver, 21_001);
    let transfer = approve(&sender, &receiver, "Quarterly report.pdf", b"report");
    assert_eq!(transfer.transfer_name, "Quarterly report.pdf");
    assert_eq!(
        sender
            .core()
            .get_targeted_transfer(transfer.id.clone())
            .unwrap()
            .unwrap()
            .transfer_name,
        "Quarterly report.pdf"
    );
    assert_eq!(
        sender
            .core()
            .list_targeted_transfers()
            .unwrap()
            .into_iter()
            .find(|row| row.id == transfer.id)
            .unwrap()
            .transfer_name,
        "Quarterly report.pdf"
    );
    let sender = sender.restart();
    assert_eq!(
        sender
            .core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .transfer_name,
        "Quarterly report.pdf"
    );
}

#[test]
fn accepted_event_is_emitted_only_after_receiver_snapshot_is_durable() {
    let (observed_tx, observed_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::with_sink(Arc::new(EventGateSink {
        kind: "approved",
        observed: observed_tx,
        release: Mutex::new(release_rx),
    }));
    establish_saved(&sender, &receiver, 21_002);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("event.txt");
    std::fs::write(&path, b"event").unwrap();
    let receiver_core = receiver.core();
    let accept = std::thread::spawn(move || {
        let offer = wait_for_offer(&receiver_core);
        receiver_core.respond_to_targeted_offer(offer.transfer_id, true)
    });
    let sender_core = sender.core();
    let receiver_id = receiver.core().status().endpoint_id;
    let create = std::thread::spawn(move || {
        sender_core.create_targeted_transfer(
            receiver_id,
            vec![source(&path)],
            Some("event.txt".to_string()),
        )
    });
    let event = observed_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    let id = serde_json::from_str::<serde_json::Value>(&event.data_json).unwrap()
        ["targeted_transfer_id"]
        .as_str()
        .unwrap()
        .to_string();
    let durable = receiver.core().get_targeted_transfer(id.clone()).unwrap();
    release_tx.send(()).unwrap();
    accept.join().unwrap().unwrap();
    create.join().unwrap().unwrap();
    assert_eq!(durable.unwrap().state, TargetedTransferState::Approved);
}

#[test]
fn terminal_events_have_ordered_revisions_and_no_later_progress() {
    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    establish_saved(&sender, &receiver, 21_003);
    let transfer = approve(&sender, &receiver, "events.bin", &vec![7; 128 * 1024]);
    let output = tempfile::tempdir().unwrap();
    receiver
        .core()
        .receive_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    let mut events = receiver
        .core()
        .list_events(None)
        .unwrap()
        .into_iter()
        .filter(|event| {
            event.phase == "targeted_transfer" && event.data_json.contains(&transfer.id)
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.revision);
    assert!(events
        .windows(2)
        .all(|pair| pair[0].revision < pair[1].revision));
    let completed = events
        .iter()
        .position(|event| event.kind == "completed")
        .expect("completed wake-up");
    assert!(events[completed + 1..]
        .iter()
        .all(|event| event.kind != "progress"));
    assert_eq!(
        receiver
            .core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .state,
        TargetedTransferState::Completed
    );
}

#[test]
fn delete_preserves_verified_progress_and_is_idempotent() {
    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    establish_saved(&sender, &receiver, 21_004);
    let transfer = approve(&sender, &receiver, "delete.bin", b"verified payload");
    let output = tempfile::tempdir().unwrap();
    receiver
        .core()
        .receive_targeted_transfer(
            transfer.id.clone(),
            output.path().to_string_lossy().into_owned(),
        )
        .unwrap();
    let completed = receiver
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    receiver
        .core()
        .delete_targeted_transfer(transfer.id.clone())
        .unwrap();
    let deleted = receiver
        .core()
        .get_targeted_transfer(transfer.id.clone())
        .unwrap()
        .unwrap();
    assert_eq!(deleted.verified_bytes, completed.verified_bytes);
    receiver
        .core()
        .delete_targeted_transfer(transfer.id.clone())
        .unwrap();
    assert_eq!(
        receiver
            .core()
            .get_targeted_transfer(transfer.id)
            .unwrap()
            .unwrap()
            .updated_at,
        deleted.updated_at
    );
}

#[test]
fn restart_restores_bound_receiver_but_not_third_peer_access() {
    let sender = ProtectedNode::new();
    let receiver = ProtectedNode::new();
    let stranger = ProtectedNode::new();
    establish_saved(&sender, &receiver, 21_005);
    let transfer = approve(&sender, &receiver, "restart.txt", b"restart payload");
    let sender = sender.restart();
    let receiver = receiver.restart();

    let stranger_result = stranger.core().receive_targeted_transfer(
        transfer.id.clone(),
        tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(stranger_result.is_err());
    let (_, leaked_ticket) = sender
        .core()
        .targeted_blob_ticket_for_test(transfer.id.clone())
        .unwrap();
    let leaked_ticket = BlobTicket::from_str(&leaked_ticket).unwrap();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let attacker = Endpoint::builder(presets::Minimal).bind().await.unwrap();
            if let Ok(connection) = attacker
                .connect(leaked_ticket.addr().clone(), iroh_blobs::ALPN)
                .await
            {
                assert!(get_hash_seq_and_sizes(
                    &connection,
                    &leaked_ticket.hash_and_format().hash,
                    1024 * 1024 * 32,
                    None,
                )
                .await
                .is_err());
            }
        });
    let output = tempfile::tempdir().unwrap();
    receiver
        .core()
        .resume_targeted_transfer(transfer.id, output.path().to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("restart.txt")).unwrap(),
        b"restart payload"
    );
}
