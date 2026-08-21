//! Prompt Targeted transfer registration with core-owned negotiation.

use std::sync::{Arc, Condvar, Mutex};

use tokio::runtime::Handle;

use super::CoreInner;
use crate::{
    api::{ShareSource, TargetedPreparationStopOutcome, TargetedTransfer},
    error::VnidropError,
};

#[derive(Debug)]
enum PreparationPhase {
    Ready,
    Preparing,
    StopRequested,
    Registering,
    Registered(String),
    Stopping,
    Stopped,
    Failed,
    Finished(TargetedPreparationStopOutcome),
}

pub(super) struct PreparationGate {
    phase: Mutex<PreparationPhase>,
    changed: Condvar,
}

impl PreparationGate {
    fn new() -> Self {
        Self {
            phase: Mutex::new(PreparationPhase::Ready),
            changed: Condvar::new(),
        }
    }

    fn start(&self) -> Result<(), VnidropError> {
        let mut phase = self.phase.lock().expect("targeted preparation phase");
        match *phase {
            PreparationPhase::Ready => {
                *phase = PreparationPhase::Preparing;
                Ok(())
            }
            _ => Err(VnidropError::invalid_input(anyhow::anyhow!(
                "targeted preparation can send only once"
            ))),
        }
    }

    pub(super) fn begin_registration(&self) -> bool {
        let mut phase = self.phase.lock().expect("targeted preparation phase");
        if matches!(*phase, PreparationPhase::StopRequested) {
            return false;
        }
        debug_assert!(matches!(*phase, PreparationPhase::Preparing));
        *phase = PreparationPhase::Registering;
        true
    }

    fn registered(&self, id: String) {
        let mut phase = self.phase.lock().expect("targeted preparation phase");
        debug_assert!(matches!(*phase, PreparationPhase::Registering));
        *phase = PreparationPhase::Registered(id);
        self.changed.notify_all();
    }

    fn failed(&self) {
        let mut phase = self.phase.lock().expect("targeted preparation phase");
        *phase = if matches!(*phase, PreparationPhase::StopRequested) {
            PreparationPhase::Finished(TargetedPreparationStopOutcome::PreparationStopped)
        } else {
            PreparationPhase::Failed
        };
        self.changed.notify_all();
    }

    fn stop_target(&self) -> Result<Option<String>, TargetedPreparationStopOutcome> {
        let mut phase = self.phase.lock().expect("targeted preparation phase");
        loop {
            match &*phase {
                PreparationPhase::Ready => {
                    *phase = PreparationPhase::Stopped;
                    return Err(TargetedPreparationStopOutcome::PreparationStopped);
                }
                PreparationPhase::Preparing => {
                    *phase = PreparationPhase::StopRequested;
                    phase = self
                        .changed
                        .wait(phase)
                        .expect("targeted preparation phase");
                }
                PreparationPhase::StopRequested => {
                    phase = self
                        .changed
                        .wait(phase)
                        .expect("targeted preparation phase");
                }
                PreparationPhase::Registering => {
                    phase = self
                        .changed
                        .wait(phase)
                        .expect("targeted preparation phase");
                }
                PreparationPhase::Registered(id) => {
                    let id = id.clone();
                    *phase = PreparationPhase::Stopping;
                    return Ok(Some(id));
                }
                PreparationPhase::Stopping => {
                    phase = self
                        .changed
                        .wait(phase)
                        .expect("targeted preparation phase");
                }
                PreparationPhase::Stopped | PreparationPhase::Failed => {
                    return Err(TargetedPreparationStopOutcome::PreparationStopped)
                }
                PreparationPhase::Finished(outcome) => return Err(*outcome),
            }
        }
    }

    fn finish(&self, outcome: TargetedPreparationStopOutcome) {
        *self.phase.lock().expect("targeted preparation phase") =
            PreparationPhase::Finished(outcome);
        self.changed.notify_all();
    }

    fn stop_failed(&self, id: String) {
        *self.phase.lock().expect("targeted preparation phase") = PreparationPhase::Registered(id);
        self.changed.notify_all();
    }
}

/// One-shot sender preparation that separates durable registration from negotiation.
#[derive(uniffi::Object)]
pub struct TargetedTransferPreparation {
    inner: Arc<CoreInner>,
    runtime: Handle,
    receiver_endpoint_id: String,
    gate: Arc<PreparationGate>,
}

impl TargetedTransferPreparation {
    pub(super) fn new(
        inner: Arc<CoreInner>,
        runtime: Handle,
        receiver_endpoint_id: String,
    ) -> Self {
        Self {
            inner,
            runtime,
            receiver_endpoint_id,
            gate: Arc::new(PreparationGate::new()),
        }
    }
}

#[uniffi::export]
impl TargetedTransferPreparation {
    /// Import sources and return once the durable Targeted identity is registered.
    pub fn send(
        &self,
        sources: Vec<ShareSource>,
        transfer_name: Option<String>,
    ) -> Result<TargetedTransfer, VnidropError> {
        self.gate.start()?;
        self.inner
            .active_targeted_preparations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner
            .emit_endpoint("runtime_obligation", "changed", serde_json::json!({}));
        let prepared = self.runtime.block_on(self.inner.prepare_targeted_transfer(
            self.receiver_endpoint_id.clone(),
            sources,
            transfer_name,
            Some(self.gate.as_ref()),
        ));
        self.inner
            .active_targeted_preparations
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        self.inner
            .emit_endpoint("runtime_obligation", "changed", serde_json::json!({}));
        let (transfer, continuation) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.gate.failed();
                return Err(error);
            }
        };

        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            if let Err(error) = inner.continue_targeted_transfer(continuation).await {
                tracing::debug!(%error, "targeted transfer negotiation finished with an error");
            }
        });
        self.gate.registered(transfer.id.clone());
        Ok(transfer)
    }

    /// Stop preparation or the transfer registered by it.
    pub fn stop(&self) -> Result<TargetedPreparationStopOutcome, VnidropError> {
        let id = match self.gate.stop_target() {
            Ok(Some(id)) => id,
            Ok(None) => unreachable!("registered preparation always has an id"),
            Err(outcome) => return Ok(outcome),
        };
        self.inner.signal_targeted_transfer_cancel_by_id(&id);
        match self
            .runtime
            .block_on(self.inner.stop_targeted_preparation(&id))
        {
            Ok(outcome) => {
                self.gate.finish(outcome);
                Ok(outcome)
            }
            Err(error) => {
                self.gate.stop_failed(id);
                Err(error)
            }
        }
    }
}
