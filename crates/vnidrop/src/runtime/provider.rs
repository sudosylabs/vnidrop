use std::sync::Arc;

use iroh_blobs::{
    provider::events::{ProviderMessage, RequestUpdate},
    Hash,
};
use serde_json::json;
use tokio::sync::mpsc;

use super::CoreInner;
use crate::access_policy::AccessDecision;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RequestStreamOutcome {
    TerminalUpdateReceived,
    Aborted,
}

pub(crate) async fn consume_request_updates(
    mut rx: irpc::channel::mpsc::Receiver<RequestUpdate>,
    mut handle_update: impl FnMut(RequestUpdate),
) -> RequestStreamOutcome {
    let mut terminal_update_received = false;
    while let Ok(Some(update)) = rx.recv().await {
        terminal_update_received |= matches!(
            update,
            RequestUpdate::Completed(_) | RequestUpdate::Aborted(_)
        );
        handle_update(update);
    }
    if terminal_update_received {
        RequestStreamOutcome::TerminalUpdateReceived
    } else {
        RequestStreamOutcome::Aborted
    }
}

impl CoreInner {
    pub(super) async fn spawn_provider_event_task(
        self: &Arc<Self>,
        mut rx: mpsc::Receiver<ProviderMessage>,
    ) {
        let core = self.clone();
        let task = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                core.handle_provider_message(message).await;
            }
        });
        *self.provider_task.lock().await = Some(task);
    }

    pub(super) async fn handle_provider_message(self: &Arc<Self>, message: ProviderMessage) {
        match message {
            ProviderMessage::ClientConnected(message) => {
                self.emit_endpoint(
                    "provider",
                    "client-connected",
                    json!({
                        "connection_id": message.inner.connection_id,
                        "endpoint_id": message.inner.endpoint_id.map(|id| id.to_string()),
                    }),
                );
                if let Some(endpoint_id) = message.inner.endpoint_id {
                    self.connection_endpoints
                        .lock()
                        .await
                        .insert(message.inner.connection_id, endpoint_id.to_string());
                }
                let _ = message.tx.send(Ok(())).await;
            }
            ProviderMessage::ClientConnectedNotify(message) => {
                self.emit_endpoint(
                    "provider",
                    "client-connected",
                    json!({
                        "connection_id": message.inner.connection_id,
                        "endpoint_id": message.inner.endpoint_id.map(|id| id.to_string()),
                    }),
                );
                if let Some(endpoint_id) = message.inner.endpoint_id {
                    self.connection_endpoints
                        .lock()
                        .await
                        .insert(message.inner.connection_id, endpoint_id.to_string());
                }
            }
            ProviderMessage::ConnectionClosed(message) => {
                self.connection_endpoints
                    .lock()
                    .await
                    .remove(&message.inner.connection_id);
                self.emit_endpoint(
                    "provider",
                    "connection-closed",
                    json!({ "connection_id": message.inner.connection_id }),
                );
            }
            ProviderMessage::GetRequestReceived(message) => {
                match self
                    .authorize_hash(message.inner.request.hash, message.inner.connection_id)
                    .await
                {
                    Ok(transfer_id) => {
                        self.track_request_updates(
                            transfer_id,
                            message.inner.connection_id,
                            message.inner.request_id,
                            message.rx,
                        )
                        .await;
                        let _ = message.tx.send(Ok(())).await;
                    }
                    Err(reason) => {
                        self.emit_denied_request(
                            message.inner.connection_id,
                            message.inner.request_id,
                            reason,
                        );
                        let _ = message
                            .tx
                            .send(Err(iroh_blobs::provider::events::AbortReason::Permission))
                            .await;
                    }
                }
            }
            ProviderMessage::GetRequestReceivedNotify(message) => {
                if let Ok(transfer_id) = self
                    .authorize_hash(message.inner.request.hash, message.inner.connection_id)
                    .await
                {
                    self.track_request_updates(
                        transfer_id,
                        message.inner.connection_id,
                        message.inner.request_id,
                        message.rx,
                    )
                    .await;
                }
            }
            ProviderMessage::GetManyRequestReceived(message) => {
                match self
                    .authorize_hashes(&message.inner.request.hashes, message.inner.connection_id)
                    .await
                {
                    Ok(transfer_id) => {
                        self.track_request_updates(
                            transfer_id,
                            message.inner.connection_id,
                            message.inner.request_id,
                            message.rx,
                        )
                        .await;
                        let _ = message.tx.send(Ok(())).await;
                    }
                    Err(reason) => {
                        self.emit_denied_request(
                            message.inner.connection_id,
                            message.inner.request_id,
                            reason,
                        );
                        let _ = message
                            .tx
                            .send(Err(iroh_blobs::provider::events::AbortReason::Permission))
                            .await;
                    }
                }
            }
            ProviderMessage::GetManyRequestReceivedNotify(message) => {
                if let Ok(transfer_id) = self
                    .authorize_hashes(&message.inner.request.hashes, message.inner.connection_id)
                    .await
                {
                    self.track_request_updates(
                        transfer_id,
                        message.inner.connection_id,
                        message.inner.request_id,
                        message.rx,
                    )
                    .await;
                }
            }
            ProviderMessage::ObserveRequestReceived(message) => {
                // Observe can leak presence of content; use the same ACL as get.
                match self
                    .authorize_hash(message.inner.request.hash, message.inner.connection_id)
                    .await
                {
                    Ok(_) => {
                        self.emit_endpoint(
                            "provider",
                            "observe-request",
                            json!({
                                "connection_id": message.inner.connection_id,
                                "request_id": message.inner.request_id,
                            }),
                        );
                        let _ = message.tx.send(Ok(())).await;
                    }
                    Err(reason) => {
                        self.emit_endpoint(
                            "provider",
                            "observe-denied",
                            json!({
                                "connection_id": message.inner.connection_id,
                                "request_id": message.inner.request_id,
                                "reason": reason,
                            }),
                        );
                        let _ = message
                            .tx
                            .send(Err(iroh_blobs::provider::events::AbortReason::Permission))
                            .await;
                    }
                }
            }
            ProviderMessage::ObserveRequestReceivedNotify(message) => {
                self.emit_endpoint(
                    "provider",
                    "observe-request",
                    json!({
                        "connection_id": message.inner.connection_id,
                        "request_id": message.inner.request_id,
                    }),
                );
            }
            ProviderMessage::Throttle(message) => {
                self.emit_endpoint(
                    "provider",
                    "throttle-request",
                    json!({
                        "connection_id": message.inner.connection_id,
                        "request_id": message.inner.request_id,
                        "size": message.inner.size,
                    }),
                );
                let _ = message.tx.send(Ok(())).await;
            }
            other => {
                self.emit_endpoint(
                    "provider",
                    "provider-message",
                    json!({ "debug": format!("{other:?}") }),
                );
            }
        }
    }

    fn emit_denied_request(&self, connection_id: u64, request_id: u64, reason: &'static str) {
        self.emit_endpoint(
            "provider",
            "request-denied",
            json!({
                "connection_id": connection_id,
                "request_id": request_id,
                "reason": reason,
            }),
        );
    }

    /// Default-deny: hash must belong to an active share the peer may read.
    pub(super) async fn authorize_hash(
        &self,
        hash: Hash,
        connection_id: u64,
    ) -> Result<u64, &'static str> {
        let transfer_ids = self.transfer_ids_for_hash(hash).await;
        if transfer_ids.is_empty() {
            return Err("unknown-hash");
        }
        self.allow_any_transfer(&transfer_ids, connection_id).await
    }

    /// Every hash in a multi-get must be authorized; progress is attributed to
    /// the first allowing transfer id.
    pub(super) async fn authorize_hashes(
        &self,
        hashes: &[Hash],
        connection_id: u64,
    ) -> Result<u64, &'static str> {
        if hashes.is_empty() {
            return Err("empty-request");
        }
        let mut attributed = None;
        for hash in hashes {
            let transfer_id = self.authorize_hash(*hash, connection_id).await?;
            attributed.get_or_insert(transfer_id);
        }
        attributed.ok_or("empty-request")
    }

    pub(super) async fn transfer_ids_for_hash(&self, hash: Hash) -> Vec<u64> {
        self.hash_to_transfer
            .lock()
            .await
            .get(&hash.to_string())
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    async fn allow_any_transfer(
        &self,
        transfer_ids: &[u64],
        connection_id: u64,
    ) -> Result<u64, &'static str> {
        let endpoint_id = self
            .connection_endpoints
            .lock()
            .await
            .get(&connection_id)
            .cloned();
        let mut last_reason = "approval-required";
        for transfer_id in transfer_ids {
            match self
                .access_policy
                .decide(*transfer_id, endpoint_id.as_deref())
                .await
            {
                AccessDecision::Allow => return Ok(*transfer_id),
                AccessDecision::Deny { reason } => last_reason = reason,
            }
        }
        Err(last_reason)
    }

    pub(super) async fn track_request_updates(
        self: &Arc<Self>,
        transfer_id: u64,
        connection_id: u64,
        request_id: u64,
        rx: irpc::channel::mpsc::Receiver<RequestUpdate>,
    ) {
        // Request update tasks are tied to individual provider streams.  Router
        // shutdown closes those streams; only the long-lived provider receiver
        // is tracked directly for explicit shutdown.
        //
        // Attach the remote endpoint id when known so the send UI can attribute
        // byte progress to a specific receiver (not just an opaque connection).
        let endpoint_id = self
            .connection_endpoints
            .lock()
            .await
            .get(&connection_id)
            .cloned();
        let core = self.clone();
        tokio::spawn(async move {
            let outcome = consume_request_updates(rx, |update| match update {
                RequestUpdate::Started(started) => core.emit_transfer(
                    transfer_id,
                    "send",
                    "transfer",
                    "started",
                    json!({
                        "connection_id": connection_id,
                        "request_id": request_id,
                        "endpoint_id": endpoint_id,
                        "hash": started.hash.to_string(),
                        "size": started.size,
                        "index": started.index,
                    }),
                ),
                RequestUpdate::Progress(progress) => core.emit_transfer(
                    transfer_id,
                    "send",
                    "transfer",
                    "progress",
                    json!({
                        "connection_id": connection_id,
                        "request_id": request_id,
                        "endpoint_id": endpoint_id,
                        "end_offset": progress.end_offset,
                    }),
                ),
                RequestUpdate::Completed(_) => {
                    core.emit_transfer(
                        transfer_id,
                        "send",
                        "transfer",
                        "completed",
                        json!({
                            "connection_id": connection_id,
                            "request_id": request_id,
                            "endpoint_id": endpoint_id,
                        }),
                    );
                }
                RequestUpdate::Aborted(_) => {
                    core.emit_transfer(
                        transfer_id,
                        "send",
                        "transfer",
                        "aborted",
                        json!({
                            "connection_id": connection_id,
                            "request_id": request_id,
                            "endpoint_id": endpoint_id,
                        }),
                    );
                }
            })
            .await;
            if outcome == RequestStreamOutcome::Aborted {
                core.emit_transfer(
                    transfer_id,
                    "send",
                    "transfer",
                    "aborted",
                    json!({
                        "connection_id": connection_id,
                        "request_id": request_id,
                        "endpoint_id": endpoint_id,
                    }),
                );
            }
        });
    }
}
