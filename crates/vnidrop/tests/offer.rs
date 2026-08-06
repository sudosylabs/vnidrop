//! Send-to-contact offers between two real nodes.

mod support;

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use support::{RecordingSink, TestNode};
use vnidrop::{
    ContactSendResult, IncomingOffer, ShareMetadataInput, ShareSource, SourceKind,
    TransferAccessMode, VnidropCore, VnidropError,
};

fn endpoint_id(node: &TestNode) -> String {
    node.core.status().endpoint_id
}

/// Establish a one-way relationship: `issuer` becomes reachable by `holder`.
fn pair(issuer: &TestNode, holder: &TestNode) {
    let issuer_id = endpoint_id(issuer);
    issuer
        .core
        .allow_device_to_reach_me(endpoint_id(holder), Some("Issuer".to_string()))
        .expect("grant delivered");

    let started = Instant::now();
    while !holder
        .core
        .list_pending_pairings()
        .iter()
        .any(|pending| pending.endpoint_id == issuer_id)
    {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "pairing offer never surfaced"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    holder
        .core
        .respond_to_pairing(issuer_id, true)
        .expect("consent recorded");
}

fn sources(path: &Path) -> Vec<ShareSource> {
    vec![ShareSource {
        kind: SourceKind::Path,
        value: path.to_string_lossy().to_string(),
        display_name: Some("shared.txt".to_string()),
        is_directory: false,
    }]
}

fn metadata(transfer_id: u64) -> ShareMetadataInput {
    ShareMetadataInput {
        transfer_id,
        transfer_name: Some("shared.txt".to_string()),
        sender_name: Some("Sender".to_string()),
        access_mode: TransferAccessMode::ApprovalRequired,
    }
}

/// Send in the background: the call blocks until the receiver decides.
fn send_in_background(
    core: Arc<VnidropCore>,
    to: String,
    path: &Path,
    transfer_id: u64,
) -> std::thread::JoinHandle<Result<ContactSendResult, VnidropError>> {
    let sources = sources(path);
    std::thread::spawn(move || core.send_to_contact(to, sources, metadata(transfer_id)))
}

fn wait_for_offer(core: &VnidropCore) -> IncomingOffer {
    let started = Instant::now();
    loop {
        if let Some(offer) = core.list_pending_offers().into_iter().next() {
            return offer;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "offer never surfaced on the receiver"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The whole point: the receiver is asked exactly once, the sender not at all.
#[test]
fn an_accepted_offer_transfers_without_prompting_the_sender() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"offered content").unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();
    // The sender must be able to reach the receiver, so the receiver issues.
    pair(&receiver, &sender);

    let handle = send_in_background(
        sender.core.arc(),
        endpoint_id(&receiver),
        &source_path,
        4_001,
    );

    let offer = wait_for_offer(&receiver.core);
    assert_eq!(offer.from_endpoint_id, endpoint_id(&sender));
    assert_eq!(offer.transfer_name, "shared.txt");
    assert_eq!(offer.file_count, 1);
    assert_eq!(offer.sender_display_name.as_deref(), Some("Sender"));

    let ticket = receiver
        .core
        .respond_to_offer(offer.offer_id, true)
        .expect("accepting yields the ticket");
    let share = handle.join().unwrap().expect("offer accepted");

    receiver
        .core
        .receive(
            ticket,
            output_dir.path().to_string_lossy().to_string(),
            Some("Receiver".to_string()),
        )
        .expect("receive completes");

    assert_eq!(
        std::fs::read(output_dir.path().join("shared.txt")).unwrap(),
        b"offered content"
    );

    // The sender was never asked: the only receiver request on its side was
    // recorded as already approved.
    let requests = sender
        .core
        .list_receiver_requests(share.share.transfer_id)
        .unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        matches!(requests[0].status.as_str(), "accepted" | "completed"),
        "sender should not have been prompted, got status {}",
        requests[0].status
    );
    assert!(requests[0].reason.is_none());
}

/// Declining yields no ticket and stops the share.
#[test]
fn a_declined_offer_yields_no_ticket() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"offered content").unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();
    pair(&receiver, &sender);

    let handle = send_in_background(
        sender.core.arc(),
        endpoint_id(&receiver),
        &source_path,
        4_002,
    );
    let offer = wait_for_offer(&receiver.core);

    assert!(
        receiver
            .core
            .respond_to_offer(offer.offer_id, false)
            .is_none(),
        "a declined offer must not hand over a ticket"
    );

    let outcome = handle.join().unwrap();
    assert!(outcome.is_err(), "sender should see the refusal");
    assert!(receiver.core.list_pending_offers().is_empty());
}

