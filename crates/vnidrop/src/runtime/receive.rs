use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_lite::StreamExt as _;
use iroh_blobs::{
    api::remote::GetProgressItem, format::collection::Collection,
    get::request::get_hash_seq_and_sizes, ticket::BlobTicket, Hash,
};
use serde_json::json;
use tokio::sync::oneshot;

mod publish;

use super::{
    export::BufferedExport, filter_peer_addr_for_relay_mode, ActiveTransfer, CoreInner,
    ReceiveAttempt,
};
use crate::{
    access_policy::mode_to_storage,
    api::{
        ReceiveOutputSink, ReceiveOutputSinkV2, ReceivedLocatorKind, TargetedOutputVerifier,
        TransferAccessMode, TransferMetadata,
    },
    error::VnidropError,
    filesystem::{
        output_file_identity, safe_output_path, validated_relative_string, verify_published_output,
        wait_for_writer, write_stream_to_blocking_writer, AtomicOutputFile,
    },
    handshake::{DeliveryReceipt, HandshakeResponse, HandshakeService},
    invitation::{PendingDeliveryReceiptInsert, ReceivedArtifactInsert, TransferUpsert},
    ticket::{
        encode_persisted_sender_address, parse_transfer_ticket_with_limits, ParsedTransferTicket,
    },
    transfer_state::{TransferDirection, TransferStatus},
};

pub(super) enum ReceiveTarget {
    Directory(PathBuf),
    OutputSink(Arc<dyn ReceiveOutputSink>),
    OutputSinkV2(
        Arc<dyn ReceiveOutputSinkV2>,
        Option<Arc<dyn TargetedOutputVerifier>>,
    ),
}

pub(crate) fn check_collection_metadata_size(sizes: &[u64], max_path_bytes: u64) -> Result<u64> {
    let (&metadata_bytes, files) = sizes
        .split_first()
        .context("remote collection is missing metadata")?;
    // CollectionV0 stores a header and a postcard list of paths, with a
    // length prefix for the list and each path.
    const MAX_LENGTH_PREFIX_BYTES: u64 = 10;
    let limit = (Collection::HEADER.len() as u64 + MAX_LENGTH_PREFIX_BYTES).saturating_add(
        (files.len() as u64).saturating_mul(max_path_bytes.saturating_add(MAX_LENGTH_PREFIX_BYTES)),
    );
    anyhow::ensure!(
        metadata_bytes <= limit,
        "remote collection metadata size {metadata_bytes} exceeds limit {limit}"
    );
    Ok(metadata_bytes)
}

impl CoreInner {
    pub(super) async fn receive(
        self: &Arc<Self>,
        ticket: String,
        output_dir: PathBuf,
        receiver_name: Option<String>,
    ) -> Result<()> {
        self.receive_to_target(ticket, ReceiveTarget::Directory(output_dir), receiver_name)
            .await
    }

    pub(super) async fn receive_with_output_sink(
        self: &Arc<Self>,
        ticket: String,
        output_sink: Arc<dyn ReceiveOutputSink>,
        receiver_name: Option<String>,
    ) -> Result<()> {
        self.receive_to_target(
            ticket,
            ReceiveTarget::OutputSink(output_sink),
            receiver_name,
        )
        .await
    }

    pub(super) async fn receive_with_output_sink_v2(
        self: &Arc<Self>,
        ticket: String,
        output_sink: Arc<dyn ReceiveOutputSinkV2>,
        receiver_name: Option<String>,
    ) -> Result<()> {
        self.receive_to_target(
            ticket,
            ReceiveTarget::OutputSinkV2(output_sink, None),
            receiver_name,
        )
        .await
    }

