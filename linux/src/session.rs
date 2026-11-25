use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex},
};

use async_channel::{Receiver, Sender};
use vnidrop::{
    CoreEvent, CoreEventSink, CoreNetworkConfig, ReceivedArtifact, RuntimeObligationFacts,
    VnidropCore, VnidropError,
};

use crate::error::{message_key, Result};
use crate::events::EventHistory;
use crate::presentation::{
    InvitationDirection, InvitationReceiver, InvitationStatus, InvitationTransfer,
};

#[derive(Clone)]
pub struct Snapshot {
    pub transfers: Vec<InvitationTransfer>,
    pub requests: Vec<InvitationReceiver>,
    pub artifacts: Vec<ReceivedArtifact>,
    pub obligations: RuntimeObligationFacts,
    pub events: Vec<CoreEvent>,
    pub devices: crate::devices::Devices,
    pub targeted: Vec<vnidrop::TargetedTransfer>,
    pub offers: Vec<vnidrop::PendingTargetedOffer>,
}

impl Snapshot {
    pub fn has_obligations(&self) -> bool {
        requires_runtime(&self.obligations)
    }
}

fn requires_runtime(facts: &RuntimeObligationFacts) -> bool {
    facts.active_invitation_transfers != 0
        || facts.invitation_provider_availability != 0
        || facts.targeted_preparations != 0
        || facts.active_targeted_transfers != 0
        || facts.targeted_provider_availability != 0
}

struct State {
    core: Option<Arc<VnidropCore>>,
    closing: bool,
    calls: usize,
}

/// Owns the blocking core boundary; callers dispatch these methods off the UI thread.
pub struct Session {
    state: Mutex<State>,
    drained: Condvar,
    changes: Receiver<()>,
    events: Arc<Mutex<EventHistory>>,
    event_revisions: Arc<Mutex<EventRevisions>>,
    presentation: Mutex<PresentationCache>,
}

#[derive(Default)]
struct EventRevisions {
    snapshot: u64,
    receivers: BTreeSet<u64>,
}

#[derive(Default)]
struct PresentationCache {
    snapshot: Option<(u64, Snapshot)>,
    receivers: BTreeMap<u64, (InvitationStatus, Vec<InvitationReceiver>)>,
}

struct EventSink(
    Sender<()>,
    Arc<Mutex<EventHistory>>,
    Arc<Mutex<EventRevisions>>,
);

impl CoreEventSink for EventSink {
    fn on_event(&self, event: CoreEvent) {
        if !is_invitation_progress(&event) {
            let mut revisions = self.2.lock().unwrap();
            revisions.snapshot += 1;
            if matches!(event.phase.as_str(), "approval" | "access" | "delivery") {
                revisions.receivers.extend(event.transfer_id);
            }
        }
        self.1.lock().unwrap().merge([event]);
        // Wakeups coalesce, but event details survive the interval between GTK refreshes.
        let _ = self.0.try_send(());
    }
}

fn is_invitation_progress(event: &CoreEvent) -> bool {
    event.transfer_id.is_some()
        && matches!(
            event.phase.as_str(),
            "import" | "transfer" | "download" | "export"
        )
        && matches!(
            event.kind.as_str(),
            "copy-progress" | "outboard-progress" | "progress"
        )
}

impl Session {
    pub fn open(profile: String, network: CoreNetworkConfig) -> Result<Arc<Self>> {
        let (sender, changes) = async_channel::bounded(1);
        let events = Arc::new(Mutex::new(EventHistory::default()));
        let event_revisions = Arc::new(Mutex::new(EventRevisions::default()));
        let core = VnidropCore::initialize_with_network_config(
            profile,
            Arc::new(EventSink(sender, events.clone(), event_revisions.clone())),
            network,
        )
        .map_err(message_key)?;
        Self::with_core(core, changes, events, event_revisions)
    }