/// A device with no grant cannot offer at all.
#[test]
fn sending_without_a_grant_is_refused_locally() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();

    let outcome = sender.core.send_to_contact(
        endpoint_id(&receiver),
        sources(&source_path),
        metadata(4_003),
    );

    assert!(outcome.is_err(), "no grant means nothing to send with");
    assert!(receiver.core.list_pending_offers().is_empty());
}

/// After the peer revokes, the offer is refused and the dead grant is dropped.
#[test]
fn a_revoked_grant_cannot_be_used_to_offer() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();
    pair(&receiver, &sender);
    // The receiver decides it no longer wants to hear from the sender.
    receiver
        .core
        .forget_contact(endpoint_id(&sender))
        .expect("forgotten");

    let outcome = sender.core.send_to_contact(
        endpoint_id(&receiver),
        sources(&source_path),
        metadata(4_004),
    );

    assert!(outcome.is_err());
    assert!(receiver.core.list_pending_offers().is_empty());
    let contacts = sender.core.list_contacts().unwrap();
    assert!(
        contacts.iter().all(|contact| !contact.can_send),
        "a refusal naming a dead grant must clear the sender's belief it can reach them"
    );
}

/// An offer-created share is never public, whatever the caller asked for.
#[test]
fn an_offer_share_is_never_public() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();
    pair(&receiver, &sender);

    let core = sender.core.arc();
    let to = endpoint_id(&receiver);
    let sources = sources(&source_path);
    let handle = std::thread::spawn(move || {
        core.send_to_contact(
            to,
            sources,
            ShareMetadataInput {
                transfer_id: 4_005,
                transfer_name: Some("shared.txt".to_string()),
                sender_name: None,
                // Deliberately asking for the wider mode.
                access_mode: TransferAccessMode::Public,
            },
        )
    });

    let offer = wait_for_offer(&receiver.core);
    receiver.core.respond_to_offer(offer.offer_id, true);
    let share = handle.join().unwrap().expect("offer accepted");

    let stored = sender
        .core
        .list_transfers()
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.transfer_id == share.share.transfer_id)
        .expect("share recorded");
    assert_eq!(stored.access_mode, TransferAccessMode::ApprovalRequired);
}

/// A second offer while one is on screen is refused rather than stacked.
#[test]
fn only_one_offer_per_device_is_pending_at_a_time() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();
    pair(&receiver, &sender);

    let first = send_in_background(
        sender.core.arc(),
        endpoint_id(&receiver),
        &source_path,
        4_006,
    );
    wait_for_offer(&receiver.core);

    let second = sender.core.send_to_contact(
        endpoint_id(&receiver),
        sources(&source_path),
        metadata(4_007),
    );
    assert!(second.is_err(), "a second prompt must not stack");
    assert_eq!(receiver.core.list_pending_offers().len(), 1);

    let offer = receiver.core.list_pending_offers().remove(0);
    receiver.core.respond_to_offer(offer.offer_id, false);
    let _ = first.join().unwrap();
}

/// Forgetting a device clears any prompt it left on screen, which would
/// otherwise be actionable with a grant that no longer exists.
#[test]
fn forgetting_a_device_clears_its_pending_offer() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let receiver = TestNode::new();
    pair(&receiver, &sender);

    let handle = send_in_background(
        sender.core.arc(),
        endpoint_id(&receiver),
        &source_path,
        4_008,
    );
    wait_for_offer(&receiver.core);

    receiver
        .core
        .forget_contact(endpoint_id(&sender))
        .expect("forgotten");

    assert!(receiver.core.list_pending_offers().is_empty());
    assert!(handle.join().unwrap().is_err());
}

/// The ordinary QR path still prompts the sender: pre-authorisation applies
/// only to transfers the sender pushed.
#[test]
fn an_ordinary_ticket_receive_still_prompts_the_sender() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let sender_dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(RecordingSink::default());
    let sender = support::CoreGuard::start(sender_dir.path(), sink);
    let receiver = TestNode::new();

    let share = sender
        .share_files(sources(&source_path), metadata(4_009))
        .expect("shared");

    let core = receiver.core.arc();
    let ticket = share.ticket.clone();
    let output = output_dir.path().to_string_lossy().to_string();
    let handle =
        std::thread::spawn(move || core.receive(ticket, output, Some("Receiver".to_string())));

    let request = support::wait_for_receiver_request(&sender, share.transfer_id);
    assert_eq!(
        request.status, "requested",
        "an unsolicited ticket receive must still ask the sender"
    );
    sender
        .respond_receiver_request(request.id, true, None)
        .unwrap();
    handle.join().unwrap().expect("receive completes");
}

// MARK: - Held offers and the foreground pull

/// Restartable node, for simulating a device that was not running.
struct RestartableNode {
    dir: tempfile::TempDir,
    core: Option<support::CoreGuard>,
}