    pub(super) async fn receive_to_target(
        self: &Arc<Self>,
        ticket: String,
        target: ReceiveTarget,
        receiver_name: Option<String>,
    ) -> Result<()> {
        let mut parsed = match parse_transfer_ticket_with_limits(&ticket, &self.limits)
            .context("failed to parse transfer ticket")
        {
            Ok(parsed) => parsed,
            Err(error) => {
                self.emit_endpoint(
                    "error",
                    "invalid-ticket",
                    json!({ "reason": error.to_string() }),
                );
                return Err(error);
            }
        };
        let sender_addr = filter_peer_addr_for_relay_mode(
            parsed.blob_ticket.addr(),
            self.relay_mode,
            &self.custom_relay_urls,
        )
        .map_err(VnidropError::network)?;
        parsed.blob_ticket = BlobTicket::new(
            sender_addr,
            parsed.blob_ticket.hash(),
            parsed.blob_ticket.format(),
        );
        let transfer_id = parsed.metadata.transfer_id;
        if !self
            .active_receive_attempts
            .lock()
            .expect("active_receive_attempts")
            .insert(transfer_id)
        {
            return Err(VnidropError::InvalidTransition {
                reason: "invitation receive is already active".to_string(),
            }
            .into());
        }
        let receive_attempt = Arc::new(ReceiveAttempt {
            id: transfer_id,
            active: self.active_receive_attempts.clone(),
        });
        self.persist_receive_start(transfer_id, &parsed)
            .await
            .map_err(VnidropError::repository)?;
        let persisted_sender_address =
            encode_persisted_sender_address(parsed.blob_ticket.addr())
                .context("failed to encode sender address for delivery receipt")?;
        let delivery_receipt = Arc::new(Mutex::new(None));
        // Cancellation is cooperative: it stops our receive future and marks
        // local state while lower-level Iroh work unwinds naturally.
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        self.active_transfers
            .lock()
            .expect("active_transfers")
            .insert(
                transfer_id,
                ActiveTransfer {
                    direction: TransferDirection::Receive,
                    cancel: Some(shutdown_tx),
                    receive_attempt: Some(receive_attempt.clone()),
                },
            );

        let metadata_json =
            serde_json::to_value(&parsed.metadata).unwrap_or(serde_json::Value::Null);
        self.emit_transfer(
            transfer_id,
            "receive",
            "lifecycle",
            "started",
            json!({
                "metadata": metadata_json,
                "receiver_name": receiver_name.as_deref(),
            }),
        );

        let receive = async {
            let _permit = self
                .transfer_slots
                .acquire()
                .await
                .context("transfer limiter is closed")
                .map_err(VnidropError::internal)?;
            self.receive_inner(
                transfer_id,
                parsed,
                target,
                receiver_name,
                persisted_sender_address.clone(),
                delivery_receipt.clone(),
            )
            .await
            .map_err(VnidropError::transfer)
        };
        let (result, cancelled) = tokio::select! {
            biased;
            _ = &mut shutdown_rx => (Err(VnidropError::cancelled("transfer cancelled")), true),
            result = receive => (result, false),
        };

        self.active_transfers
            .lock()
            .expect("active_transfers")
            .remove(&transfer_id);
        if let Err(error) = &result {
            if cancelled {
                if let Err(error) = self
                    .repository
                    .transition_transfer_status(
                        transfer_id,
                        TransferStatus::Receiving,
                        TransferStatus::Cancelled,
                    )
                    .await
                {
                    tracing::warn!(%error, transfer_id, "failed to persist receive cancellation");
                }
                self.emit_transfer(transfer_id, "receive", "lifecycle", "cancelled", json!({}));
            } else {
                self.emit_transfer(
                    transfer_id,
                    "receive",
                    "error",
                    "failed",
                    json!({ "code": error.code(), "reason": error.reason() }),
                );
                let _ = self
                    .repository
                    .transition_transfer_status(
                        transfer_id,
                        TransferStatus::Receiving,
                        TransferStatus::Failed,
                    )
                    .await;
            }
            let receipt = delivery_receipt.lock().expect("delivery_receipt").take();
            if let Some(receipt) = receipt {
                let reason = if cancelled { "cancelled" } else { error.code() };
                match self
                    .repository
                    .queue_failed_delivery_receipt(PendingDeliveryReceiptInsert {
                        local_transfer_id: transfer_id,
                        sender_blob_ticket: &persisted_sender_address,
                        request_id: &receipt.request_id,
                        sender_transfer_id: receipt.transfer_id,
                        token: &receipt.token,
                        failure_reason: Some(reason),
                    })
                    .await
                {
                    Ok(()) => self.delivery_receipt_notify.notify_one(),
                    Err(queue_error) => {
                        tracing::warn!(%queue_error, "failed to queue delivery failure receipt");
                        self.emit_transfer(
                            transfer_id,
                            "receive",
                            "delivery",
                            "receipt-failed",
                            json!({ "reason": queue_error.to_string() }),
                        );
                    }
                }
            }
        }
        result.map_err(anyhow::Error::new)
    }

