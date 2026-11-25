use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};

use tokio::sync::{Mutex, Semaphore};

use crate::api::TransferAccessMode;
use crate::blocked_devices::BlockStore;
use crate::util::now_ms;

/// Default lifetime for endpoint approval sessions (matches handshake approval TTL).
pub(crate) const APPROVAL_SESSION_TTL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccessDecision {
    Allow,
    Deny { reason: &'static str },
}

#[derive(Debug)]
pub(crate) struct AccessPolicy {
    state: Mutex<PolicyState>,
    block_changes: Semaphore,
}

#[derive(Debug, Default)]
struct PolicyState {
    modes: HashMap<u64, TransferAccess>,
    approved_sessions: HashMap<(u64, String), ApprovalSession>,
    blocked: HashSet<String>,
    peer_leases: HashMap<String, Weak<AtomicBool>>,
}

#[derive(Debug)]
struct TransferAccess {
    mode: TransferAccessMode,
    active: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub(crate) struct AccessLease {
    transfer: Arc<AtomicBool>,
    peer: Option<Arc<AtomicBool>>,
}

impl AccessLease {
    pub(crate) fn is_active(&self) -> bool {
        self.transfer.load(Ordering::Acquire)
            && self
                .peer
                .as_ref()
                .is_none_or(|peer| peer.load(Ordering::Acquire))
    }
}

impl AccessPolicy {
    pub(crate) fn new(blocked: impl IntoIterator<Item = String>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(PolicyState {
                blocked: blocked.into_iter().collect(),
                ..PolicyState::default()
            }),
            block_changes: Semaphore::new(1),
        })
    }

    pub(crate) async fn block_endpoint(
        &self,
        store: &BlockStore,
        endpoint_id: &str,
    ) -> anyhow::Result<()> {
        let _permit = self
            .block_changes
            .acquire()
            .await
            .expect("semaphore stays open");
        store.block_endpoint(endpoint_id, now_ms()).await?;
        let mut state = self.state.lock().await;
        state.blocked.insert(endpoint_id.to_owned());
        state
            .approved_sessions
            .retain(|(_, peer), _| peer != endpoint_id);
        if let Some(lease) = state
            .peer_leases
            .remove(endpoint_id)
            .and_then(|lease| lease.upgrade())
        {
            lease.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub(crate) async fn unblock_endpoint(
        &self,
        store: &BlockStore,
        endpoint_id: &str,
    ) -> anyhow::Result<()> {
        let _permit = self
            .block_changes
            .acquire()
            .await
            .expect("semaphore stays open");
        store.unblock_endpoint(endpoint_id).await?;
        self.state.lock().await.blocked.remove(endpoint_id);
        Ok(())
    }

    pub(crate) async fn set_mode(&self, transfer_id: u64, mode: TransferAccessMode) {
        let mut state = self.state.lock().await;
        let modes = &mut state.modes;
        if let Some(access) = modes.get_mut(&transfer_id) {
            access.mode = mode;
        } else {
            modes.insert(
                transfer_id,
                TransferAccess {
                    mode,
                    active: Arc::new(AtomicBool::new(true)),
                },
            );
        }
    }

    pub(crate) async fn remove_transfer(&self, transfer_id: u64) {
        let mut state = self.state.lock().await;
        if let Some(access) = state.modes.remove(&transfer_id) {
            access.active.store(false, Ordering::Release);
        }
        state
            .approved_sessions
            .retain(|(id, _), _| *id != transfer_id);
    }

    pub(crate) async fn approve_endpoint(&self, transfer_id: u64, endpoint_id: String) -> bool {
        // Never grant permanent sessions from the public API: always expire.
        self.approve_endpoint_until(
            transfer_id,
            endpoint_id,
            Some(now_ms() + APPROVAL_SESSION_TTL_MS),
        )
        .await
    }

    pub(crate) async fn approve_endpoint_until(
        &self,
        transfer_id: u64,
        endpoint_id: String,
        expires_at: Option<i64>,
    ) -> bool {
        let mut state = self.state.lock().await;
        if state.blocked.contains(&endpoint_id) {
            return false;
        }
        state
            .approved_sessions
            .insert((transfer_id, endpoint_id), ApprovalSession { expires_at });
        true
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
        let mut state = self.state.lock().await;
        if endpoint_id.is_some_and(|peer| state.blocked.contains(peer)) {
            return Err("not-accepted");
        }
        let access = state.modes.get(&transfer_id).ok_or("unknown-transfer")?;
        let transfer = access.active.clone();
        if access.mode == TransferAccessMode::ApprovalRequired {
            let endpoint_id = endpoint_id.ok_or("missing-endpoint-id")?;
            let key = (transfer_id, endpoint_id.to_string());
            let sessions = &mut state.approved_sessions;
            match sessions.get(&key) {
                Some(session) if session.is_valid(now_ms()) => {}
                Some(_) => {
                    sessions.remove(&key);
                    return Err("approval-expired");
                }
                None => return Err("approval-required"),
            }
        }
        let peer = endpoint_id.map(|endpoint_id| {
            let leases = &mut state.peer_leases;
            if let Some(lease) = leases.get(endpoint_id).and_then(Weak::upgrade) {
                return lease;
            }
            if leases.len() == leases.capacity() {
                leases.retain(|_, lease| lease.strong_count() > 0);
            }
            let lease = Arc::new(AtomicBool::new(true));
            leases.insert(endpoint_id.to_owned(), Arc::downgrade(&lease));
            lease
        });
        Ok(AccessLease { transfer, peer })
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