impl RestartableNode {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let core = support::CoreGuard::start(dir.path(), Arc::new(RecordingSink::default()));
        Self {
            dir,
            core: Some(core),
        }
    }

    fn core(&self) -> &VnidropCore {
        self.core.as_ref().expect("node is running")
    }

    fn stop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }

    fn start(&mut self) {
        self.core = Some(support::CoreGuard::start(
            self.dir.path(),
            Arc::new(RecordingSink::default()),
        ));
    }
}

/// Pair so `sender` may reach the restartable node.
fn pair_with_restartable(sender: &TestNode, receiver: &RestartableNode) {
    let receiver_id = receiver.core().status().endpoint_id;
    receiver
        .core()
        .allow_device_to_reach_me(endpoint_id(sender), Some("Receiver".to_string()))
        .expect("grant delivered");

    let started = Instant::now();
    while !sender
        .core
        .list_pending_pairings()
        .iter()
        .any(|pending| pending.endpoint_id == receiver_id)
    {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "pairing offer never surfaced"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    sender
        .core
        .respond_to_pairing(receiver_id, true)
        .expect("consent recorded");
}

/// The whole point of §11: a closed app is not an error, it is a delay.
#[test]
fn an_offer_to_a_device_that_is_not_running_is_held_and_collected_later() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"held content").unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let sender = TestNode::new();
    let mut receiver = RestartableNode::new();
    pair_with_restartable(&sender, &receiver);
    let receiver_id = receiver.core().status().endpoint_id;

    receiver.stop();

    let outcome = sender
        .core
        .send_to_contact(receiver_id, sources(&source_path), metadata(5_001))
        .expect("an unreachable device is not a failure");
    assert!(
        !outcome.delivered,
        "nothing was delivered, the offer is waiting"
    );
    let held = sender.core.list_held_offers().unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].transfer_id, outcome.share.transfer_id);

    receiver.start();
    let collected = receiver
        .core()
        .poll_contacts_for_offers()
        .expect("poll succeeds");

    assert_eq!(collected, 1);
    let offer = receiver.core().list_pending_offers().remove(0);
    assert_eq!(offer.transfer_name, "shared.txt");

    let ticket = receiver
        .core()
        .respond_to_offer(offer.offer_id, true)
        .expect("accepting yields the ticket");
    receiver
        .core()
        .receive(
            ticket,
            output_dir.path().to_string_lossy().to_string(),
            Some("Receiver".to_string()),
        )
        .expect("receive completes");

    assert_eq!(
        std::fs::read(output_dir.path().join("shared.txt")).unwrap(),
        b"held content"
    );
    assert!(
        sender.core.list_held_offers().unwrap().is_empty(),
        "a collected offer is no longer held"
    );
}

/// Collected offers are consumed, so a second pull does not re-deliver them.
#[test]
fn polling_twice_does_not_collect_the_same_offer_again() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let mut receiver = RestartableNode::new();
    pair_with_restartable(&sender, &receiver);
    let receiver_id = receiver.core().status().endpoint_id;
    receiver.stop();
    sender
        .core
        .send_to_contact(receiver_id, sources(&source_path), metadata(5_002))
        .unwrap();

    // A fresh core each time, so the per-device poll rate limit does not mask
    // the consume-on-delivery behaviour being asserted here.
    receiver.start();
    assert_eq!(receiver.core().poll_contacts_for_offers().unwrap(), 1);
    receiver.stop();
    receiver.start();
    assert_eq!(
        receiver.core().poll_contacts_for_offers().unwrap(),
        0,
        "the offer was already handed over"
    );
}

/// Cancelling the transfer withdraws the ticket that was waiting for pickup.
#[test]
fn cancelling_a_transfer_withdraws_its_held_offer() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"content").unwrap();

    let sender = TestNode::new();
    let mut receiver = RestartableNode::new();
    pair_with_restartable(&sender, &receiver);
    let receiver_id = receiver.core().status().endpoint_id;
    receiver.stop();
    let outcome = sender
        .core
        .send_to_contact(receiver_id, sources(&source_path), metadata(5_004))
        .unwrap();
    assert_eq!(sender.core.list_held_offers().unwrap().len(), 1);

    sender
        .core
        .cancel_transfer(outcome.share.transfer_id)
        .unwrap();

    assert!(sender.core.list_held_offers().unwrap().is_empty());
    receiver.start();
    assert_eq!(receiver.core().poll_contacts_for_offers().unwrap(), 0);
}

/// A device with no relationship learns nothing by polling.
#[test]
fn polling_a_device_that_holds_nothing_for_you_returns_nothing() {
    let sender = TestNode::new();
    let receiver = TestNode::new();
    pair(&receiver, &sender);

    assert_eq!(receiver.core.poll_contacts_for_offers().unwrap(), 0);
    assert!(receiver.core.list_pending_offers().is_empty());
}
