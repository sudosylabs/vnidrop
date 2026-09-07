use std::sync::{Arc, Condvar, Mutex};

use async_channel::{Receiver, Sender};
use vnidrop::{
    CoreEvent, CoreEventSink, CoreNetworkConfig, ReceivedArtifact, ReceiverRequest,
    RuntimeObligationFacts, StoredTransfer, VnidropCore, VnidropError,
};

use crate::error::{message_key, Result};
use crate::events::EventHistory;

#[derive(Clone)]
pub struct Snapshot {
    pub transfers: Vec<StoredTransfer>,
    pub requests: Vec<ReceiverRequest>,
    pub artifacts: Vec<ReceivedArtifact>,
    pub obligations: RuntimeObligationFacts,
    pub events: Vec<CoreEvent>,
    pub devices: crate::devices::Devices,
    pub targeted: Vec<vnidrop::TargetedTransfer>,
    pub offers: Vec<vnidrop::PendingTargetedOffer>,
}

impl Snapshot {
    pub fn has_obligations(&self) -> bool {
        let facts = &self.obligations;
        facts.active_invitation_transfers != 0
            || facts.invitation_provider_availability != 0
            || facts.targeted_preparations != 0
            || facts.active_targeted_transfers != 0
            || facts.targeted_provider_availability != 0
    }
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
}

struct EventSink(Sender<()>, Arc<Mutex<EventHistory>>);

impl CoreEventSink for EventSink {
    fn on_event(&self, event: CoreEvent) {
        self.1.lock().unwrap().merge([event]);
        // Wakeups coalesce, but event details survive the interval between GTK refreshes.
        let _ = self.0.try_send(());
    }
}

impl Session {
    pub fn open(profile: String, network: CoreNetworkConfig) -> Result<Arc<Self>> {
        let (sender, changes) = async_channel::bounded(1);
        let events = Arc::new(Mutex::new(EventHistory::default()));
        let core = VnidropCore::initialize_with_network_config(
            profile,
            Arc::new(EventSink(sender, events.clone())),
            network,
        )
        .map_err(message_key)?;
        Self::with_core(core, changes, events)
    }

    fn with_core(
        core: Arc<VnidropCore>,
        changes: Receiver<()>,
        events: Arc<Mutex<EventHistory>>,
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

    pub fn snapshot(&self) -> Result<Snapshot> {
        self.call(|core| {
            let mut transfers = core.list_transfers()?;
            transfers.sort_by_key(|transfer| std::cmp::Reverse(transfer.created_at));
            let mut requests = Vec::new();
            for transfer in &transfers {
                if transfer.direction == "send" {
                    requests.extend(core.list_receiver_requests(transfer.transfer_id)?);
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
        })
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
