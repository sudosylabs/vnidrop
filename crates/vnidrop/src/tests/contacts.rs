use crate::{
    contacts::ContactStore,
    grant::{Challenge, GrantId, GrantLifetime, GrantRejection, HeldGrant, IssuedGrant},
    repository::Repository,
};

const PEER: &str = "peer-endpoint";
const SELF_ID: &str = "self-endpoint";
const NOW: i64 = 1_700_000_000_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

async fn store(temp: &tempfile::TempDir) -> (Repository, ContactStore) {
    let repository = Repository::open(temp.path()).await.unwrap();
    let contacts = repository.contacts();
    (repository, contacts)
}

async fn contact_with_issued_grant(contacts: &ContactStore) -> IssuedGrant {
    contacts
        .upsert_contact(PEER, Some("Peer Laptop"), NOW)
        .await
        .unwrap();
    let grant = IssuedGrant::mint(PEER.to_string(), NOW, GrantLifetime::default());
    contacts.insert_issued_grant(&grant).await.unwrap();
    grant
}

#[tokio::test]
async fn contacts_and_grants_survive_reopening_the_same_data_dir() {
    let temp = tempfile::tempdir().unwrap();
    let minted = {
        let (repository, contacts) = store(&temp).await;
        let grant = contact_with_issued_grant(&contacts).await;
        contacts
            .insert_held_grant(&HeldGrant {
                grant_id: GrantId::generate(),
                secret: grant.secret.clone(),
                peer_endpoint_id: PEER.to_string(),
                created_at: NOW,
                expires_at: Some(NOW + 90 * DAY_MS),
            })
            .await
            .unwrap();
        drop(repository);
        grant
    };

    let (_repository, contacts) = store(&temp).await;

    let reloaded = contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .expect("issued grant persisted");
    assert_eq!(reloaded.secret, minted.secret);
    assert_eq!(reloaded.issued_to_endpoint_id, PEER);
    assert!(contacts.held_grant_for(PEER).await.unwrap().is_some());
    assert_eq!(contacts.list_contacts().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_persisted_grant_still_validates_a_proof() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;

    // The round trip through hex storage must not disturb the secret.
    let reloaded = contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .expect("issued grant persisted");
    let challenge = Challenge::generate();
    let held = HeldGrant {
        grant_id: minted.grant_id,
        secret: minted.secret.clone(),
        peer_endpoint_id: SELF_ID.to_string(),
        created_at: NOW,
        expires_at: None,
    };

    let outcome = reloaded.accept(
        &held.prove(&challenge, PEER),
        &challenge,
        SELF_ID,
        PEER,
        NOW,
        GrantLifetime::default(),
    );

    assert!(outcome.is_ok(), "expected acceptance, got {outcome:?}");
}

#[tokio::test]
async fn renewal_is_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;
    let renewed_to = Some(NOW + 120 * DAY_MS);

    contacts
        .renew_issued_grant(minted.grant_id, renewed_to)
        .await
        .unwrap();

    let reloaded = contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .expect("issued grant persisted");
    assert_eq!(reloaded.expires_at, renewed_to);
}

#[tokio::test]
async fn revocation_is_tombstoned_so_the_peer_learns_it_was_revoked() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;

    contacts
        .revoke_issued_grant(minted.grant_id, NOW)
        .await
        .unwrap();

    let reloaded = contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .expect("a revoked grant is kept as a tombstone, not deleted");
    assert_eq!(reloaded.revoked_at, Some(NOW));

    // A tombstone answers Revoked, never Unknown: the peer needs to know to
    // drop the entry rather than retry forever.
    let challenge = Challenge::generate();
    let held = HeldGrant {
        grant_id: minted.grant_id,
        secret: minted.secret.clone(),
        peer_endpoint_id: SELF_ID.to_string(),
        created_at: NOW,
        expires_at: None,
    };
    assert_eq!(
        reloaded.accept(
            &held.prove(&challenge, PEER),
            &challenge,
            SELF_ID,
            PEER,
            NOW,
            GrantLifetime::default(),
        ),
        Err(GrantRejection::Revoked)
    );
}

#[tokio::test]
async fn deleting_a_contact_removes_both_directions_and_reports_issued_grants() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;
    let held_id = GrantId::generate();
    contacts
        .insert_held_grant(&HeldGrant {
            grant_id: held_id,
            secret: minted.secret.clone(),
            peer_endpoint_id: PEER.to_string(),
            created_at: NOW,
            expires_at: None,
        })
        .await
        .unwrap();

    let to_notify = contacts.delete_contact(PEER).await.unwrap();

    assert_eq!(to_notify, vec![minted.grant_id]);
    assert!(contacts.list_contacts().await.unwrap().is_empty());
    assert!(contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .is_none());
    assert!(contacts.held_grant_for(PEER).await.unwrap().is_none());
}

#[tokio::test]
async fn deleting_all_contacts_clears_every_grant() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    contact_with_issued_grant(&contacts).await;
    contacts
        .upsert_contact("other-peer", None, NOW)
        .await
        .unwrap();
    let other = IssuedGrant::mint("other-peer".to_string(), NOW, GrantLifetime::default());
    contacts.insert_issued_grant(&other).await.unwrap();

    let to_notify = contacts.delete_all_contacts().await.unwrap();

    assert_eq!(to_notify.len(), 2);
    assert!(contacts.list_contacts().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_local_label_is_never_overwritten_by_a_name_the_remote_claims() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    contacts
        .upsert_contact(PEER, Some("Original"), NOW)
        .await
        .unwrap();
    contacts
        .set_contact_label(PEER, Some("My Laptop"))
        .await
        .unwrap();

    contacts
        .upsert_contact(PEER, Some("Totally Not Evil"), NOW + 1)
        .await
        .unwrap();

    let contact = contacts.find_contact(PEER).await.unwrap().expect("contact");
    assert_eq!(contact.local_label.as_deref(), Some("My Laptop"));
    assert_eq!(
        contact.remote_display_name.as_deref(),
        Some("Totally Not Evil"),
        "the claimed name is still recorded, just not promoted to the label"
    );
}

