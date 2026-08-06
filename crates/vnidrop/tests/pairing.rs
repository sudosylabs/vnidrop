//! Device history pairing over the offer ALPN, between two real nodes.

mod support;

use std::time::{Duration, Instant};

use support::TestNode;
use vnidrop::VnidropCore;

fn endpoint_id(node: &TestNode) -> String {
    node.core.status().endpoint_id
}

/// The pairing prompt arrives asynchronously on the peer's side.
fn wait_for_pending_pairing(core: &VnidropCore, from_endpoint: &str) {
    let started = Instant::now();
    loop {
        if core
            .list_pending_pairings()
            .iter()
            .any(|pending| pending.endpoint_id == from_endpoint)
        {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "pairing offer from {from_endpoint} never surfaced"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Alice agrees to be reachable by Bob; Bob consents; Bob can now reach Alice.
#[test]
fn a_delivered_grant_becomes_a_contact_only_after_the_peer_consents() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let bob_id = endpoint_id(&bob);
    let alice_id = endpoint_id(&alice);

    alice
        .core
        .allow_device_to_reach_me(bob_id.clone(), Some("Alice Laptop".to_string()))
        .expect("grant delivered");

    // Delivery alone must not create a contact: Bob has not agreed yet.
    wait_for_pending_pairing(&bob.core, &alice_id);
    assert!(
        bob.core.list_contacts().unwrap().is_empty(),
        "an undelivered-consent grant must not appear as a contact"
    );

    assert!(bob
        .core
        .respond_to_pairing(alice_id.clone(), true)
        .expect("consent recorded"));

    let contacts = bob.core.list_contacts().unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].endpoint_id, alice_id);
    assert!(
        contacts[0].can_send,
        "holding a live grant is what makes a contact reachable"
    );
    assert!(bob.core.list_pending_pairings().is_empty());
}

/// Declining leaves nothing behind: no contact, no stored capability.
#[test]
fn declining_a_pairing_stores_nothing() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let alice_id = endpoint_id(&alice);

    alice
        .core
        .allow_device_to_reach_me(endpoint_id(&bob), None)
        .expect("grant delivered");
    wait_for_pending_pairing(&bob.core, &alice_id);

    assert!(bob
        .core
        .respond_to_pairing(alice_id.clone(), false)
        .unwrap());

    assert!(bob.core.list_contacts().unwrap().is_empty());
    assert!(bob.core.list_pending_pairings().is_empty());
    assert!(
        !bob.core.respond_to_pairing(alice_id, true).unwrap(),
        "a declined offer cannot be accepted afterwards"
    );
}

/// The pairing is directional: Alice issuing to Bob does not let Alice reach Bob.
#[test]
fn each_direction_is_a_separate_decision() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let alice_id = endpoint_id(&alice);
    let bob_id = endpoint_id(&bob);

    alice
        .core
        .allow_device_to_reach_me(bob_id.clone(), None)
        .expect("grant delivered");
    wait_for_pending_pairing(&bob.core, &alice_id);
    bob.core.respond_to_pairing(alice_id.clone(), true).unwrap();

    // Alice recorded Bob as a contact when she issued, but she holds no grant
    // from him, so she cannot reach him.
    let alice_contacts = alice.core.list_contacts().unwrap();
    assert_eq!(alice_contacts.len(), 1);
    assert_eq!(alice_contacts[0].endpoint_id, bob_id);
    assert!(
        !alice_contacts[0].can_send,
        "issuing a grant does not grant the issuer anything in return"
    );
}

/// Revoking kills the peer's entry without their cooperation, and tells them.
#[test]
fn forgetting_a_contact_revokes_the_peers_access() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let alice_id = endpoint_id(&alice);
    let bob_id = endpoint_id(&bob);

    alice
        .core
        .allow_device_to_reach_me(bob_id.clone(), None)
        .expect("grant delivered");
    wait_for_pending_pairing(&bob.core, &alice_id);
    bob.core.respond_to_pairing(alice_id.clone(), true).unwrap();
    assert!(bob.core.list_contacts().unwrap()[0].can_send);

    alice.core.forget_contact(bob_id).expect("forgotten");

    // Best-effort notification: Bob is online, so his dead entry should clear
    // promptly rather than at his next attempt.
    let started = Instant::now();
    loop {
        let contacts = bob.core.list_contacts().unwrap();
        let cleared = contacts.first().is_none_or(|contact| !contact.can_send);
        if cleared {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "revocation notice never reached the peer"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(alice.core.list_contacts().unwrap().is_empty());
}

/// A blocked device is refused, and cannot tell blocking from any other refusal.
#[test]
fn a_blocked_device_cannot_pair() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let bob_id = endpoint_id(&bob);

    bob.core
        .block_contact(endpoint_id(&alice))
        .expect("blocked");

    let outcome = alice.core.allow_device_to_reach_me(bob_id, None);

    assert!(outcome.is_err(), "a blocked peer must refuse the grant");
    assert!(bob.core.list_pending_pairings().is_empty());
    assert!(bob.core.list_contacts().unwrap().is_empty());
}

/// Blocking locally also prevents pairing outward, so the block is symmetric
/// from the user's point of view.
#[test]
fn blocking_prevents_issuing_a_grant_to_that_device() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let bob_id = endpoint_id(&bob);

    alice.core.block_contact(bob_id.clone()).expect("blocked");

    let outcome = alice.core.allow_device_to_reach_me(bob_id.clone(), None);
    assert!(outcome.is_err());

    alice
        .core
        .unblock_contact(bob_id.clone())
        .expect("unblocked");
    assert!(alice.core.list_blocked_contacts().unwrap().is_empty());
}

/// Re-pairing an existing contact refreshes the grant without a second prompt.
#[test]
fn re_pairing_a_known_contact_does_not_prompt_again() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let alice_id = endpoint_id(&alice);
    let bob_id = endpoint_id(&bob);

    alice
        .core
        .allow_device_to_reach_me(bob_id.clone(), None)
        .unwrap();
    wait_for_pending_pairing(&bob.core, &alice_id);
    bob.core.respond_to_pairing(alice_id.clone(), true).unwrap();

    alice
        .core
        .allow_device_to_reach_me(bob_id, None)
        .expect("re-issued");

    assert!(
        bob.core.list_pending_pairings().is_empty(),
        "an established contact must not raise a fresh consent prompt"
    );
    assert_eq!(bob.core.list_contacts().unwrap().len(), 1);
}

/// The user's own label survives whatever the remote later calls itself.
#[test]
fn a_local_label_survives_a_remote_rename() {
    let alice = TestNode::new();
    let bob = TestNode::new();
    let alice_id = endpoint_id(&alice);
    let bob_id = endpoint_id(&bob);

    alice
        .core
        .allow_device_to_reach_me(bob_id.clone(), Some("Alice Laptop".to_string()))
        .unwrap();
    wait_for_pending_pairing(&bob.core, &alice_id);
    bob.core.respond_to_pairing(alice_id.clone(), true).unwrap();
    bob.core
        .set_contact_label(alice_id.clone(), Some("Work Mac".to_string()))
        .unwrap();

    alice
        .core
        .allow_device_to_reach_me(bob_id, Some("Totally Not Evil".to_string()))
        .unwrap();

    let contact = bob
        .core
        .list_contacts()
        .unwrap()
        .into_iter()
        .find(|contact| contact.endpoint_id == alice_id)
        .expect("contact");
    assert_eq!(contact.local_label.as_deref(), Some("Work Mac"));
}
