use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tokio::sync::RwLock;

use crate::api::TransferAccessMode;
use crate::util::now_ms;

/// Default lifetime for endpoint approval sessions (matches handshake approval TTL).
pub(crate) const APPROVAL_SESSION_TTL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccessDecision {
    Allow,
    Deny { reason: &'static str },
}

#[derive(Debug, Default)]
pub(crate) struct AccessPolicy {
    modes: RwLock<HashMap<u64, TransferAccess>>,
    approved_sessions: RwLock<HashMap<(u64, String), ApprovalSession>>,
}

#[derive(Debug)]
struct TransferAccess {
    mode: TransferAccessMode,
    lease: AccessLease,
}

#[derive(Debug, Clone)]
pub(crate) struct AccessLease(Arc<AtomicBool>);

impl AccessLease {
    pub(crate) fn is_active(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl AccessPolicy {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) async fn set_mode(&self, transfer_id: u64, mode: TransferAccessMode) {
        let mut modes = self.modes.write().await;
        if let Some(access) = modes.get_mut(&transfer_id) {
            access.mode = mode;
        } else {
            modes.insert(
                transfer_id,
                TransferAccess {
                    mode,
                    lease: AccessLease(Arc::new(AtomicBool::new(true))),
                },
            );
        }
    }

    pub(crate) async fn remove_transfer(&self, transfer_id: u64) {
        let mut modes = self.modes.write().await;
        if let Some(access) = modes.remove(&transfer_id) {
            access.lease.0.store(false, Ordering::Release);
        }
        self.approved_sessions
            .write()
            .await
            .retain(|(id, _), _| *id != transfer_id);
    }

    pub(crate) async fn approve_endpoint(&self, transfer_id: u64, endpoint_id: String) {
        // Never grant permanent sessions from the public API: always expire.
        self.approve_endpoint_until(
            transfer_id,
            endpoint_id,
            Some(now_ms() + APPROVAL_SESSION_TTL_MS),
        )
        .await;
    }

    pub(crate) async fn approve_endpoint_until(
        &self,
        transfer_id: u64,
        endpoint_id: String,
        expires_at: Option<i64>,
    ) {
        self.approved_sessions
            .write()
            .await
            .insert((transfer_id, endpoint_id), ApprovalSession { expires_at });
    }

    pub(crate) async fn decide(
        &self,
        transfer_id: u64,
        endpoint_id: Option<&str>,
    ) -> AccessDecision {
        match self.authorize(transfer_id, endpoint_id).await {
            Ok(_) => AccessDecision::Allow,
            Err(reason) => AccessDecision::Deny { reason },
        }
    }

    pub(crate) async fn authorize(
        &self,
        transfer_id: u64,
        endpoint_id: Option<&str>,
    ) -> Result<AccessLease, &'static str> {
        let modes = self.modes.read().await;
        let access = modes.get(&transfer_id).ok_or("unknown-transfer")?;
        if access.mode == TransferAccessMode::ApprovalRequired {
            let endpoint_id = endpoint_id.ok_or("missing-endpoint-id")?;
            let key = (transfer_id, endpoint_id.to_string());
            let mut sessions = self.approved_sessions.write().await;
            match sessions.get(&key) {
                Some(session) if session.is_valid(now_ms()) => {}
                Some(_) => {
                    sessions.remove(&key);
                    return Err("approval-expired");
                }
                None => return Err("approval-required"),
            }
        }
        Ok(access.lease.clone())
    }
}

pub(crate) fn mode_from_storage(value: &str) -> TransferAccessMode {
    match value {
        "public" => TransferAccessMode::Public,
        _ => TransferAccessMode::ApprovalRequired,
    }
}

pub(crate) fn mode_to_storage(mode: &TransferAccessMode) -> &'static str {
    match mode {
        TransferAccessMode::Public => "public",
        TransferAccessMode::ApprovalRequired => "approval_required",
    }
}

#[derive(Debug, Clone)]
struct ApprovalSession {
    expires_at: Option<i64>,
}

impl ApprovalSession {
    fn is_valid(&self, now: i64) -> bool {
        self.expires_at.is_none_or(|expires_at| expires_at >= now)
    }
}
