//! Grants: the capability a device issues so a known peer may reach it.
//!
//! A history entry is not "I remember this endpoint id", it is "this device
//! issued me a capability". The issuer is the only party that can validate a
//! grant, which is what makes both consent and revocation enforceable: refusing
//! to issue leaves the peer with nothing usable, and deleting the issued record
//! ends the relationship without the peer's cooperation.
//!
//! This module is pure: no storage, no network, no clock of its own. Callers
//! supply `now_ms` so expiry and renewal stay testable.

use std::fmt;

use anyhow::{bail, Context, Result};
use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};

/// Domain separator for the possession proof. Changing this invalidates every
/// outstanding grant, so it is versioned rather than edited.
const PROOF_CONTEXT: &[u8] = b"vnidrop-grant-v1";

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

    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; CHALLENGE_LEN]) -> Self {
        Self(bytes)
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
    Expired,
    WrongEndpoint,
    BadProof,
}

impl GrantRejection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::WrongEndpoint => "wrong-endpoint",
            Self::BadProof => "bad-proof",
        }
    }
}

/// A grant as held by the party that issued it. This is the authoritative
/// record: `grants_held` on the peer is only a copy for display.
#[derive(Debug, Clone)]
pub(crate) struct IssuedGrant {
    pub(crate) grant_id: GrantId,
    pub(crate) secret: GrantSecret,
    /// The grant is usable only by this endpoint, so it cannot be lent onward.
    pub(crate) issued_to_endpoint_id: String,
    pub(crate) created_at: i64,
    /// Idle expiry, pushed forward on every accepted proof. `None` never expires.
    pub(crate) expires_at: Option<i64>,
    pub(crate) revoked_at: Option<i64>,
}

impl IssuedGrant {
    pub(crate) fn mint(
        issued_to_endpoint_id: String,
        now_ms: i64,
        lifetime: GrantLifetime,
    ) -> Self {
        Self {
            grant_id: GrantId::generate(),
            secret: GrantSecret::generate(),
            issued_to_endpoint_id,
            created_at: now_ms,
            expires_at: lifetime.deadline_from(now_ms),
            revoked_at: None,
        }
    }

    /// Validate a proof presented by `remote_endpoint_id` over this connection's
    /// challenge. Returns the renewed expiry the caller must persist.
    ///
    /// Checks run in a fixed order so a caller cannot learn more from an early
    /// return than from a late one: revocation and expiry are properties of the
    /// issuer's own record, and the endpoint binding is checked before the MAC
    /// so a stolen grant cannot be probed for validity from another device.
    pub(crate) fn accept(
        &self,
        proof: &GrantProof,
        challenge: &Challenge,
        issuer_endpoint_id: &str,
        remote_endpoint_id: &str,
        now_ms: i64,
        lifetime: GrantLifetime,
    ) -> Result<Option<i64>, GrantRejection> {
        if proof.grant_id != self.grant_id {
            return Err(GrantRejection::Unknown);
        }
        if self.revoked_at.is_some() {
            return Err(GrantRejection::Revoked);
        }
        if self.is_expired(now_ms) {
            return Err(GrantRejection::Expired);
        }
        if remote_endpoint_id != self.issued_to_endpoint_id {
            return Err(GrantRejection::WrongEndpoint);
        }

        let expected = compute_proof(
            &self.secret,
            challenge,
            issuer_endpoint_id,
            remote_endpoint_id,
        );
        // Constant-time: blake3::Hash's PartialEq is constant-time by design.
        if !constant_time_eq(&expected, &proof.mac) {
            return Err(GrantRejection::BadProof);
        }

        Ok(lifetime.deadline_from(now_ms))
    }

    pub(crate) fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at < now_ms)
    }
}

/// A grant as held by the party it was issued to: the capability used to reach
/// the peer that minted it.
///
/// `expires_at` here is advisory only — a copy of what the issuer said at issue
/// time, useful for showing "expires soon" in the UI. The issuer's record is
/// authoritative and may have been renewed or revoked since.
#[derive(Debug, Clone)]
pub(crate) struct HeldGrant {
    pub(crate) grant_id: GrantId,
    pub(crate) secret: GrantSecret,
    /// The peer that issued this grant, and therefore the only one it works on.
    pub(crate) peer_endpoint_id: String,
    pub(crate) created_at: i64,
    pub(crate) expires_at: Option<i64>,
}