#[tokio::test]
async fn upsert_keeps_the_original_creation_time_and_records_activity() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    contacts.upsert_contact(PEER, None, NOW).await.unwrap();

    contacts
        .upsert_contact(PEER, None, NOW + 5 * DAY_MS)
        .await
        .unwrap();
    contacts
        .touch_transfer(PEER, NOW + 6 * DAY_MS)
        .await
        .unwrap();

    let contact = contacts.find_contact(PEER).await.unwrap().expect("contact");
    assert_eq!(contact.created_at, NOW);
    assert_eq!(contact.last_transfer_at, Some(NOW + 6 * DAY_MS));
}

#[tokio::test]
async fn the_last_known_address_is_remembered_for_later_dialing() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    contacts.upsert_contact(PEER, None, NOW).await.unwrap();

    contacts
        .set_last_known_addr(PEER, "vndaddr1:encoded")
        .await
        .unwrap();

    let contact = contacts.find_contact(PEER).await.unwrap().expect("contact");
    assert_eq!(contact.last_known_addr.as_deref(), Some("vndaddr1:encoded"));
}

#[tokio::test]
async fn blocking_revokes_outstanding_grants_so_it_is_not_merely_cosmetic() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;

    contacts.block_endpoint(PEER, NOW).await.unwrap();

    assert!(contacts.is_blocked(PEER).await.unwrap());
    let reloaded = contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .expect("grant kept as tombstone");
    assert_eq!(reloaded.revoked_at, Some(NOW));
}

#[tokio::test]
async fn unblocking_does_not_restore_the_revoked_grant() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;
    contacts.block_endpoint(PEER, NOW).await.unwrap();

    contacts.unblock_endpoint(PEER).await.unwrap();

    assert!(!contacts.is_blocked(PEER).await.unwrap());
    let reloaded = contacts
        .find_issued_grant(minted.grant_id)
        .await
        .unwrap()
        .expect("grant kept as tombstone");
    assert!(
        reloaded.revoked_at.is_some(),
        "unblocking must not silently hand back access; the peer has to pair again"
    );
}

#[tokio::test]
async fn newest_held_grant_wins_after_re_pairing() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let older = HeldGrant {
        grant_id: GrantId::generate(),
        secret: IssuedGrant::mint(PEER.to_string(), NOW, GrantLifetime::default()).secret,
        peer_endpoint_id: PEER.to_string(),
        created_at: NOW,
        expires_at: None,
    };
    let newer = HeldGrant {
        grant_id: GrantId::generate(),
        secret: IssuedGrant::mint(PEER.to_string(), NOW, GrantLifetime::default()).secret,
        peer_endpoint_id: PEER.to_string(),
        created_at: NOW + DAY_MS,
        expires_at: None,
    };
    contacts.insert_held_grant(&older).await.unwrap();
    contacts.insert_held_grant(&newer).await.unwrap();

    let selected = contacts.held_grant_for(PEER).await.unwrap().expect("grant");

    assert_eq!(selected.grant_id, newer.grant_id);
}

#[tokio::test]
async fn a_held_grant_is_dropped_once_the_issuer_reports_it_dead() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let held = HeldGrant {
        grant_id: GrantId::generate(),
        secret: IssuedGrant::mint(PEER.to_string(), NOW, GrantLifetime::default()).secret,
        peer_endpoint_id: PEER.to_string(),
        created_at: NOW,
        expires_at: None,
    };
    contacts.insert_held_grant(&held).await.unwrap();

    contacts.delete_held_grant(held.grant_id).await.unwrap();

    assert!(contacts.held_grant_for(PEER).await.unwrap().is_none());
}

#[tokio::test]
async fn purging_drops_lapsed_and_revoked_grants_but_keeps_live_ones() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let live = contact_with_issued_grant(&contacts).await;
    let lapsed = IssuedGrant::mint(
        "stale-peer".to_string(),
        NOW - 400 * DAY_MS,
        GrantLifetime::Days(1),
    );
    contacts.insert_issued_grant(&lapsed).await.unwrap();

    let purged = contacts.purge_dead_grants(NOW).await.unwrap();

    assert_eq!(purged, 1);
    assert!(contacts
        .find_issued_grant(live.grant_id)
        .await
        .unwrap()
        .is_some());
    assert!(contacts
        .find_issued_grant(lapsed.grant_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_corrupt_stored_secret_is_an_error_not_a_silent_refusal() {
    let temp = tempfile::tempdir().unwrap();
    let (_repository, contacts) = store(&temp).await;
    let minted = contact_with_issued_grant(&contacts).await;
    contacts
        .corrupt_secret_for_test(minted.grant_id)
        .await
        .unwrap();

    // Refusing the peer here would be indistinguishable from revocation, so the
    // corruption has to surface instead.
    assert!(contacts.find_issued_grant(minted.grant_id).await.is_err());
}