    fn with_core(
        core: Arc<VnidropCore>,
        changes: Receiver<()>,
        events: Arc<Mutex<EventHistory>>,
        event_revisions: Arc<Mutex<EventRevisions>>,
    ) -> Result<Arc<Self>> {
        let history = core.list_events(None).map_err(message_key)?;
        events.lock().unwrap().merge(history);
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                core: Some(core),
                closing: false,
                calls: 0,
            }),
            drained: Condvar::new(),
            changes,
            events,
            event_revisions,
            presentation: Mutex::new(PresentationCache::default()),
        }))
    }

    pub fn changes(&self) -> Receiver<()> {
        self.changes.clone()
    }

    pub fn call<T>(
        &self,
        operation: impl FnOnce(&VnidropCore) -> std::result::Result<T, VnidropError>,
    ) -> Result<T> {
        let core = {
            let mut state = self.state.lock().unwrap();
            if state.closing {
                return Err("linux_session_closed");
            }
            let core = state.core.clone().ok_or("error_starting_up")?;
            state.calls += 1;
            core
        };
        let _lease = CallLease(self);
        operation(&core).map_err(message_key)
    }

    pub fn transfer_events(&self, transfer_id: u64) -> Result<Vec<CoreEvent>> {
        self.call(|core| core.list_events(Some(transfer_id)))
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        self.call(|core| self.read_snapshot(core, None))
    }

    /// Progress events still reach the UI event history. They do not change the
    /// durable invitation records, device list, or receiver decisions.
    pub fn presentation_snapshot(&self, force: bool) -> Result<Snapshot> {
        let mut cache = self.presentation.lock().unwrap();
        if force {
            cache.receivers.clear();
        }
        let (version, changed) = {
            let revisions = self.event_revisions.lock().unwrap();
            if let Some((version, snapshot)) = &cache.snapshot {
                if !force && *version == revisions.snapshot {
                    let mut current = snapshot.clone();
                    current.events = self.events.lock().unwrap().snapshot();
                    return Ok(current);
                }
            }
            (revisions.snapshot, revisions.receivers.clone())
        };
        let updated = self.call(|core| self.read_snapshot(core, Some((&mut cache, &changed))))?;
        let mut revisions = self.event_revisions.lock().unwrap();
        if revisions.snapshot == version {
            revisions.receivers.clear();
        }
        cache.snapshot = Some((version, updated.clone()));
        Ok(updated)
    }

    fn read_snapshot(
        &self,
        core: &VnidropCore,
        cache: Option<(&mut PresentationCache, &BTreeSet<u64>)>,
    ) -> std::result::Result<Snapshot, VnidropError> {
        let mut transfers: Vec<InvitationTransfer> =
            core.list_transfers()?.into_iter().map(Into::into).collect();
        transfers.sort_by_key(|transfer| std::cmp::Reverse(transfer.created_at));
        let mut requests = Vec::new();
        let sends = transfers
            .iter()
            .filter(|transfer| transfer.role == InvitationDirection::Send);
        if let Some((cache, changed)) = cache {
            let ids: BTreeSet<_> = sends.clone().map(|transfer| transfer.transfer_id).collect();
            cache.receivers.retain(|id, _| ids.contains(id));
            for transfer in sends {
                let id = transfer.transfer_id;
                if changed.contains(&id)
                    || cache
                        .receivers
                        .get(&id)
                        .is_none_or(|(status, _)| *status != transfer.lifecycle)
                {
                    cache.receivers.insert(
                        id,
                        (
                            transfer.lifecycle,
                            core.list_receiver_requests(id)?
                                .into_iter()
                                .map(Into::into)
                                .collect(),
                        ),
                    );
                }
                requests.extend(cache.receivers[&id].1.iter().cloned());
            }
        } else {
            for transfer in sends {
                requests.extend(
                    core.list_receiver_requests(transfer.transfer_id)?
                        .into_iter()
                        .map(InvitationReceiver::from),
                );
            }
        }
        let artifacts = core.list_received_artifacts()?;
        let obligations = core.runtime_obligation_facts()?;
        let targeted = core.list_targeted_transfers()?;
        let offers = core.list_pending_targeted_offers();
        let mut devices = crate::devices::Devices::read(core)?;
        devices.history_peers = targeted
            .iter()
            .filter(|t| t.state != vnidrop::TargetedTransferState::Deleted)
            .map(|t| crate::targeted::peer(t).to_owned())
            .collect();
        // Core reads can overlap synchronous event callbacks; never retain the event lock across them.
        let events = self.events.lock().unwrap().snapshot();
        Ok(Snapshot {
            transfers,
            requests,
            artifacts,
            obligations,
            events,
            devices,
            targeted,
            offers,
        })
    }

    pub fn has_obligations(&self) -> Result<bool> {
        self.call(VnidropCore::runtime_obligation_facts)
            .map(|facts| requires_runtime(&facts))
    }

    pub fn close(&self) {
        let core = {
            let mut state = self.state.lock().unwrap();
            state.closing = true;
            state.core.clone()
        };
        // Shutdown signals receives before waiting for their worker calls to drain.
        if let Some(core) = core {
            core.shutdown();
        }
        let mut state = self.state.lock().unwrap();
        while state.calls != 0 {
            state = self.drained.wait(state).unwrap();
        }
        state.core.take();
        self.changes.close();
    }
}

struct CallLease<'a>(&'a Session);

impl Drop for CallLease<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.calls -= 1;
        self.0.drained.notify_all();
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
