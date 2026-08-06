use crate::grant::{
    parse_secret, prove, Challenge, GrantId, GrantLifetime, GrantRejection, GrantSecret,
    IssuedGrant,
};

const ISSUER: &str = "issuer-endpoint";
const HOLDER: &str = "holder-endpoint";
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

fn issued(now_ms: i64) -> IssuedGrant {
    IssuedGrant::mint(HOLDER.to_string(), now_ms, GrantLifetime::default())
}

fn accept_with(
    grant: &IssuedGrant,
    challenge: &Challenge,
    remote_endpoint_id: &str,
    now_ms: i64,
) -> Result<Option<i64>, GrantRejection> {
    let proof = prove(
        grant.grant_id,
        &grant.secret,
        challenge,
        ISSUER,
        remote_endpoint_id,
    );
    grant.accept(
        &proof,
        challenge,
        ISSUER,
        remote_endpoint_id,
        now_ms,
        GrantLifetime::default(),
    )
}

#[test]
fn accepts_a_valid_proof_and_returns_the_renewed_deadline() {
    let now = 1_700_000_000_000;
    let grant = issued(now);
    let challenge = Challenge::generate();

    let renewed = accept_with(&grant, &challenge, HOLDER, now + DAY_MS).expect("proof accepted");

    assert_eq!(renewed, Some(now + DAY_MS + 90 * DAY_MS));
}

#[test]
fn renewal_extends_past_the_original_expiry() {
    let now = 1_700_000_000_000;
    let grant = issued(now);
    let original = grant.expires_at.expect("default lifetime expires");

    // Used one day before lapsing: the new deadline must be later than the old.
    let use_at = original - DAY_MS;
    let renewed = accept_with(&grant, &Challenge::generate(), HOLDER, use_at)
        .expect("proof accepted")
        .expect("renewed deadline");

    assert!(renewed > original);
}

#[test]
fn rejects_a_proof_bound_to_a_different_challenge() {
    let now = 1_700_000_000_000;
    let grant = issued(now);
    let captured = Challenge::from_bytes([7u8; 32]);
    let proof = prove(grant.grant_id, &grant.secret, &captured, ISSUER, HOLDER);

    // Replaying a captured proof against a fresh challenge must fail.
    let outcome = grant.accept(
        &proof,
        &Challenge::from_bytes([9u8; 32]),
        ISSUER,
        HOLDER,
        now,
        GrantLifetime::default(),
    );

    assert_eq!(outcome, Err(GrantRejection::BadProof));
}

#[test]
fn rejects_a_proof_from_an_endpoint_the_grant_was_not_issued_to() {
    let now = 1_700_000_000_000;
    let grant = issued(now);

    let outcome = accept_with(&grant, &Challenge::generate(), "someone-else", now);

    assert_eq!(outcome, Err(GrantRejection::WrongEndpoint));
}

#[test]
fn rejects_a_proof_replayed_against_a_different_issuer() {
    let now = 1_700_000_000_000;
    let grant = issued(now);
    let challenge = Challenge::generate();
    let proof = prove(
        grant.grant_id,
        &grant.secret,
        &challenge,
        "other-issuer",
        HOLDER,
    );

    let outcome = grant.accept(
        &proof,
        &challenge,
        ISSUER,
        HOLDER,
        now,
        GrantLifetime::default(),
    );

    assert_eq!(outcome, Err(GrantRejection::BadProof));
}

#[test]
fn rejects_a_revoked_grant_distinguishably() {
    let now = 1_700_000_000_000;
    let mut grant = issued(now);
    grant.revoked_at = Some(now);

    // Revocation is reported as such so the peer can drop the dead entry.
    assert_eq!(
        accept_with(&grant, &Challenge::generate(), HOLDER, now),
        Err(GrantRejection::Revoked)
    );
}

#[test]
fn rejects_an_idle_grant_after_its_deadline() {
    let now = 1_700_000_000_000;
    let grant = issued(now);
    let expires_at = grant.expires_at.expect("default lifetime expires");

    assert_eq!(
        accept_with(&grant, &Challenge::generate(), HOLDER, expires_at),
        Ok(Some(expires_at + 90 * DAY_MS)),
        "a grant is still usable on its deadline"
    );
    assert_eq!(
        accept_with(&grant, &Challenge::generate(), HOLDER, expires_at + 1),
        Err(GrantRejection::Expired)
    );
}

#[test]
fn rejects_a_proof_for_a_different_grant_id() {
    let now = 1_700_000_000_000;
    let grant = issued(now);
    let other = issued(now);
    let challenge = Challenge::generate();
    let proof = prove(other.grant_id, &other.secret, &challenge, ISSUER, HOLDER);

    let outcome = grant.accept(
        &proof,
        &challenge,
        ISSUER,
        HOLDER,
        now,
        GrantLifetime::default(),
    );

    assert_eq!(outcome, Err(GrantRejection::Unknown));
}

#[test]
fn never_lifetime_produces_no_deadline() {
    let now = 1_700_000_000_000;
    let grant = IssuedGrant::mint(HOLDER.to_string(), now, GrantLifetime::Never);
    assert_eq!(grant.expires_at, None);

    let challenge = Challenge::generate();
    let proof = prove(grant.grant_id, &grant.secret, &challenge, ISSUER, HOLDER);
    let renewed = grant
        .accept(
            &proof,
            &challenge,
            ISSUER,
            HOLDER,
            now + 10_000 * DAY_MS,
            GrantLifetime::Never,
        )
        .expect("proof accepted");

    assert_eq!(renewed, None);
}

#[test]
fn grant_ids_and_secrets_round_trip_through_storage_encoding() {
    let id = GrantId::generate();
    assert_eq!(GrantId::decode(&id.encode()).expect("decodes"), id);

    let secret = GrantSecret::generate();
    assert_eq!(parse_secret(&secret.encode()).expect("decodes"), secret);
}

#[test]
fn rejects_malformed_or_degenerate_stored_secrets() {
    assert!(parse_secret("not-hex").is_err());
    assert!(parse_secret("aabb").is_err(), "wrong length");
    assert!(
        parse_secret(&"00".repeat(32)).is_err(),
        "an all-zero secret means corrupt storage, not a usable grant"
    );
}

#[test]
fn secrets_are_redacted_in_debug_output() {
    let secret = GrantSecret::generate();
    let rendered = format!("{secret:?}");

    assert!(!rendered.contains(&secret.encode()));
    assert_eq!(rendered, "GrantSecret(redacted)");
}

#[test]
fn generated_grants_are_unique() {
    let now = 1_700_000_000_000;
    let first = issued(now);
    let second = issued(now);

    assert_ne!(first.grant_id, second.grant_id);
    assert_ne!(first.secret, second.secret);
}