impl HeldGrant {
    /// Build the proof to present to the issuing peer.
    pub(crate) fn prove(&self, challenge: &Challenge, self_endpoint_id: &str) -> GrantProof {
        prove(
            self.grant_id,
            &self.secret,
            challenge,
            &self.peer_endpoint_id,
            self_endpoint_id,
        )
    }
}

/// How long a grant survives without use. Grants expire on idleness rather than
/// age, so a relationship in regular use never lapses while a forgotten one
/// cleans itself up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantLifetime {
    Days(u32),
    Never,
}

impl GrantLifetime {
    pub(crate) const DEFAULT_DAYS: u32 = 90;

    pub(crate) fn deadline_from(self, now_ms: i64) -> Option<i64> {
        match self {
            Self::Never => None,
            Self::Days(days) => Some(now_ms + i64::from(days) * 24 * 60 * 60 * 1_000),
        }
    }
}

impl Default for GrantLifetime {
    fn default() -> Self {
        Self::Days(Self::DEFAULT_DAYS)
    }
}

impl From<crate::api::GrantLifetimeSetting> for GrantLifetime {
    fn from(setting: crate::api::GrantLifetimeSetting) -> Self {
        match setting {
            crate::api::GrantLifetimeSetting::Days30 => Self::Days(30),
            crate::api::GrantLifetimeSetting::Days90 => Self::Days(90),
            crate::api::GrantLifetimeSetting::Days365 => Self::Days(365),
            crate::api::GrantLifetimeSetting::Never => Self::Never,
        }
    }
}

/// Build the proof for a grant this device holds.
pub(crate) fn prove(
    grant_id: GrantId,
    secret: &GrantSecret,
    challenge: &Challenge,
    issuer_endpoint_id: &str,
    holder_endpoint_id: &str,
) -> GrantProof {
    GrantProof {
        grant_id,
        mac: compute_proof(secret, challenge, issuer_endpoint_id, holder_endpoint_id),
    }
}

/// Keyed MAC over the challenge and both endpoint identities.
///
/// Binding the challenge stops a captured proof being replayed; binding both
/// endpoint ids stops it being replayed against a different peer. Lengths are
/// prefixed so two different id pairs cannot produce the same input.
fn compute_proof(
    secret: &GrantSecret,
    challenge: &Challenge,
    issuer_endpoint_id: &str,
    holder_endpoint_id: &str,
) -> [u8; PROOF_LEN] {
    let mut input = Vec::with_capacity(
        PROOF_CONTEXT.len()
            + CHALLENGE_LEN
            + issuer_endpoint_id.len()
            + holder_endpoint_id.len()
            + 16,
    );
    input.extend_from_slice(PROOF_CONTEXT);
    input.extend_from_slice(&challenge.0);
    push_length_prefixed(&mut input, issuer_endpoint_id.as_bytes());
    push_length_prefixed(&mut input, holder_endpoint_id.as_bytes());
    *blake3::keyed_hash(&secret.0, &input).as_bytes()
}

fn push_length_prefixed(buffer: &mut Vec<u8>, bytes: &[u8]) {
    buffer.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    buffer.extend_from_slice(bytes);
}

fn constant_time_eq(left: &[u8; PROOF_LEN], right: &[u8; PROOF_LEN]) -> bool {
    // blake3::Hash compares in constant time; reuse it rather than hand-rolling.
    blake3::Hash::from_bytes(*left) == blake3::Hash::from_bytes(*right)
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

/// Parse a stored grant secret, rejecting anything malformed rather than
/// silently producing a grant that can never validate.
pub(crate) fn parse_secret(value: &str) -> Result<GrantSecret> {
    let secret = GrantSecret::decode(value)?;
    if secret.0.iter().all(|byte| *byte == 0) {
        bail!("refusing an all-zero grant secret");
    }
    Ok(secret)
}
