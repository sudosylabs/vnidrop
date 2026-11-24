//! Control-plane hardening: offer bounds, cooldowns, saved-device cap, redaction.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use crate::{
    access_policy::{AccessDecision, AccessPolicy},
    api::{CoreEvent, CoreEventSink, CoreLimits, PendingTargetedOffer},
    approval::ApprovalService,
    control_plane::IdentityCooldown,
    event_hub::EventHub,
    handshake::{
        DeliveryFailureReceipt, DeliveryReceiptResponse, HandshakeResponse, RequestTransfer,
    },
    invitation::{ReceiverRequestInsert, Repository, TransferUpsert},
    persistence::{self, AppDataStores},
    secure_secret::FaultInjectingSecretStore,
    targeted_transfer::inbox::{TargetedOfferDecision, TargetedOfferInbox},
    targeted_transfer::{TargetedAuthorization, TargetedAuthorizationDraft},
    transfer_state::{ReceiverRequestStatus, TransferDirection, TransferStatus},
    CoreNetworkConfig, DeviceRelationshipState, ShareMetadataInput, ShareSource, SourceKind,
    TransferAccessMode, VnidropCore, VnidropError,
};

struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

#[tokio::test]
async fn delivered_authorization_must_match_the_approved_offer_projection() {
    let (inbox, _) = inbox_with_limits(1, 60_000, 5).await;
    let offer = sample_offer("bound-offer", "sender-a");
    let submit = {
        let inbox = inbox.clone();
        let offer = offer.clone();
        tokio::spawn(async move { inbox.submit(offer).await })
    };
    let started = std::time::Instant::now();
    while inbox.list().await.is_empty() {
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let exact = TargetedAuthorization::issue(TargetedAuthorizationDraft {
        transfer_id: offer.transfer_id.clone(),
        protocol_transfer_id: 42,
        sender_endpoint_id: offer.sender_endpoint_id.clone(),
        receiver_endpoint_id: offer.receiver_endpoint_id.clone(),
        manifest_id: offer.manifest_id.clone(),
        content_hash: offer.content_hash.clone(),
        file_count: offer.file_count,
        total_size: offer.total_size,
        protocol_version: offer.protocol_version,
        transfer_name: offer.transfer_name.clone(),
        blob_ticket: "blob-a".to_string(),
    })
    .unwrap();
    assert!(inbox.authorization_matches_pending(&exact).await);

    let substituted = TargetedAuthorization::issue(TargetedAuthorizationDraft {
        manifest_id: "replacement-manifest".to_string(),
        content_hash: "replacement-hash".to_string(),
        transfer_name: "replacement.txt".to_string(),
        total_size: 99,
        blob_ticket: "blob-b".to_string(),
        ..TargetedAuthorizationDraft {
            transfer_id: offer.transfer_id,
            protocol_transfer_id: 42,
            sender_endpoint_id: offer.sender_endpoint_id,
            receiver_endpoint_id: offer.receiver_endpoint_id,
            manifest_id: offer.manifest_id,
            content_hash: offer.content_hash,
            file_count: offer.file_count,
            total_size: offer.total_size,
            protocol_version: offer.protocol_version,
            transfer_name: offer.transfer_name,
            blob_ticket: "blob-a".to_string(),
        }
    })
    .unwrap();
    assert!(!inbox.authorization_matches_pending(&substituted).await);
    assert!(
        inbox.list().await.len() == 1,
        "rejection keeps the approved offer pending"
    );
    inbox.discard("bound-offer").await;
    let _ = submit.await;
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

    fn kinds(&self) -> Vec<String> {
        self.events().into_iter().map(|event| event.kind).collect()
    }
}

async fn approval_service_for_test(
    path: &Path,
    policy: Arc<AccessPolicy>,
) -> (
    ApprovalService,
    AppDataStores,
    Arc<EventHub>,
    Arc<RecordingSink>,
) {
    let stores = persistence::open_all(path).await.unwrap();
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let hub = Arc::new(EventHub::start(
        stores.invitation.clone(),
        sink.clone(),
        64,
        100,
    ));
    let approval = ApprovalService::new(
        stores.invitation.clone(),
        stores.blocked.clone(),
        hub.clone(),
        policy,
        5,
        7,
        None,
    );
    (approval, stores, hub, sink)
}

#[tokio::test]
async fn approval_events_ignore_unverified_transfer_requests() {
    let temp = tempfile::tempdir().unwrap();
    let (approval, stores, hub, sink) =
        approval_service_for_test(temp.path(), AccessPolicy::new([])).await;
    stores.blocked.block_endpoint("blocked", 1).await.unwrap();
    let request = RequestTransfer {
        transfer_id: 42,
        transfer_hash: "missing".to_owned(),
        transfer_name: "demo".to_owned(),
        receiver_name: None,
        receiver_device_name: None,
        app_version: "0.1.0".to_owned(),
    };

    assert_eq!(
        approval
            .request_transfer("unrecognized".into(), request.clone())
            .await,
        HandshakeResponse::Denied {
            reason: "unknown-transfer".into()
        }
    );
    assert_eq!(
        approval
            .request_transfer("blocked".into(), request.clone())
            .await,
        HandshakeResponse::Denied {
            reason: "not-accepted".into()
        }
    );
    assert_eq!(
        approval
            .request_transfer(
                "unrecognized".into(),
                RequestTransfer {
                    transfer_name: "oversized".to_owned(),
                    ..request
                }
            )
            .await,
        HandshakeResponse::Denied {
            reason: "metadata-too-large".into()
        }
    );
    hub.flush().await;
    assert!(sink.events().is_empty());
    assert!(stores
        .invitation
        .list_events(None, 100)
        .await
        .unwrap()
        .is_empty());
    hub.shutdown().await;
}

#[tokio::test]
async fn approval_events_bound_delivery_failure_reasons_and_ignore_retries() {
    let temp = tempfile::tempdir().unwrap();
    let (approval, stores, hub, sink) =
        approval_service_for_test(temp.path(), AccessPolicy::new([])).await;
    stores
        .invitation
        .insert_receiver_request(ReceiverRequestInsert {
            id: "request",
            transfer_id: 42,
            remote_endpoint_id: "receiver",
            transfer_name: "demo",
            receiver_name: None,
            receiver_device_name: None,
            app_version: "0.1.0",
        })
        .await
        .unwrap();
    stores
        .invitation
        .update_receiver_request_status("request", ReceiverRequestStatus::Accepted, None)
        .await
        .unwrap();
    stores
        .invitation
        .set_receiver_receipt_token("request", &blake3::hash(b"test-token").to_hex())
        .await
        .unwrap();
    let receipt = DeliveryFailureReceipt {
        request_id: "request".to_owned(),
        transfer_id: 42,
        token: "test-token".to_owned(),
        reason: "éabcdé-over-budget".to_owned(),
    };

    for _ in 0..2 {
        assert_eq!(
            approval
                .fail_delivery("receiver".to_owned(), receipt.clone())
                .await,
            DeliveryReceiptResponse::Recorded
        );
    }
    let requests = stores.invitation.list_receiver_requests(42).await.unwrap();
    assert_eq!(requests[0].status, "failed");
    assert_eq!(requests[0].reason.as_deref(), Some("éabcd"));
    assert_eq!(sink.kinds(), ["receiver-failed"]);
    hub.shutdown().await;
}

#[tokio::test]
async fn approval_does_not_grant_access_when_receipt_token_cannot_be_saved() {
    let temp = tempfile::tempdir().unwrap();
    let policy = AccessPolicy::new([]);
    policy
        .set_mode(42, TransferAccessMode::ApprovalRequired)
        .await;
    let (approval, stores, hub, sink) =
        approval_service_for_test(temp.path(), policy.clone()).await;
    stores
        .invitation
        .insert_transfer(TransferUpsert {
            transfer_id: 42,
            peer_id: None,
            direction: TransferDirection::Send,
            status: TransferStatus::Sharing,
            transfer_name: Some("demo"),
            sender_name: None,
            content_hash: Some("hash"),
            ticket: None,
            file_count: 1,
            total_size: 12,
            access_mode: "approval_required",
        })
        .await
        .unwrap();
    let sql = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(temp.path().join("vnidrop.sqlite3")),
    )
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER test_fail_receipt_token BEFORE UPDATE OF receipt_token_hash ON receiver_requests
                 BEGIN SELECT RAISE(ABORT, 'injected receipt token write failure'); END")
        .execute(&sql).await.unwrap();

    let handshake = {
        let approval = approval.clone();
        tokio::spawn(async move {
            approval
                .request_transfer(
                    "receiver".into(),
                    RequestTransfer {
                        transfer_id: 42,
                        transfer_hash: "hash".to_owned(),
                        transfer_name: "demo".to_owned(),
                        receiver_name: None,
                        receiver_device_name: None,
                        app_version: "0.1.0".to_owned(),
                    },
                )
                .await
        })
    };
    let request = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(request) = stores
                .invitation
                .list_receiver_requests(42)
                .await
                .unwrap()
                .pop()
            {
                break request;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("approval request should be persisted");
    approval.respond(request.id, true, None).await.unwrap();
    let response = tokio::time::timeout(std::time::Duration::from_secs(2), handshake)
        .await
        .expect("handshake should finish")
        .unwrap();
    assert_eq!(
        response,
        HandshakeResponse::Denied {
            reason: "repository-error".into()
        }
    );
    assert_eq!(
        policy.decide(42, Some("receiver")).await,
        AccessDecision::Deny {
            reason: "approval-required"
        }
    );
    assert!(!sink.kinds().iter().any(|kind| kind == "receiver-accepted"));
    hub.shutdown().await;
}

fn sample_offer(transfer_id: &str, sender: &str) -> PendingTargetedOffer {
    PendingTargetedOffer {
        transfer_id: transfer_id.to_string(),
        sender_endpoint_id: sender.to_string(),
        receiver_endpoint_id: "receiver".to_string(),
        manifest_id: "manifest".to_string(),
        content_hash: "hash".to_string(),
        transfer_name: "secret-name.pdf".to_string(),
        file_count: 1,
        total_size: 12,
        protocol_version: 1,
        received_at: 1,
    }
}

async fn inbox_with_limits(
    max_pending: usize,
    cooldown_ms: u64,
    strikes: u64,
) -> (TargetedOfferInbox, Arc<RecordingSink>) {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let sink = Arc::new(RecordingSink {
        events: Mutex::new(Vec::new()),
    });
    let hub = Arc::new(EventHub::start(repository, sink.clone(), 64, 100));
    let cooldown = IdentityCooldown::new(cooldown_ms, strikes);
    let inbox = TargetedOfferInbox::new(hub, max_pending, cooldown, 5_000);
    // Keep temp dir alive for the hub's repository by leaking — tests are short-lived.
    std::mem::forget(temp);
    (inbox, sink)
}

#[tokio::test]
async fn inbox_bounds_recent_settled_offer_decisions() {
    let (inbox, sink) = inbox_with_limits(1, 60_000, 5).await;

    for index in 0..=256 {
        let transfer_id = format!("offer-{index}");
        let offer = sample_offer(&transfer_id, &format!("sender-{index}"));
        let event_count = sink.events.lock().unwrap().len();
        let submission = {
            let inbox = inbox.clone();
            tokio::spawn(async move { inbox.submit(offer).await })
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while sink.events.lock().unwrap().len() == event_count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("offer should become visible");
        inbox.respond(&transfer_id, false).await.unwrap();
        assert!(matches!(
            submission.await.unwrap(),
            TargetedOfferDecision::Declined { .. }
        ));
    }

    assert!(!inbox.is_settled("offer-0").await);
    assert!(inbox.is_settled("offer-256").await);
}

#[tokio::test]
async fn inbox_delivers_a_durable_authorization_signal_when_the_listener_arrives_later() {
    let (inbox, sink) = inbox_with_limits(1, 60_000, 5).await;
    let submission = {
        let inbox = inbox.clone();
        tokio::spawn(async move { inbox.submit(sample_offer("approved-offer", "sender")).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while sink.events.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("offer should become visible");

    inbox.accept_live("approved-offer").await.unwrap();
    inbox.authorization_stored("approved-offer").await;
    inbox
        .wait_for_authorization("approved-offer")
        .await
        .unwrap();
    assert_eq!(submission.await.unwrap(), TargetedOfferDecision::Accepted);
    assert!(inbox.is_settled("approved-offer").await);
}

#[tokio::test]
async fn one_unresolved_offer_per_sender_and_global_queue_bound() {
    let (inbox, sink) = inbox_with_limits(1, 60_000, 5).await;

    let first = sample_offer("t1", "sender-a");
    let submit_first = {
        let inbox = inbox.clone();
        tokio::spawn(async move { inbox.submit(first).await })
    };
    // Wait until the prompt is live.
    let started = std::time::Instant::now();
    loop {
        if !inbox.list().await.is_empty() {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(sink.kinds().contains(&"offer-received".to_string()));

    let same_sender = inbox.submit(sample_offer("t2", "sender-a")).await;
    assert_eq!(
        same_sender,
        TargetedOfferDecision::Refused {
            reason: "offer-already-pending".to_string()
        }
    );

    let other_sender = inbox.submit(sample_offer("t3", "sender-b")).await;
    assert_eq!(
        other_sender,
        TargetedOfferDecision::Refused {
            reason: "too-many-pending-offers".to_string()
        }
    );
    assert_eq!(inbox.list().await.len(), 1);
    // Excess rejects never emit a second prompt.
    assert_eq!(
        sink.kinds()
            .into_iter()
            .filter(|kind| kind == "offer-received")
            .count(),
        1
    );

    inbox.respond("t1", false).await.unwrap();
    let _ = submit_first.await.unwrap();
}

#[tokio::test]
async fn decline_cools_sender_without_affecting_unrelated_devices() {
    let (inbox, _sink) = inbox_with_limits(8, 60_000, 5).await;
    let offer = sample_offer("decline-1", "noisy");
    let wait = {
        let inbox = inbox.clone();
        tokio::spawn(async move { inbox.submit(offer).await })
    };
    let started = std::time::Instant::now();
    loop {
        if inbox.get_pending("decline-1").await.is_some() {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    inbox.respond("decline-1", false).await.unwrap();
    let _ = wait.await.unwrap();

    let cooled = inbox.submit(sample_offer("decline-2", "noisy")).await;
    assert_eq!(
        cooled,
        TargetedOfferDecision::Refused {
            reason: "identity-cooldown".to_string()
        }
    );
    assert!(inbox.list().await.is_empty());

    let unrelated = {
        let inbox = inbox.clone();
        tokio::spawn(async move { inbox.submit(sample_offer("ok", "friend")).await })
    };
    let started = std::time::Instant::now();
    loop {
        if inbox.get_pending("ok").await.is_some() {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    inbox.respond("ok", false).await.unwrap();
    let _ = unrelated.await.unwrap();
}

#[tokio::test]
async fn malformed_strikes_trip_cooldown() {
    let cooldown = IdentityCooldown::new(60_000, 2);
    assert!(!cooldown.record_malformed("attacker"));
    assert!(cooldown.record_malformed("attacker"));
    assert!(cooldown.is_cooling("attacker"));
    assert!(!cooldown.is_cooling("bystander"));
}

#[tokio::test]
async fn offer_received_events_redact_endpoint_ids_and_names() {
    let (inbox, sink) = inbox_with_limits(4, 60_000, 5).await;
    let sender = "abc123endpointid000000000000000000000000000000000000000000000000";
    let wait = {
        let inbox = inbox.clone();
        let offer = sample_offer("redact-1", sender);
        tokio::spawn(async move { inbox.submit(offer).await })
    };
    let started = std::time::Instant::now();
    loop {
        if sink
            .kinds()
            .into_iter()
            .any(|kind| kind == "offer-received")
        {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let event = sink
        .events()
        .into_iter()
        .find(|event| event.kind == "offer-received")
        .expect("offer event");
    assert!(!event.data_json.contains(sender));
    assert!(!event.data_json.contains("secret-name"));
    assert!(event.data_json.contains("redacted"));
    inbox.respond("redact-1", false).await.unwrap();
    let _ = wait.await.unwrap();
}

struct ProtectedNode {
    _data_dir: tempfile::TempDir,
    _secret_store: Arc<FaultInjectingSecretStore>,
    _limits: CoreLimits,
    _network_config: CoreNetworkConfig,
    sink: Arc<RecordingSink>,
    core: Option<Arc<VnidropCore>>,
}

impl ProtectedNode {
    fn with_limits(limits: CoreLimits) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let store = Arc::new(FaultInjectingSecretStore::default());
        let network_config = CoreNetworkConfig::default();
        let core = VnidropCore::initialize_with_test_secret_store_limits_and_network(
            data_dir.path().to_string_lossy().into_owned(),
            sink.clone(),
            store.clone(),
            limits.clone(),
            network_config.clone(),
        )
        .expect("protected test core");
        Self {
            _data_dir: data_dir,
            _secret_store: store,
            _limits: limits,
            _network_config: network_config,
            sink,
            core: Some(core),
        }
    }

    fn core(&self) -> Arc<VnidropCore> {
        self.core.as_ref().expect("core alive").clone()
    }
}

impl Drop for ProtectedNode {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

fn share_path(
    core: &VnidropCore,
    source: &std::path::Path,
    transfer_id: u64,
) -> crate::ShareResult {
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
    let started = std::time::Instant::now();
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
            started.elapsed() < std::time::Duration::from_secs(15),
            "timed out waiting for receiver request"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn complete_transfer(sender: &ProtectedNode, receiver: &ProtectedNode, transfer_id: u64) {
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("hello.txt");
    std::fs::write(&source_path, b"mutual consent").unwrap();
    let share = share_path(&sender.core(), &source_path, transfer_id);
    let output_dir = output_dir.path().to_string_lossy().to_string();
    let receiver_core = receiver.core().clone();
    let ticket = share.ticket.clone();
    let handle = std::thread::spawn(move || {
        receiver_core.receive(ticket, output_dir, Some("receiver".to_string()))
    });
    let request = wait_for_receiver_request(&sender.core(), share.transfer_id);
    sender
        .core()
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().unwrap();

    let started = std::time::Instant::now();
    let peer = receiver.core().status().endpoint_id.clone();
    loop {
        if sender
            .core()
            .list_pairing_eligibilities()
            .unwrap()
            .iter()
            .any(|entry| entry.peer_endpoint_id == peer)
        {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "eligibility never appeared"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn establish_saved(alice: &ProtectedNode, bob: &ProtectedNode, transfer_id: u64) {
    complete_transfer(alice, bob, transfer_id);
    let bob_id = bob.core().status().endpoint_id.clone();
    assert!(alice
        .core()
        .request_saved_device_pairing(bob_id.clone())
        .unwrap());
    let started = std::time::Instant::now();
    loop {
        let pending = bob
            .core()
            .list_device_relationships()
            .unwrap()
            .into_iter()
            .find(|entry| {
                entry.remote_endpoint_id == alice.core().status().endpoint_id
                    && entry.state == DeviceRelationshipState::PendingIncoming
            });
        if pending.is_some() {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "pairing prompt never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(bob
        .core()
        .respond_to_device_pairing(alice.core().status().endpoint_id.clone(), true)
        .unwrap());
    let started = std::time::Instant::now();
    loop {
        if alice.core().list_saved_devices().unwrap().len() == 1
            && bob.core().list_saved_devices().unwrap().len() == 1
        {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "saved relationship never activated"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn default_limits_include_saved_device_cap_and_control_plane_timeouts() {
    let limits = CoreLimits::default();
    limits.validate().unwrap();
    assert!(limits.identity_cooldown_ms > 0);
    assert!(limits.malformed_strike_limit > 0);
    assert!(limits.pairing_timeout_ms > 0);
    assert!(limits.offer_timeout_ms > 0);
    assert!(limits.connection_timeout_ms > 0);
    assert!(
        limits.max_pending_offers <= 64,
        "pending offers stay tightly bounded"
    );
}

#[test]
fn saved_device_cap_blocks_only_new_relationships() {
    let tight = CoreLimits {
        max_saved_devices: 1,
        ..CoreLimits::default()
    };
    let alice = ProtectedNode::with_limits(tight.clone());
    let bob = ProtectedNode::with_limits(tight.clone());
    let carol = ProtectedNode::with_limits(tight);

    establish_saved(&alice, &bob, 13_001);
    assert_eq!(alice.core().list_saved_devices().unwrap().len(), 1);

    // Existing relationship remains usable for targeted transfer.
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"still works").unwrap();
    let bob_id = bob.core().status().endpoint_id.clone();
    let bob_core = bob.core().clone();
    let accept = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let offer = loop {
            if let Some(offer) = bob_core.list_pending_targeted_offers().into_iter().next() {
                break offer;
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(20),
                "offer never arrived for existing saved peer"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        bob_core
            .respond_to_targeted_offer(offer.transfer_id, true)
            .unwrap()
    });
    alice
        .core()
        .new_targeted_transfer_preparation(bob_id)
        .unwrap()
        .send(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source_path.to_string_lossy().into_owned(),
                display_name: Some("payload.txt".to_string()),
                is_directory: false,
            }],
            Some("payload.txt".to_string()),
        )
        .unwrap();
    accept.join().unwrap();

    // New relationship is refused while the cap is full.
    complete_transfer(&alice, &carol, 13_002);
    let carol_id = carol.core().status().endpoint_id.clone();
    assert!(
        !alice.core().request_saved_device_pairing(carol_id).unwrap(),
        "cap must block only new relationships"
    );
    assert!(alice.core().list_saved_devices().unwrap().len() <= 1);
    assert!(carol.core().list_saved_devices().unwrap().is_empty());
}

#[test]
fn silent_reject_keeps_invalid_offers_off_the_prompt_surface() {
    let alice = ProtectedNode::with_limits(CoreLimits::default());
    let bob = ProtectedNode::with_limits(CoreLimits::default());
    // No Saved relationship — create must fail before any receiver prompt.
    let bob_id = bob.core().status().endpoint_id.clone();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("payload.txt");
    std::fs::write(&source_path, b"nope").unwrap();
    let err = alice
        .core()
        .new_targeted_transfer_preparation(bob_id)
        .err()
        .expect("unsaved peer must be rejected");
    assert!(matches!(err, VnidropError::Permission { .. }));
    assert!(bob.core().list_pending_targeted_offers().is_empty());
    assert!(
        !bob.sink
            .kinds()
            .into_iter()
            .any(|kind| kind == "offer-received"),
        "silent reject must not emit offer prompts"
    );
}

#[test]
fn blocked_peer_cannot_create_pairing_prompt() {
    let alice = ProtectedNode::with_limits(CoreLimits::default());
    let bob = ProtectedNode::with_limits(CoreLimits::default());
    complete_transfer(&alice, &bob, 13_010);
    let alice_id = alice.core().status().endpoint_id.clone();
    bob.core().block_device(alice_id.clone()).unwrap();
    assert!(
        !alice
            .core()
            .request_saved_device_pairing(bob.core().status().endpoint_id.clone())
            .unwrap()
            || bob
                .core()
                .list_device_relationships()
                .unwrap()
                .iter()
                .all(|entry| entry.state != DeviceRelationshipState::PendingIncoming)
    );
    // Stronger: after block, bob must not surface an incoming pairing prompt.
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(bob
        .core()
        .list_device_relationships()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.remote_endpoint_id == alice_id)
        .all(|entry| entry.state != DeviceRelationshipState::PendingIncoming));
}

#[test]
fn ticket_errors_redact_raw_ticket_blobs() {
    let err = VnidropError::ticket(anyhow::anyhow!(
        "bad ticket vnd1:abcDEF1234567890 and endpoint 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    ));
    let rendered = err.to_string();
    assert!(!rendered.contains("vnd1:abcDEF"));
    assert!(!rendered.contains("0123456789abcdef0123456789abcdef"));
    assert!(rendered.contains("[redacted]"));
}
