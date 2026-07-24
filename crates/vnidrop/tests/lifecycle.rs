mod support;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use futures_lite::StreamExt as _;
use iroh_blobs::store::fs::FsStore;
use support::{share_path, CoreGuard, RecordingSink, TestNode, TestRelay};
use vnidrop::{
    CoreEvent, CoreEventSink, CoreLimits, CoreNetworkConfig, CoreRelayMode, ShareMetadataInput,
    ShareSource, SourceKind, TransferAccessMode,
};

#[test]
fn share_creation_persists_selected_access_mode_atomically() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("public.txt");
    std::fs::write(&source_path, b"public share").unwrap();
    let sender = TestNode::new();

    sender
        .core
        .share_files(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source_path.to_string_lossy().into_owned(),
                display_name: Some("public.txt".to_string()),
                is_directory: false,
            }],
            ShareMetadataInput {
                transfer_id: 9,
                transfer_name: Some("Public file".to_string()),
                sender_name: Some("Sender".to_string()),
                access_mode: TransferAccessMode::Public,
            },
        )
        .unwrap();

    let transfer = sender.core.list_transfers().unwrap().remove(0);
    assert_eq!(transfer.access_mode, TransferAccessMode::Public);
}

#[test]
fn cancelling_share_updates_status_and_events() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("cancel.txt");
    std::fs::write(&source_path, b"cancel me").unwrap();
    let sender = TestNode::new();
    let share = share_path(&sender.core, &source_path, 10, "cancel.txt", false);

    sender.core.cancel_transfer(share.transfer_id).unwrap();

    let transfers = sender.core.list_transfers().unwrap();
    assert_eq!(transfers[0].status, "stopped");
    assert!(sender
        .sink
        .events()
        .iter()
        .any(|event| event.kind == "share-stopped"));
}

#[test]
fn deleting_share_revokes_it_and_removes_persisted_history() {
    let source_dir = tempfile::tempdir().unwrap();
    let core_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("delete.txt");
    std::fs::write(&source_path, b"delete me").unwrap();

    let sender = CoreGuard::start(core_dir.path(), Arc::new(RecordingSink::default()));
    let share = share_path(&sender, &source_path, 101, "delete.txt", false);
    sender.delete_transfer(share.transfer_id).unwrap();

    assert!(sender.list_transfers().unwrap().is_empty());
    assert_eq!(sender.status().active_shares, 0);
    drop(sender);

    let restarted = CoreGuard::start(core_dir.path(), Arc::new(RecordingSink::default()));
    assert!(restarted.list_transfers().unwrap().is_empty());
    assert_eq!(restarted.status().active_shares, 0);
}

#[test]
fn active_share_tag_is_persistent_and_removed_with_transfer() {
    let source_dir = tempfile::tempdir().unwrap();
    let core_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("tagged.txt");
    std::fs::write(&source_path, b"tagged content").unwrap();

    let sender = CoreGuard::start(core_dir.path(), Arc::new(RecordingSink::default()));
    let share = share_path(&sender, &source_path, 102, "tagged.txt", false);
    drop(sender);
    assert_eq!(share_tag_count(core_dir.path()), 1);

    let restarted = CoreGuard::start(core_dir.path(), Arc::new(RecordingSink::default()));
    restarted.delete_transfer(share.transfer_id).unwrap();
    drop(restarted);
    assert_eq!(share_tag_count(core_dir.path()), 0);
}

fn share_tag_count(core_dir: &std::path::Path) -> usize {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let store = FsStore::load(core_dir.join("blobs")).await.unwrap();
        let mut tags = store.tags().list_prefix("vnidrop/share/").await.unwrap();
        let mut count = 0;
        while let Some(tag) = tags.next().await {
            tag.unwrap();
            count += 1;
        }
        store.shutdown().await.unwrap();
        count
    })
}

#[test]
fn persisted_share_is_recovered_and_can_be_stopped_after_restart() {
    let source_dir = tempfile::tempdir().unwrap();
    let core_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("persistent.txt");
    std::fs::write(&source_path, b"survives restart").unwrap();

    let sender = CoreGuard::start(core_dir.path(), Arc::new(RecordingSink::default()));
    let share = share_path(&sender, &source_path, 20, "persistent.txt", false);
    drop(sender);

    let restarted = CoreGuard::start(core_dir.path(), Arc::new(RecordingSink::default()));
    assert_eq!(restarted.status().active_shares, 1);
    restarted.cancel_transfer(share.transfer_id).unwrap();

    let transfer = restarted
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == share.transfer_id)
        .unwrap();
    assert_eq!(transfer.status, "stopped");
    assert_eq!(restarted.status().active_shares, 0);
}

