use crate::grant::{Challenge, GrantId, GrantRejection, GrantSecret};

#[test]
fn grant_identifiers_round_trip() {
    let grant_id = GrantId::generate();
    assert_eq!(
        GrantId::decode(&grant_id.encode()).expect("id decodes"),
        grant_id
    );

    let secret = GrantSecret::generate();
    assert_eq!(
        GrantSecret::decode(&secret.encode()).expect("secret decodes"),
        secret
    );
    assert_eq!(format!("{secret:?}"), "GrantSecret(redacted)");

    let challenge = Challenge::generate();
    assert_eq!(
        Challenge::decode(&challenge.encode()).expect("challenge decodes"),
        challenge
    );
}

#[test]
fn grant_secret_decode_rejects_garbage() {
    assert!(GrantSecret::decode("not-hex").is_err());
    assert!(GrantSecret::decode("aabb").is_err(), "wrong length");
}

#[test]
fn grant_rejection_labels_are_stable() {
    assert_eq!(GrantRejection::Unknown.as_str(), "unknown");
    assert_eq!(GrantRejection::Revoked.as_str(), "revoked");
}
