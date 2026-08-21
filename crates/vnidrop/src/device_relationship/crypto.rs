//! Relationship-grant possession proofs.

use crate::{
    error::VnidropError,
    grant::{Challenge, GrantId, GrantProof, GrantSecret},
    secure_secret::SecretMaterial,
};

const RELATIONSHIP_GRANT_CONTEXT: &[u8] = b"vnidrop-relationship-grant-v1";

pub(super) fn encode_relationship_grant_secret(
    secret: &GrantSecret,
) -> Result<SecretMaterial, VnidropError> {
    // Custody stores only the 32-byte secret; issuer/holder/generation/protocol
    // bindings live in the relationship row and are enforced at prove/verify time.
    SecretMaterial::new(secret.as_bytes().to_vec())
}

pub(super) fn secret_from_material(material: &SecretMaterial) -> Result<GrantSecret, VnidropError> {
    let bytes: [u8; 32] =
        material
            .to_vec()
            .try_into()
            .map_err(|_| VnidropError::SecureStorageCorrupted {
                reason: "relationship grant secret has invalid length".to_string(),
            })?;
    Ok(GrantSecret::from_bytes(bytes))
}

pub(super) fn prove_relationship_grant(
    grant_id: GrantId,
    secret: &GrantSecret,
    challenge: &Challenge,
    issuer: &str,
    holder: &str,
    generation: u64,
    protocol_version: u16,
) -> GrantProof {
    let mac = relationship_mac(
        secret,
        challenge,
        issuer,
        holder,
        generation,
        protocol_version,
    );
    GrantProof::from_parts(grant_id, mac)
}

pub(super) fn verify_relationship_grant(
    secret: &GrantSecret,
    proof: &GrantProof,
    challenge: &Challenge,
    issuer: &str,
    holder: &str,
    generation: u64,
    protocol_version: u16,
) -> Result<(), &'static str> {
    let expected = relationship_mac(
        secret,
        challenge,
        issuer,
        holder,
        generation,
        protocol_version,
    );
    if blake3::Hash::from_bytes(expected) != blake3::Hash::from_bytes(*proof.mac()) {
        return Err("bad relationship grant proof");
    }
    Ok(())
}

fn relationship_mac(
    secret: &GrantSecret,
    challenge: &Challenge,
    issuer: &str,
    holder: &str,
    generation: u64,
    protocol_version: u16,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(secret.as_bytes());
    hasher.update(RELATIONSHIP_GRANT_CONTEXT);
    hasher.update(challenge.as_bytes());
    hasher.update(&(issuer.len() as u64).to_le_bytes());
    hasher.update(issuer.as_bytes());
    hasher.update(&(holder.len() as u64).to_le_bytes());
    hasher.update(holder.as_bytes());
    hasher.update(&generation.to_le_bytes());
    hasher.update(&protocol_version.to_le_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod grant_vectors {
    use super::*;
    use crate::api::saved_device_capabilities;
    use data_encoding::HEXLOWER;

    #[test]
    fn relationship_grant_proof_vectors_are_stable() {
        let secret =
            GrantSecret::decode("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let grant_id = GrantId::decode("0123456789abcdef0123456789abcdef").unwrap();
        let challenge = Challenge::from_bytes([9u8; 32]);
        let protocol = saved_device_capabilities().relationship_protocol_version;
        let proof = prove_relationship_grant(
            grant_id, &secret, &challenge, "issuer", "holder", 1, protocol,
        );
        // Binding and replay resistance: wrong holder or challenge must fail.
        verify_relationship_grant(&secret, &proof, &challenge, "issuer", "holder", 1, protocol)
            .unwrap();
        let mac_hex = HEXLOWER.encode(proof.mac());
        assert_eq!(
            mac_hex,
            "faab284de3b13049ee418da86a890c41b07330a023f7810cc347768c6ac32d1a"
        );
        assert!(verify_relationship_grant(
            &secret, &proof, &challenge, "issuer", "other", 1, protocol,
        )
        .is_err());
        let other_challenge = Challenge::from_bytes([8u8; 32]);
        assert!(verify_relationship_grant(
            &secret,
            &proof,
            &other_challenge,
            "issuer",
            "holder",
            1,
            protocol,
        )
        .is_err());
    }
}
