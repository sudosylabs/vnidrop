use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::{
    sync::{mpsc, oneshot, Mutex as TokioMutex},
    task::JoinHandle,
};

use crate::{
    api::{CoreEvent, CoreEventSink},
    control_plane::redact_json,
    invitation::Repository,
    transfer_state::TransferDirection,
    util::now_ms,
};

#[derive(Debug, Clone, Copy)]
enum EventScope {
    Endpoint,
    Transfer,
}

impl EventScope {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Endpoint => "endpoint",
            Self::Transfer => "transfer",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum EventPhase {
    Startup,
    Recovery,
    Shutdown,
    Provider,
    Error,
    Import,
    Ticket,
    Lifecycle,
    Network,
    Download,
    Export,
    Access,
    Handshake,
    Approval,
    Transfer,
    /// Delivery receipts from receivers (completed download acknowledgements).
    Delivery,
    /// Saved-device pairing eligibility and consent prompts.
    Pairing,
    /// Saved-device targeted-transfer pre-approval prompts.
    TargetedTransfer,
    /// Payload-free wake-up for platform runtime-retention fact refreshes.
    RuntimeObligation,
}

impl EventPhase {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "startup" => Some(Self::Startup),
            "recovery" => Some(Self::Recovery),
            "shutdown" => Some(Self::Shutdown),
            "provider" => Some(Self::Provider),
            "error" => Some(Self::Error),
            "import" => Some(Self::Import),
            "ticket" => Some(Self::Ticket),
            "lifecycle" => Some(Self::Lifecycle),
            "network" => Some(Self::Network),
            "download" => Some(Self::Download),
            "export" => Some(Self::Export),
            "access" => Some(Self::Access),
            "handshake" => Some(Self::Handshake),
            "approval" => Some(Self::Approval),
            "transfer" => Some(Self::Transfer),
            "delivery" => Some(Self::Delivery),
            "pairing" => Some(Self::Pairing),
            "targeted_transfer" => Some(Self::TargetedTransfer),
            "runtime_obligation" => Some(Self::RuntimeObligation),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Recovery => "recovery",
            Self::Shutdown => "shutdown",
            Self::Provider => "provider",
            Self::Error => "error",
            Self::Import => "import",
            Self::Ticket => "ticket",
            Self::Lifecycle => "lifecycle",
            Self::Network => "network",
            Self::Download => "download",
            Self::Export => "export",
            Self::Access => "access",
            Self::Handshake => "handshake",
            Self::Approval => "approval",
            Self::Transfer => "transfer",
            Self::Delivery => "delivery",
            Self::Pairing => "pairing",
            Self::TargetedTransfer => "targeted_transfer",
            Self::RuntimeObligation => "runtime_obligation",
        }
    }
}

struct EventKind(String);

impl EventKind {
    fn parse(value: &str) -> Option<Self> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        valid.then(|| Self(value.to_string()))
    }
}

enum EventCommand {
    Persist(CoreEvent),
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
}

pub(crate) struct EventHub {
    sink: Arc<dyn CoreEventSink>,
    tx: mpsc::Sender<EventCommand>,
    join: TokioMutex<Option<JoinHandle<()>>>,
    sequence: Mutex<u64>,
}

impl EventHub {
    pub(crate) fn start(
        repository: Repository,
        sink: Arc<dyn CoreEventSink>,
        queue_capacity: usize,
        max_history: u64,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel(queue_capacity);
        let join = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    EventCommand::Persist(event) => {
                        if let Err(error) = repository.insert_event(&event, max_history).await {
                            tracing::warn!(%error, event_id = %event.id, "failed to persist core event");
                        }
                    }
                    EventCommand::Flush(done) => {
                        let _ = done.send(());
                    }
                    EventCommand::Shutdown(done) => {
                        let _ = done.send(());
                        break;
                    }
                }
            }
        });

        Self {
            sink,
            tx,
            join: TokioMutex::new(Some(join)),
            sequence: Mutex::new(1),
        }
    }

    pub(crate) fn emit_endpoint(&self, phase: &str, kind: &str, data: Value) {
        let Some(phase) = EventPhase::parse(phase) else {
            tracing::warn!(phase, "dropped event with unknown phase");
            return;
        };
        let Some(kind) = EventKind::parse(kind) else {
            tracing::warn!(kind, "dropped event with invalid kind");
            return;
        };
        self.emit(EventScope::Endpoint, None, None, phase, kind, data);
    }

    pub(crate) fn emit_transfer(
        &self,
        transfer_id: u64,
        direction: &str,
        phase: &str,
        kind: &str,
        data: Value,
    ) {
        let Ok(direction) = TransferDirection::try_from(direction) else {
            tracing::warn!(direction, "dropped event with unknown direction");
            return;
        };
        let Some(phase) = EventPhase::parse(phase) else {
            tracing::warn!(phase, "dropped event with unknown phase");
            return;
        };
        let Some(kind) = EventKind::parse(kind) else {
            tracing::warn!(kind, "dropped event with invalid kind");
            return;
        };
        self.emit(
            EventScope::Transfer,
            Some(transfer_id),
            Some(direction),
            phase,
            kind,
            data,
        );
    }

    pub(crate) async fn flush(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(EventCommand::Flush(tx)).await.is_ok() {
            let _ = rx.await;
        }
    }

    pub(crate) async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(EventCommand::Shutdown(tx)).await.is_ok() {
            let _ = rx.await;
        }
        if let Some(join) = self.join.lock().await.take() {
            let _ = join.await;
        }
    }

    fn emit(
        &self,
        scope: EventScope,
        transfer_id: Option<u64>,
        direction: Option<TransferDirection>,
        phase: EventPhase,
        kind: EventKind,
        data: Value,
    ) {
        let timestamp = now_ms();
        // A panic while formatting a previous event cannot invalidate an
        // integer counter, so recover the guard instead of cascading a panic.
        let mut sequence = self
            .sequence
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let revision = *sequence;
        let id = format!("{timestamp}-{revision}");
        *sequence = sequence.saturating_add(1);
        drop(sequence);

        // Compose observes this event synchronously, while SQLite persistence is
        // serialized through the queue.  That keeps the UI responsive without
        // losing the ability to flush persisted history during shutdown/tests.
        // Production diagnostics redact endpoint ids, tickets, grants, and paths;
        // typed UniFFI list APIs still expose the values the UI needs.
        let event = CoreEvent {
            id,
            revision,
            timestamp,
            scope: scope.as_str().to_string(),
            transfer_id,
            direction: direction.map(|direction| direction.as_str().to_string()),
            phase: phase.as_str().to_string(),
            kind: kind.0,
            data_json: redact_json(data).to_string(),
        };
        if let Err(error) = self.tx.try_send(EventCommand::Persist(event.clone())) {
            tracing::warn!(event_id = %event.id, %error, "event persistence queue dropped event");
        }
        self.sink.on_event(event);
    }
}