    pub(super) async fn receive_inner(
        self: &Arc<Self>,
        transfer_id: u64,
        parsed: ParsedTransferTicket,
        target: ReceiveTarget,
        receiver_name: Option<String>,
        persisted_sender_address: String,
        pending_delivery_receipt: Arc<Mutex<Option<DeliveryReceipt>>>,
    ) -> Result<()> {
        if let ReceiveTarget::Directory(output_dir) = &target {
            tokio::fs::create_dir_all(output_dir)
                .await
                .map_err(VnidropError::filesystem)?;
        }
        let sender_addr = parsed.blob_ticket.addr().clone();

        self.emit_transfer(transfer_id, "receive", "network", "connecting", json!({}));
        // Every VniDrop ticket carries metadata and must complete the handshake.
        let (delivery_receipt, authenticated_sender_name) = self
            .request_transfer_approval(
                transfer_id,
                sender_addr.clone(),
                &parsed.metadata,
                receiver_name.as_deref(),
            )
            .await?;
        *pending_delivery_receipt
            .lock()
            .expect("pending_delivery_receipt") = Some(delivery_receipt.clone());
        let connection = self
            .endpoint
            .connect(sender_addr.clone(), iroh_blobs::ALPN)
            .await
            .map_err(VnidropError::network)?;
        self.emit_transfer(transfer_id, "receive", "network", "connected", json!({}));

        let hash_and_format = parsed.blob_ticket.hash_and_format();
        let (_hash_seq, sizes) =
            get_hash_seq_and_sizes(&connection, &hash_and_format.hash, 1024 * 1024 * 32, None)
                .await
                .context("failed to get file sizes")
                .map_err(VnidropError::network)?;
        let total_size = sizes
            .iter()
            .try_fold(0u64, |total, size| total.checked_add(*size))
            .context("remote collection size overflow")?;
        let total_files = sizes.len().saturating_sub(1) as u64;
        if total_files > self.limits.max_collection_files {
            anyhow::bail!(
                "remote collection has {total_files} files, limit is {}",
                self.limits.max_collection_files
            );
        }
        if total_size > self.limits.max_total_bytes {
            anyhow::bail!(
                "remote collection size {total_size} exceeds limit {}",
                self.limits.max_total_bytes
            );
        }
        check_collection_metadata_size(&sizes, self.limits.max_path_bytes)?;
        self.emit_transfer(
            transfer_id,
            "receive",
            "download",
            "found-collection",
            json!({ "total_files": total_files, "total_size": total_size }),
        );

        // Protect both partial download state and the completed collection until
        // every output has been published and recorded.
        let download_tag = self.store.tags().temp_tag(hash_and_format).await?;
        let get = self.store.remote().fetch(connection, hash_and_format);
        let mut stream = get.stream();
        loop {
            let Some(item) = stream.next().await else {
                anyhow::bail!("download ended without completion");
            };
            match item {
                GetProgressItem::Progress(downloaded) => {
                    self.emit_transfer(
                        transfer_id,
                        "receive",
                        "download",
                        "progress",
                        json!({ "downloaded": downloaded, "total_size": total_size }),
                    );
                }
                GetProgressItem::Done(_) => break,
                GetProgressItem::Error(error) => {
                    return Err(
                        VnidropError::network(anyhow::anyhow!("download failed: {error}")).into(),
                    );
                }
            }
        }

        let collection = Collection::load(hash_and_format.hash, self.store.as_ref()).await?;
        self.export_collection(transfer_id, &sizes[1..], target, collection)
            .await?;
        self.repository
            .complete_receive_with_pending_receipt(PendingDeliveryReceiptInsert {
                local_transfer_id: transfer_id,
                sender_blob_ticket: &persisted_sender_address,
                request_id: &delivery_receipt.request_id,
                sender_transfer_id: delivery_receipt.transfer_id,
                token: &delivery_receipt.token,
                failure_reason: None,
            })
            .await
            .map_err(VnidropError::repository)?;
        pending_delivery_receipt
            .lock()
            .expect("pending_delivery_receipt")
            .take();
        self.delivery_receipt_notify.notify_one();
        let peer_endpoint_id = sender_addr.id.to_string();
        if let Err(error) = self
            .pairing_eligibility
            .activate_after_completed_transfer(
                &peer_endpoint_id,
                authenticated_sender_name.as_deref(),
                &delivery_receipt.request_id,
                &delivery_receipt.token,
            )
            .await
        {
            tracing::warn!(%error, "failed to activate pairing eligibility after receive");
        }
        drop(download_tag);
        self.emit_transfer(transfer_id, "receive", "lifecycle", "done", json!({}));
        Ok(())
    }

