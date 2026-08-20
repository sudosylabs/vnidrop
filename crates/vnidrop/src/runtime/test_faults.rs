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
                .targeted_store()
                .get_row(id)
                .await?
                .map(|row| row.content_hash)
                .ok_or_else(|| VnidropError::invalid_input(anyhow::anyhow!("unknown transfer")))
        })
    }

    pub(crate) fn corrupt_content_hash(&self, id: &str) -> Result<(), VnidropError> {
        self.runtime.block_on(
            self.inner
                .targeted_store()
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