#[test]
fn persisted_share_is_revoked_when_restarted_with_a_different_relay_profile() {
    let relay_a = TestRelay::start();
    let relay_b = TestRelay::start();
    assert_ne!(relay_a.url, relay_b.url);

    let source_dir = tempfile::tempdir().unwrap();
    let core_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("stale-relay.txt");
    std::fs::write(&source_path, b"must not survive a relay profile change").unwrap();
    let sender = CoreGuard::start_with_network_config(
        core_dir.path(),
        Arc::new(RecordingSink::default()),
        CoreNetworkConfig {
            mode: CoreRelayMode::StrictCustom,
            relay_urls: vec![relay_a.url.clone()],
        },
    );
    let share = share_path(&sender, &source_path, 120, "stale-relay.txt", false);
    drop(sender);
    drop(relay_a);

    let restarted = CoreGuard::start_with_network_config(
        core_dir.path(),
        Arc::new(RecordingSink::default()),
        CoreNetworkConfig {
            mode: CoreRelayMode::StrictCustom,
            relay_urls: vec![relay_b.url.clone()],
        },
    );

    assert_eq!(restarted.status().active_shares, 0);
    let transfer = restarted
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == share.transfer_id)
        .unwrap();
    assert_eq!(transfer.status, "stopped");
    assert!(restarted
        .list_events(Some(share.transfer_id))
        .unwrap()
        .iter()
        .any(|event| {
            event.phase == "recovery" && event.kind == "share-stopped-network-profile-changed"
        }));
    drop(restarted);
    assert_eq!(share_tag_count(core_dir.path()), 0);
}

#[test]
fn stopped_share_rejects_receive() {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("revoked.txt");
    std::fs::write(&source_path, b"must not be served").unwrap();
    let sender = TestNode::new();
    let receiver = TestNode::new();
    let share = share_path(&sender.core, &source_path, 21, "revoked.txt", false);

    sender.core.cancel_transfer(share.transfer_id).unwrap();
    let result = receiver.core.receive(
        share.ticket,
        output_dir.path().to_string_lossy().to_string(),
        Some("receiver".to_string()),
    );

    assert!(result.is_err(), "a stopped share must not serve blob bytes");
    assert!(!output_dir.path().join("revoked.txt").exists());
}

#[test]
fn ticket_created_event_does_not_include_full_ticket() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("secret.txt");
    std::fs::write(&source_path, b"capability material").unwrap();
    let sender = TestNode::new();
    let share = share_path(&sender.core, &source_path, 40, "secret.txt", false);

    let ticket_events: Vec<_> = sender
        .sink
        .events()
        .into_iter()
        .filter(|event| event.phase == "ticket" && event.kind == "created")
        .collect();
    assert_eq!(ticket_events.len(), 1);
    let data = &ticket_events[0].data_json;
    assert!(
        !data.contains(&share.ticket),
        "events must not retain the full vnd1 ticket capability"
    );
    assert!(
        !data.contains("vnd1:"),
        "events must not embed ticket prefixes"
    );
    assert!(
        data.contains(&share.hash),
        "events should still record the content hash for diagnostics"
    );
}

#[test]
fn receive_rejects_non_vnidrop_ticket_input() {
    let output_dir = tempfile::tempdir().unwrap();
    let receiver = TestNode::new();

    let result = receiver.core.receive(
        "blobaaabcdefghijklmnopqrstuvwxyz0123456789".to_string(),
        output_dir.path().to_string_lossy().to_string(),
        Some("receiver".to_string()),
    );
    assert!(
        result.is_err(),
        "non-vnd1 tickets must be rejected before network work"
    );
}

#[test]
fn failed_import_leaves_durable_failed_transfer() {
    let source_dir = tempfile::tempdir().unwrap();
    let sender = TestNode::new();
    let transfer_id = 22;

    let result = sender.core.share_files(
        vec![ShareSource {
            kind: SourceKind::Path,
            value: source_dir
                .path()
                .join("missing.txt")
                .to_string_lossy()
                .to_string(),
            display_name: Some("missing.txt".to_string()),
            is_directory: false,
        }],
        ShareMetadataInput {
            transfer_id,
            transfer_name: Some("missing".to_string()),
            sender_name: None,
            access_mode: TransferAccessMode::ApprovalRequired,
        },
    );

    assert!(result.is_err());
    let transfer = sender
        .core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == transfer_id)
        .unwrap();
    assert_eq!(transfer.status, "failed");
    assert_eq!(sender.core.status().active_shares, 0);
}

