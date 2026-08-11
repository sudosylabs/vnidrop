//! Grant identity and possession-proof primitives for saved-device relationships.
//!
//! The issuer is the only party that can validate a grant, which is what makes
//! both consent and revocation enforceable. This module is pure: no storage and
//! no network. Relationship-bound MACs live in `device_relationship::crypto`.

use std::fmt;

use anyhow::{Context, Result};
use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};

const GRANT_ID_LEN: usize = 16;
const GRANT_SECRET_LEN: usize = 32;
const CHALLENGE_LEN: usize = 32;
const PROOF_LEN: usize = 32;

/// Opaque public identifier for a grant. Safe to send in the clear.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct GrantId([u8; GRANT_ID_LEN]);

impl GrantId {
    pub(crate) fn generate() -> Self {
        Self(random_bytes())
    }

    pub(crate) fn encode(&self) -> String {
        HEXLOWER.encode(&self.0)
    }

    pub(crate) fn decode(value: &str) -> Result<Self> {
        let bytes = HEXLOWER
            .decode(value.as_bytes())
            .context("invalid grant id encoding")?;
        let bytes: [u8; GRANT_ID_LEN] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid grant id length"))?;
        Ok(Self(bytes))
    }
}

impl fmt::Debug for GrantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GrantId({})", self.encode())
    }
}

/// Key material. Never logged, never emitted in an event, never returned across
/// the UniFFI boundary.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GrantSecret([u8; GRANT_SECRET_LEN]);

impl GrantSecret {
    pub(crate) fn generate() -> Self {
        Self(random_bytes())
    }

    pub(crate) fn as_bytes(&self) -> &[u8; GRANT_SECRET_LEN] {
        &self.0
    }

    pub(crate) fn from_bytes(bytes: [u8; GRANT_SECRET_LEN]) -> Self {
        Self(bytes)
    }

    pub(crate) fn encode(&self) -> String {
        HEXLOWER.encode(&self.0)
    }

    pub(crate) fn decode(value: &str) -> Result<Self> {
        let bytes = HEXLOWER
            .decode(value.as_bytes())
            .context("invalid grant secret encoding")?;
        let bytes: [u8; GRANT_SECRET_LEN] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid grant secret length"))?;
        Ok(Self(bytes))
    }
}

// Redacted on purpose: a secret must not reach a log line through a derived
// Debug on some enclosing struct.
impl fmt::Debug for GrantSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GrantSecret(redacted)")
    }
}

/// Random challenge sent by the issuer to bind a proof to one connection.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Challenge([u8; CHALLENGE_LEN]);

impl Challenge {
    pub(crate) fn generate() -> Self {
        Self(random_bytes())
    }

    pub(crate) fn as_bytes(&self) -> &[u8; CHALLENGE_LEN] {
        &self.0
    }

    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; CHALLENGE_LEN]) -> Self {
        Self(bytes)
    }

    pub(crate) fn encode(&self) -> String {
        HEXLOWER.encode(&self.0)
    }

    pub(crate) fn decode(value: &str) -> Result<Self> {
        let bytes = HEXLOWER
            .decode(value.as_bytes())
            .context("invalid challenge encoding")?;
        let bytes: [u8; CHALLENGE_LEN] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid challenge length"))?;
        Ok(Self(bytes))
    }
}

impl fmt::Debug for Challenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Challenge(..)")
    }
}

/// Proof that the sender holds the secret behind `grant_id`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GrantProof {
    pub(crate) grant_id: GrantId,
    mac: [u8; PROOF_LEN],
}

impl GrantProof {
    pub(crate) fn from_parts(grant_id: GrantId, mac: [u8; PROOF_LEN]) -> Self {
        Self { grant_id, mac }
    }

    pub(crate) fn mac(&self) -> &[u8; PROOF_LEN] {
        &self.mac
    }
}

impl fmt::Debug for GrantProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrantProof")
            .field("grant_id", &self.grant_id)
            .finish_non_exhaustive()
    }
}

/// Why a presented proof was not accepted.
///
/// `Revoked` is reported to the peer so its client can drop the dead entry.
/// `Unknown` is deliberately also used for blocked endpoints, so blocking
/// cannot be detected by probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantRejection {
    Unknown,
    Revoked,
}

impl GrantRejection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Revoked => "revoked",
        }
    }
}

/// Cryptographically secure random bytes.
///
/// Panics if the OS entropy source fails. That is unrecoverable and must never
/// degrade into a weak grant, so it is not surfaced as a fallible API.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("OS entropy source unavailable");
    bytes
}
