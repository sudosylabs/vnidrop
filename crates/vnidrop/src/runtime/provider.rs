use std::{collections::HashSet, sync::Arc};

use iroh_blobs::{
    provider::events::{ProviderMessage, RequestUpdate},
    Hash,
};
use serde_json::json;
use tokio::sync::mpsc;

use super::CoreInner;
use crate::access_policy::AccessLease;

pub(super) struct ProviderAccess {
    transfer_id: u64,
    leases: Vec<AccessLease>,
}

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
                    .authorize_hashes(&[message.inner.request.hash], message.inner.connection_id)
                    .await
                {
                    Ok(transfer_ids) => {
                        self.track_request_updates(
                            transfer_ids,
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
                if let Ok(transfer_ids) = self
                    .authorize_hashes(&[message.inner.request.hash], message.inner.connection_id)
                    .await
                {
                    self.track_request_updates(
                        transfer_ids,
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
                    Ok(transfer_ids) => {
                        self.track_request_updates(
                            transfer_ids,
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
                if let Ok(transfer_ids) = self
                    .authorize_hashes(&message.inner.request.hashes, message.inner.connection_id)
                    .await
                {
                    self.track_request_updates(
                        transfer_ids,
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
                let key = (message.inner.connection_id, message.inner.request_id);
                let decision = if self.provider_request_is_active(key).await {
                    Ok(())
                } else {
                    Err(iroh_blobs::provider::events::AbortReason::Permission)
                };
                let _ = message.tx.send(decision).await;
            }
            ProviderMessage::PushRequestReceived(message) => {
                let _ = message
                    .tx
                    .send(Err(iroh_blobs::provider::events::AbortReason::Permission))
                    .await;
                self.emit_denied_request(
                    message.inner.connection_id,
                    message.inner.request_id,
                    "push-not-supported",
                );
            }
            ProviderMessage::PushRequestReceivedNotify(message) => {
                self.emit_denied_request(
                    message.inner.connection_id,
                    message.inner.request_id,
                    "push-not-supported",
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

    #[cfg(test)]
    pub(super) async fn register_provider_request_for_test(&self, transfer_id: u64) {
        let lease = self
            .access_policy
            .authorize(transfer_id, Some("test-peer"))
            .await
            .unwrap();
        self.provider_requests.lock().await.insert(
            (1, 1),
            ProviderAccess {
                transfer_id,
                leases: vec![lease],
            },
        );
    }

    pub(super) async fn provider_request_is_active(&self, key: (u64, u64)) -> bool {
        self.provider_requests
            .lock()
            .await
            .get(&key)
            .is_some_and(|request| request.leases.iter().all(AccessLease::is_active))
    }

    /// Default-deny: hash must belong to an active share the peer may read.
    async fn authorize_hash(
        &self,
        hash: Hash,
        connection_id: u64,
    ) -> Result<(u64, AccessLease), &'static str> {
        let transfer_ids = self.transfer_ids_for_hash(hash).await;
        if transfer_ids.is_empty() {
            return Err("unknown-hash");
        }
        let endpoint = self
            .connection_endpoints
            .lock()
            .await
            .get(&connection_id)
            .cloned()
            .ok_or("missing-endpoint-id")?;
        let mut last_reason = "approval-required";
        for id in transfer_ids {
            match self.access_policy.authorize(id, Some(&endpoint)).await {
                Ok(lease) => return Ok((id, lease)),
                Err(reason) => last_reason = reason,
            }
        }
        Err(last_reason)
    }

    async fn authorize_hashes(
        &self,
        hashes: &[Hash],
        connection_id: u64,
    ) -> Result<ProviderAccess, &'static str> {
        let mut transfer_id = None;
        let mut seen = HashSet::new();
        let mut leases = Vec::new();
        for hash in hashes {
            let (id, lease) = self.authorize_hash(*hash, connection_id).await?;
            transfer_id.get_or_insert(id);
            if seen.insert(id) {
                leases.push(lease);
            }
        }
        Ok(ProviderAccess {
            transfer_id: transfer_id.ok_or("empty-request")?,
            leases,
        })
    }

    pub(super) async fn transfer_ids_for_hash(&self, hash: Hash) -> Vec<u64> {
        self.hash_to_transfer
            .lock()
            .await
            .get(&hash.to_string())
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    async fn track_request_updates(
        self: &Arc<Self>,
        access: ProviderAccess,
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
        let transfer_id = access.transfer_id;
        let request_key = (connection_id, request_id);
        let was_idle = {
            let mut requests = self.provider_requests.lock().await;
            let was_idle = requests.is_empty();
            requests.insert(request_key, access);
            was_idle
        };
        if was_idle {
            self.emit_endpoint("runtime_obligation", "changed", json!({}));
        }
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
            let became_idle = {
                let mut requests = core.provider_requests.lock().await;
                requests.remove(&request_key);
                requests.is_empty()
            };
            if became_idle {
                core.emit_endpoint("runtime_obligation", "changed", json!({}));
            }
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
