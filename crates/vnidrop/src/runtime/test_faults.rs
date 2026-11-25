use std::sync::{atomic::Ordering, Arc};

use tokio::runtime::Handle;

use super::CoreInner;
use crate::error::VnidropError;

/// Private deterministic failure seams for Targeted transfer characterization.
pub(crate) struct TargetedFaultAdapters {
    pub(crate) store: TargetedStoreFaultAdapter,
    pub(crate) negotiation: TargetedNegotiationFaultAdapter,
    pub(crate) timing: TargetedTimingFaultAdapter,
}

impl TargetedFaultAdapters {
    pub(super) fn new(inner: Arc<CoreInner>, runtime: Handle) -> Self {
        Self {
            store: TargetedStoreFaultAdapter {
                inner: inner.clone(),
                runtime: runtime.clone(),
            },
            negotiation: TargetedNegotiationFaultAdapter {
                inner: inner.clone(),
            },
            timing: TargetedTimingFaultAdapter { inner, runtime },
        }
    }
}

pub(crate) struct TargetedStoreFaultAdapter {
    inner: Arc<CoreInner>,
    runtime: Handle,
}

impl TargetedStoreFaultAdapter {
    pub(crate) fn content_hash(&self, id: &str) -> Result<String, VnidropError> {
        self.runtime.block_on(async {
            self.inner
                .targeted_transfers
                .get_row(id)
                .await?
                .map(|row| row.content_hash)
                .ok_or_else(|| VnidropError::invalid_input(anyhow::anyhow!("unknown transfer")))
        })
    }

    pub(crate) fn corrupt_content_hash(&self, id: &str) -> Result<(), VnidropError> {
        self.runtime.block_on(
            self.inner
                .targeted_transfers
                .corrupt_content_hash_for_test(id),
        )
    }
}

pub(crate) struct TargetedNegotiationFaultAdapter {
    inner: Arc<CoreInner>,
}

impl TargetedNegotiationFaultAdapter {
    pub(crate) fn suppress_authorization_delivery(&self, suppress: bool) {
        self.inner
            .suppress_targeted_authorization_delivery
            .store(suppress, Ordering::SeqCst);
    }

    pub(crate) fn suppress_completion(&self, suppress: bool) {
        self.inner
            .suppress_targeted_completion
            .store(suppress, Ordering::SeqCst);
    }

    pub(crate) fn authorization_delivery_attempts(&self) -> u64 {
        self.inner
            .targeted_authorization_delivery_attempts
            .load(Ordering::SeqCst)
    }
}

pub(crate) struct TargetedTimingFaultAdapter {
    inner: Arc<CoreInner>,
    runtime: Handle,
}

impl TargetedTimingFaultAdapter {
    pub(crate) fn hold_preparation_before_registration(
        &self,
    ) -> TargetedPreparationRegistrationGate {
        hold_preparation(&self.inner.targeted_preparation_registration_gate)
    }

    pub(crate) fn hold_preparation_during_import(&self) -> TargetedPreparationRegistrationGate {
        hold_preparation(&self.inner.targeted_preparation_import_gate)
    }

    pub(crate) fn hold_all_transfer_slots(&self) -> tokio::sync::oneshot::Sender<()> {
        let inner = self.inner.clone();
        let permits = inner.limits.max_concurrent_transfers as u32;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let _permits = inner
                .transfer_slots
                .acquire_many(permits)
                .await
                .expect("transfer limiter open");
            ready_tx.send(()).expect("slot holder ready");
            let _ = release_rx.await;
        });
        ready_rx.recv().expect("slot holder started");
        release_tx
    }
}

pub(super) type TargetedPreparationPauseChannels = std::sync::Mutex<
    Option<(
        std::sync::mpsc::SyncSender<()>,
        tokio::sync::oneshot::Receiver<()>,
    )>,
>;

fn hold_preparation(
    pause: &TargetedPreparationPauseChannels,
) -> TargetedPreparationRegistrationGate {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    *pause.lock().expect("targeted preparation pause") = Some((ready_tx, release_rx));
    TargetedPreparationRegistrationGate {
        ready: ready_rx,
        release: release_tx,
    }
}

pub(crate) struct TargetedPreparationRegistrationGate {
    ready: std::sync::mpsc::Receiver<()>,
    release: tokio::sync::oneshot::Sender<()>,
}

impl TargetedPreparationRegistrationGate {
    pub(crate) fn wait_until_held(&self) {
        self.ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("preparation reached the test pause");
    }

    pub(crate) fn release(self) {
        let _ = self.release.send(());
    }
}

impl CoreInner {
    pub(super) async fn wait_for_targeted_preparation_registration_gate(&self) {
        wait_for_preparation_release(&self.targeted_preparation_registration_gate).await;
    }

    pub(super) async fn wait_for_targeted_preparation_import_gate(&self) {
        wait_for_preparation_release(&self.targeted_preparation_import_gate).await;
    }
}

async fn wait_for_preparation_release(pause: &TargetedPreparationPauseChannels) {
    let gate = pause.lock().expect("targeted preparation pause").take();
    if let Some((ready, release)) = gate {
        let _ = ready.send(());
        let _ = release.await;
    }
}
