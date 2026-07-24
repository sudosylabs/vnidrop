use std::{sync::Arc, time::Duration};

use serde_json::json;

use super::{filter_peer_addr_for_relay_mode, CoreInner};
use crate::{
    handshake::{
        DeliveryFailureReceipt, DeliveryReceipt, DeliveryReceiptResponse, HandshakeService,
    },
    repository::PendingDeliveryReceipt,
    ticket::parse_persisted_sender_address,
};

const DELIVERY_RECEIPT_TIMEOUT: Duration = Duration::from_secs(5);
const DELIVERY_RECEIPT_RETRY_INTERVAL: Duration = Duration::from_secs(5);
const DELIVERY_RECEIPT_MAX_RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);

impl CoreInner {
    pub(super) async fn spawn_delivery_receipt_task(self: &Arc<Self>) {
        let core = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            let mut retry_interval = DELIVERY_RECEIPT_RETRY_INTERVAL;
            loop {
                let Some(core) = core.upgrade() else {
                    break;
                };
                let has_pending = core.deliver_pending_receipts().await;
                if has_pending {
                    let notified = tokio::select! {
                        () = core.delivery_receipt_notify.notified() => true,
                        () = tokio::time::sleep(retry_interval) => false,
                    };
                    if notified {
                        retry_interval = DELIVERY_RECEIPT_RETRY_INTERVAL;
                    } else {
                        retry_interval = retry_interval
                            .saturating_mul(2)
                            .min(DELIVERY_RECEIPT_MAX_RETRY_INTERVAL);
                    }
                } else {
                    core.delivery_receipt_notify.notified().await;
                    retry_interval = DELIVERY_RECEIPT_RETRY_INTERVAL;
                }
            }
        });
        *self.delivery_receipt_task.lock().await = Some(task);
    }

    async fn deliver_pending_receipts(&self) -> bool {
        let receipts = match self.repository.list_pending_delivery_receipts().await {
            Ok(receipts) => receipts,
            Err(error) => {
                tracing::warn!(%error, "failed to load pending delivery receipts");
                return true;
            }
        };
        let has_pending = !receipts.is_empty();
        for receipt in receipts {
            self.deliver_pending_receipt(receipt).await;
        }
        has_pending
    }

    async fn deliver_pending_receipt(&self, pending: PendingDeliveryReceipt) {
        let sender_addr = match parse_persisted_sender_address(&pending.sender_blob_ticket) {
            Ok(addr) => addr,
            Err(error) => {
                tracing::warn!(%error, request_id = %pending.request_id, "discarded invalid pending delivery receipt");
                let _ = self
                    .repository
                    .delete_pending_delivery_receipt(&pending.request_id)
                    .await;
                self.emit_transfer(
                    pending.local_transfer_id,
                    "receive",
                    "delivery",
                    "receipt-rejected",
                    json!({ "reason": "invalid-sender-ticket" }),
                );
                return;
            }
        };
        let sender_addr = match filter_peer_addr_for_relay_mode(
            &sender_addr,
            self.relay_mode,
            &self.custom_relay_urls,
        ) {
            Ok(addr) => addr,
            Err(error) => {
                self.emit_transfer(
                    pending.local_transfer_id,
                    "receive",
                    "delivery",
                    "receipt-failed",
                    json!({ "reason": error.to_string() }),
                );
                return;
            }
        };
        let client = HandshakeService::client(self.endpoint.clone(), sender_addr);
        let request_id = pending.request_id.clone();
        let response = tokio::time::timeout(DELIVERY_RECEIPT_TIMEOUT, async {
            if let Some(reason) = pending.failure_reason {
                client
                    .report_delivery_failure(DeliveryFailureReceipt {
                        request_id,
                        transfer_id: pending.sender_transfer_id,
                        token: pending.token,
                        reason,
                    })
                    .await
            } else {
                client
                    .report_delivery(DeliveryReceipt {
                        request_id,
                        transfer_id: pending.sender_transfer_id,
                        token: pending.token,
                    })
                    .await
            }
        })
        .await;
        match response {
            Ok(Ok(DeliveryReceiptResponse::Recorded)) => {
                if let Err(error) = self
                    .repository
                    .delete_pending_delivery_receipt(&pending.request_id)
                    .await
                {
                    tracing::warn!(%error, request_id = %pending.request_id, "failed to clear recorded delivery receipt");
                    return;
                }
                self.emit_transfer(
                    pending.local_transfer_id,
                    "receive",
                    "delivery",
                    "receipt-recorded",
                    json!({ "sender_transfer_id": pending.sender_transfer_id }),
                );
            }
            Ok(Ok(DeliveryReceiptResponse::Rejected { reason })) => {
                let _ = self
                    .repository
                    .delete_pending_delivery_receipt(&pending.request_id)
                    .await;
                self.emit_transfer(
                    pending.local_transfer_id,
                    "receive",
                    "delivery",
                    "receipt-rejected",
                    json!({ "reason": reason }),
                );
            }
            Ok(Err(error)) => self.emit_transfer(
                pending.local_transfer_id,
                "receive",
                "delivery",
                "receipt-failed",
                json!({ "reason": error.to_string() }),
            ),
            Err(_) => self.emit_transfer(
                pending.local_transfer_id,
                "receive",
                "delivery",
                "receipt-failed",
                json!({ "reason": "delivery receipt timed out" }),
            ),
        }
    }
}