#[test]
fn duplicate_transfer_id_does_not_replace_active_share() {
    let source_dir = tempfile::tempdir().unwrap();
    let first_path = source_dir.path().join("first.txt");
    let second_path = source_dir.path().join("second.txt");
    std::fs::write(&first_path, b"first").unwrap();
    std::fs::write(&second_path, b"second").unwrap();
    let sender = TestNode::new();
    let first = share_path(&sender.core, &first_path, 23, "first.txt", false);

    let duplicate = sender.core.share_files(
        vec![ShareSource {
            kind: SourceKind::Path,
            value: second_path.to_string_lossy().to_string(),
            display_name: Some("second.txt".to_string()),
            is_directory: false,
        }],
        ShareMetadataInput {
            transfer_id: first.transfer_id,
            transfer_name: Some("second".to_string()),
            sender_name: None,
            access_mode: TransferAccessMode::ApprovalRequired,
        },
    );

    assert!(duplicate.is_err());
    assert_eq!(sender.core.status().active_shares, 1);
    let transfer = sender
        .core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == first.transfer_id)
        .unwrap();
    assert_eq!(transfer.status, "sharing");
    assert_eq!(transfer.transfer_name.as_deref(), Some("first.txt"));
}

#[test]
fn access_mode_update_requires_active_persisted_share() {
    let sender = TestNode::new();

    assert!(sender
        .core
        .set_transfer_access_mode(999, TransferAccessMode::Public)
        .is_err());
}

#[test]
fn approve_endpoint_requires_active_share_and_nonempty_id() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();
    let sender = TestNode::new();
    let share = share_path(&sender.core, &source_path, 42, "shared.txt", false);

    assert!(sender
        .core
        .approve_endpoint_for_transfer(share.transfer_id, "   ".to_string())
        .is_err());
    assert!(sender
        .core
        .approve_endpoint_for_transfer(999, "endpoint-a".to_string())
        .is_err());
    sender
        .core
        .approve_endpoint_for_transfer(share.transfer_id, "endpoint-a".to_string())
        .unwrap();

    sender.core.cancel_transfer(share.transfer_id).unwrap();
    assert!(sender
        .core
        .approve_endpoint_for_transfer(share.transfer_id, "endpoint-a".to_string())
        .is_err());
}

#[test]
fn source_limit_rejection_creates_no_transfer_state() {
    let core_dir = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let first = source_dir.path().join("one.txt");
    let second = source_dir.path().join("two.txt");
    std::fs::write(&first, b"one").unwrap();
    std::fs::write(&second, b"two").unwrap();
    let limits = CoreLimits {
        max_sources: 1,
        ..CoreLimits::default()
    };
    let sender =
        CoreGuard::start_with_limits(core_dir.path(), Arc::new(RecordingSink::default()), limits);

    let result = sender.share_files(
        vec![
            ShareSource {
                kind: SourceKind::Path,
                value: first.to_string_lossy().to_string(),
                display_name: Some("one.txt".to_string()),
                is_directory: false,
            },
            ShareSource {
                kind: SourceKind::Path,
                value: second.to_string_lossy().to_string(),
                display_name: Some("two.txt".to_string()),
                is_directory: false,
            },
        ],
        ShareMetadataInput {
            transfer_id: 24,
            transfer_name: Some("too many".to_string()),
            sender_name: None,
            access_mode: TransferAccessMode::ApprovalRequired,
        },
    );

    assert!(result.is_err());
    assert!(sender.list_transfers().unwrap().is_empty());
}

#[test]
fn cancellation_during_import_is_durable() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("gated.bin");
    std::fs::write(&source_path, vec![0u8; 256 * 1024]).unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let import_gate = Arc::new(ImportStartedGate::default());
    let sender = CoreGuard::start(data_dir.path(), import_gate.clone());
    let core = sender.arc();
    let worker = std::thread::spawn(move || {
        core.share_files(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source_path.to_string_lossy().to_string(),
                display_name: Some("gated.bin".to_string()),
                is_directory: false,
            }],
            ShareMetadataInput {
                transfer_id: 25,
                transfer_name: Some("gated".to_string()),
                sender_name: None,
                access_mode: TransferAccessMode::ApprovalRequired,
            },
        )
    });

    // The synchronous event callback holds the worker after active-transfer
    // registration, so cancellation cannot race a fast import to completion.
    import_gate.wait_until_blocked();
    let cancel_result = sender.cancel_transfer(25);
    import_gate.release();
    cancel_result.unwrap();
    assert!(worker.join().unwrap().is_err());

    let transfer = sender
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == 25)
        .unwrap();
    assert_eq!(transfer.status, "cancelled");
    assert_eq!(sender.status().active_transfers, 0);
    assert_eq!(sender.status().active_shares, 0);
}

#[derive(Default)]
struct ImportStartedGate {
    state: Mutex<ImportGateState>,
    changed: Condvar,
}

#[derive(Default)]
struct ImportGateState {
    blocked: bool,
    released: bool,
}

impl ImportStartedGate {
    fn wait_until_blocked(&self) {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |state| !state.blocked)
            .unwrap();
        assert!(state.blocked, "import did not reach the start gate");
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.released = true;
        self.changed.notify_all();
    }
}

impl CoreEventSink for ImportStartedGate {
    fn on_event(&self, event: CoreEvent) {
        if event.phase != "import" || event.kind != "started" {
            return;
        }
        let mut state = self.state.lock().unwrap();
        state.blocked = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).unwrap();
        }
    }
}
