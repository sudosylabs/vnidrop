//! Saved-device control-plane hardening (design §14).
//!
//! Bounds hostile / noisy peers without imposing quotas on transfers the
//! receiver has already accepted.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use data_encoding::HEXLOWER;
use serde_json::{Map, Value};

use crate::util::now_ms;

/// Per-identity quiet period after declines or repeated malformed traffic.
#[derive(Clone)]
pub(crate) struct IdentityCooldown {
    inner: Arc<Mutex<CooldownInner>>,
    cooldown_ms: i64,
    strike_limit: u32,
}

#[derive(Default)]
struct CooldownInner {
    until: HashMap<String, i64>,
    strikes: HashMap<String, u32>,
}

impl IdentityCooldown {
    pub(crate) fn new(cooldown_ms: u64, strike_limit: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CooldownInner::default())),
            cooldown_ms: cooldown_ms as i64,
            strike_limit: strike_limit as u32,
        }
    }

    pub(crate) fn is_cooling(&self, identity: &str) -> bool {
        let now = now_ms();
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.until.retain(|_, until| *until > now);
        state.until.contains_key(identity)
    }

    pub(crate) fn record_decline(&self, identity: &str) {
        let until = now_ms() + self.cooldown_ms;
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.until.insert(identity.to_string(), until);
        state.strikes.remove(identity);
    }

    /// Count a malformed / spoofed / ineligible control-plane message.
    ///
    /// Trips cooldown once the strike limit is reached. Returns whether the
    /// identity is now cooling (including an already-active cooldown).
    pub(crate) fn record_malformed(&self, identity: &str) -> bool {
        if self.is_cooling(identity) {
            return true;
        }
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let strikes = state.strikes.entry(identity.to_string()).or_insert(0);
        *strikes = strikes.saturating_add(1);
        if *strikes >= self.strike_limit {
            state
                .until
                .insert(identity.to_string(), now_ms() + self.cooldown_ms);
            state.strikes.remove(identity);
            return true;
        }
        false
    }

    pub(crate) fn clear_strikes(&self, identity: &str) {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.strikes.remove(identity);
    }
}

const REDACTED: &str = "[redacted]";

/// Keys whose values must never appear in production events / diagnostics.
fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key,
        "endpoint_id"
            | "peer_endpoint_id"
            | "sender_endpoint_id"
            | "receiver_endpoint_id"
            | "from_endpoint_id"
            | "remote_endpoint_id"
            | "local_endpoint_id"
            | "ticket"
            | "blob_ticket"
            | "authorization"
            | "capability"
            | "secret"
            | "grant"
            | "grant_id"
            | "proof"
            | "mac"
            | "filename"
            | "file_name"
            | "path"
            | "display_name"
            | "transfer_name"
            | "sender_display_name"
            | "remote_display_name"
            | "address"
            | "addrs"
            | "relay_url"
            | "relay_urls"
    )
}

/// Stable fingerprint so diagnostics can correlate without leaking raw values.
pub(crate) fn fingerprint(value: &str) -> String {
    let digest = blake3::hash(value.as_bytes());
    let hex = HEXLOWER.encode(digest.as_bytes());
    format!("<redacted:{}>", &hex[..8])
}

pub(crate) fn redact_json(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(redact_object(map)),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_json).collect()),
        other => other,
    }
}

fn redact_object(map: Map<String, Value>) -> Map<String, Value> {
    map.into_iter()
        .map(|(key, value)| {
            if is_sensitive_key(&key) {
                let redacted = match value {
                    Value::String(raw) if !raw.is_empty() => Value::String(fingerprint(&raw)),
                    Value::Null => Value::Null,
                    _ => Value::String(REDACTED.to_string()),
                };
                (key, redacted)
            } else {
                (key, redact_json(value))
            }
        })
        .collect()
}

/// Scrub ticket-like and long opaque blobs from free-form error / log text.
pub(crate) fn redact_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(idx) = rest.find("vnd1:") {
        out.push_str(&rest[..idx]);
        out.push_str(REDACTED);
        rest = &rest[idx + 5..];
        // Skip the remainder of the ticket token (non-whitespace).
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
    out.push_str(rest);
    // Collapse long hex runs that look like endpoint ids / grant material.
    collapse_long_hex(&out)
}

fn collapse_long_hex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            let len = i - start;
            if len >= 32 {
                out.push_str(REDACTED);
            } else {
                out.push_str(&input[start..i]);
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Documented non-enforcement: accepted transfers have no per-device quota.
    const ACCEPTED_TRANSFER_QUOTA_FIELDS: &[&str] = &[
        "max_per_device_files",
        "max_per_device_bytes",
        "max_per_device_bandwidth",
        "max_per_device_transfers",
    ];

    #[test]
    fn cooldown_trips_after_strike_limit_and_isolates_identities() {
        let guard = IdentityCooldown::new(60_000, 3);
        assert!(!guard.record_malformed("a"));
        assert!(!guard.record_malformed("a"));
        assert!(guard.record_malformed("a"));
        assert!(guard.is_cooling("a"));
        assert!(!guard.is_cooling("b"));
        guard.record_decline("b");
        assert!(guard.is_cooling("b"));
        assert!(guard.is_cooling("a"));
    }

    #[test]
    fn redaction_scrubs_sensitive_event_fields() {
        let raw = json!({
            "transfer_id": "ok-to-keep",
            "sender_endpoint_id": "abc123endpointid000000000000000000000000000000000000000000000000",
            "transfer_name": "secret.pdf",
            "ticket": "vnd1:deadbeef",
            "file_count": 2,
            "nested": { "capability": [1, 2, 3], "state": "saved" }
        });
        let redacted = redact_json(raw);
        let obj = redacted.as_object().unwrap();
        assert_eq!(obj.get("transfer_id").unwrap(), "ok-to-keep");
        assert_eq!(obj.get("file_count").unwrap(), 2);
        let sender = obj.get("sender_endpoint_id").unwrap().as_str().unwrap();
        assert!(sender.starts_with("<redacted:"));
        assert!(!sender.contains("abc123"));
        let name = obj.get("transfer_name").unwrap().as_str().unwrap();
        assert!(name.starts_with("<redacted:"));
        assert!(!name.contains("secret"));
        assert!(obj
            .get("ticket")
            .unwrap()
            .as_str()
            .unwrap()
            .starts_with("<redacted:"));
        let nested = obj.get("nested").unwrap().as_object().unwrap();
        assert_eq!(nested.get("capability").unwrap(), REDACTED);
        assert_eq!(nested.get("state").unwrap(), "saved");
    }

    #[test]
    fn redact_text_strips_tickets_and_long_hex() {
        let text = "ticket vnd1:abcDEF123 and id 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let scrubbed = redact_text(text);
        assert!(!scrubbed.contains("vnd1:"));
        assert!(!scrubbed.contains("0123456789abcdef"));
        assert!(scrubbed.contains(REDACTED));
    }

    #[test]
    fn accepted_transfer_quota_fields_are_not_core_limits() {
        // Control-plane hardening must not invent per-device accepted-transfer quotas.
        let encoded = serde_json::to_string(&crate::api::CoreLimits::default()).unwrap();
        for field in ACCEPTED_TRANSFER_QUOTA_FIELDS {
            assert!(
                !encoded.contains(field),
                "CoreLimits must not enforce {field}"
            );
        }
    }
}