    pub(super) async fn persist_receive_start(
        &self,
        transfer_id: u64,
        parsed: &ParsedTransferTicket,
    ) -> Result<()> {
        let peer_id = parsed.blob_ticket.addr().id.to_string();
        self.repository
            .start_receive(TransferUpsert {
                transfer_id,
                peer_id: Some(&peer_id),
                direction: TransferDirection::Receive,
                status: TransferStatus::Receiving,
                transfer_name: Some(parsed.metadata.transfer_name.as_str()),
                sender_name: parsed.metadata.sender_name.as_deref(),
                content_hash: Some(parsed.metadata.content_hash.as_str()),
                ticket: None,
                file_count: parsed.metadata.file_count,
                total_size: parsed.metadata.total_size,
                access_mode: mode_to_storage(&TransferAccessMode::ApprovalRequired),
            })
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn request_transfer_approval(
        &self,
        local_transfer_id: u64,
        addr: iroh::EndpointAddr,
        metadata: &TransferMetadata,
        receiver_name: Option<&str>,
    ) -> Result<(DeliveryReceipt, Option<String>)> {
        self.emit_transfer(
            local_transfer_id,
            "receive",
            "handshake",
            "approval-requesting",
            json!({
                "sender_transfer_id": metadata.transfer_id,
                "metadata": metadata,
            }),
        );

        let client = HandshakeService::client(self.endpoint.clone(), addr);
        match client
            .request_transfer(metadata, receiver_name)
            .await
            .map_err(|error| {
                VnidropError::network(anyhow::anyhow!("handshake request failed: {error}"))
            })? {
            HandshakeResponse::Approved {
                request_id,
                token,
                expires_at,
                sender_name,
            } => {
                self.emit_transfer(
                    local_transfer_id,
                    "receive",
                    "handshake",
                    "approval-granted",
                    json!({
                        "sender_transfer_id": metadata.transfer_id,
                        "expires_at": expires_at,
                    }),
                );
                Ok((
                    DeliveryReceipt {
                        request_id,
                        transfer_id: metadata.transfer_id,
                        token,
                    },
                    sender_name,
                ))
            }
            HandshakeResponse::Denied { reason } => Err(VnidropError::permission(anyhow::anyhow!(
                "transfer request was denied by sender: {reason}"
            ))
            .into()),
        }
    }
}
