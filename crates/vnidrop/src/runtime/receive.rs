use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_lite::StreamExt as _;
use iroh_blobs::{
    api::proto::ExportRangesItem, api::remote::GetProgressItem, format::collection::Collection,
    get::request::get_hash_seq_and_sizes, ticket::BlobTicket, Hash,
};
use serde_json::json;
use tokio::sync::oneshot;

use super::{filter_peer_addr_for_relay_mode, ActiveTransfer, CoreInner};
use crate::{
    access_policy::mode_to_storage,
    api::{
        ReceiveOutputSink, ReceiveOutputSinkV2, ReceivedLocatorKind, TransferAccessMode,
        TransferMetadata,
    },
    error::VnidropError,
    filesystem::{
        validated_relative_string, wait_for_writer, write_stream_to_blocking_writer,
        AtomicOutputFile,
    },
    handshake::{DeliveryReceipt, HandshakeResponse, HandshakeService},
    repository::{PendingDeliveryReceiptInsert, ReceivedArtifactInsert, TransferUpsert},
    ticket::{
        encode_persisted_sender_address, parse_transfer_ticket_with_limits, ParsedTransferTicket,
    },
    transfer_state::{TransferDirection, TransferStatus},
};

pub(super) enum ReceiveTarget {
    Directory(PathBuf),
    OutputSink(Arc<dyn ReceiveOutputSink>),
    OutputSinkV2(Arc<dyn ReceiveOutputSinkV2>),
}

#[derive(Clone, Copy)]
struct ReceivedTransfer<'a> {
    protocol_id: u64,
    local_id: &'a str,
}

pub(super) struct OutputSinkFile<'a> {
    sink: &'a dyn ReceiveOutputSink,
    relative_path: String,
    terminal: bool,
}

impl<'a> OutputSinkFile<'a> {
    fn start(sink: &'a dyn ReceiveOutputSink, relative_path: String) -> Result<Self> {
        sink.start_file(relative_path.clone())
            .map_err(anyhow::Error::new)?;
        Ok(Self {
            sink,
            relative_path,
            terminal: false,
        })
    }

    fn write(&self, bytes: Vec<u8>) -> Result<()> {
        self.sink
            .write_chunk(self.relative_path.clone(), bytes)
            .map_err(anyhow::Error::new)
    }

    fn finish(mut self) -> Result<()> {
        // finish_file is terminal even when the foreign implementation reports
        // an error; implementations must release their open resource before
        // returning so Rust never invokes two terminal callbacks.
        self.terminal = true;
        self.sink
            .finish_file(self.relative_path.clone())
            .map_err(anyhow::Error::new)
    }
}

impl Drop for OutputSinkFile<'_> {
    fn drop(&mut self) {
        if !self.terminal {
            self.terminal = true;
            if let Err(error) = self.sink.abort_file(
                self.relative_path.clone(),
                "transfer interrupted before file completion".to_string(),
            ) {
                tracing::warn!(
                    %error,
                    relative_path = %self.relative_path,
                    "failed to abort receive output sink file"
                );
            }
        }
    }
}

pub(super) struct OutputSinkFileV2<'a> {
    sink: &'a dyn ReceiveOutputSinkV2,
    relative_path: String,
    terminal: bool,
}

impl<'a> OutputSinkFileV2<'a> {
    fn start(sink: &'a dyn ReceiveOutputSinkV2, relative_path: String) -> Result<Self> {
        sink.start_file(relative_path.clone())
            .map_err(anyhow::Error::new)?;
        Ok(Self {
            sink,
            relative_path,
            terminal: false,
        })
    }

    fn write(&self, bytes: Vec<u8>) -> Result<()> {
        self.sink
            .write_chunk(self.relative_path.clone(), bytes)
            .map_err(anyhow::Error::new)
    }

    fn finish(mut self) -> Result<crate::api::PublishedOutput> {
        self.terminal = true;
        self.sink
            .finish_file(self.relative_path.clone())
            .map_err(anyhow::Error::new)
    }
}

impl Drop for OutputSinkFileV2<'_> {
    fn drop(&mut self) {
        if !self.terminal {
            self.terminal = true;
            if let Err(error) = self.sink.abort_file(
                self.relative_path.clone(),
                "transfer interrupted before file completion".to_string(),
            ) {
                tracing::warn!(%error, relative_path = %self.relative_path, "failed to abort receive output sink file");
            }
        }
    }
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
            ReceiveTarget::OutputSinkV2(output_sink),
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
        let _permit = self
            .transfer_slots
            .acquire()
            .await
            .context("transfer limiter is closed")
            .map_err(VnidropError::internal)?;
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
        self.persist_receive_start(transfer_id, &parsed, receiver_name.as_deref())
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
                    cancel: shutdown_tx,
                },
            );

        let (result, cancelled) = tokio::select! {
            result = self.receive_inner(
                transfer_id,
                parsed,
                target,
                receiver_name,
                persisted_sender_address.clone(),
                delivery_receipt.clone(),
            ) => {
                (result.map_err(VnidropError::transfer), false)
            },
            _ = &mut shutdown_rx => (Err(VnidropError::cancelled("transfer cancelled")), true),
        };

        self.active_transfers
            .lock()
            .expect("active_transfers")
            .remove(&transfer_id);
        if let Err(error) = &result {
            if cancelled {
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
        let delivery_receipt = self
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
        self.export_collection(transfer_id, total_files, target, collection)
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
        drop(download_tag);
        self.emit_transfer(transfer_id, "receive", "lifecycle", "done", json!({}));
        self.delivery_receipt_notify.notify_one();
        Ok(())
    }

    pub(super) async fn persist_receive_start(
        &self,
        transfer_id: u64,
        parsed: &ParsedTransferTicket,
        receiver_name: Option<&str>,
    ) -> Result<()> {
        let peer_id = parsed.blob_ticket.addr().id.to_string();
        self.repository
            .start_receive(TransferUpsert {
                transfer_id,
                peer_id: Some(&peer_id),
                direction: TransferDirection::Receive,
                status: TransferStatus::Receiving,
                transfer_name: Some(parsed.metadata.transfer_name.as_str()),
                content_hash: Some(parsed.metadata.content_hash.as_str()),
                ticket: None,
                file_count: parsed.metadata.file_count,
                total_size: parsed.metadata.total_size,
                access_mode: mode_to_storage(&TransferAccessMode::ApprovalRequired),
            })
            .await
            .map_err(VnidropError::repository)?;
        let metadata_json =
            serde_json::to_value(&parsed.metadata).unwrap_or(serde_json::Value::Null);
        self.emit_transfer(
            transfer_id,
            "receive",
            "lifecycle",
            "started",
            json!({
                "metadata": metadata_json,
                "receiver_name": receiver_name,
            }),
        );
        Ok(())
    }

    pub(super) async fn request_transfer_approval(
        &self,
        local_transfer_id: u64,
        addr: iroh::EndpointAddr,
        metadata: &TransferMetadata,
        receiver_name: Option<&str>,
    ) -> Result<DeliveryReceipt> {
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
                Ok(DeliveryReceipt {
                    request_id,
                    transfer_id: metadata.transfer_id,
                    token,
                })
            }
            HandshakeResponse::Denied { reason } => Err(VnidropError::permission(anyhow::anyhow!(
                "transfer request was denied by sender: {reason}"
            ))
            .into()),
        }
    }

    pub(super) async fn export_collection(
        &self,
        transfer_id: u64,
        total_files: u64,
        target: ReceiveTarget,
        collection: Collection,
    ) -> Result<()> {
        let transfer_local_id = self
            .repository
            .transfer_local_id(transfer_id)
            .await
            .map_err(VnidropError::repository)?;
        let received_transfer = ReceivedTransfer {
            protocol_id: transfer_id,
            local_id: &transfer_local_id,
        };
        for (i, (name, hash)) in collection.iter().enumerate() {
            match &target {
                ReceiveTarget::Directory(output_dir) => {
                    self.export_blob_to_directory(
                        received_transfer,
                        total_files,
                        i as u64,
                        output_dir,
                        name.as_ref(),
                        *hash,
                    )
                    .await?;
                }
                ReceiveTarget::OutputSink(output_sink) => {
                    self.export_blob_to_sink(
                        transfer_id,
                        total_files,
                        i as u64,
                        output_sink.as_ref(),
                        name.as_ref(),
                        *hash,
                    )
                    .await?;
                }
                ReceiveTarget::OutputSinkV2(output_sink) => {
                    self.export_blob_to_sink_v2(
                        received_transfer,
                        total_files,
                        i as u64,
                        output_sink.as_ref(),
                        name.as_ref(),
                        *hash,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn export_blob_to_directory(
        &self,
        transfer: ReceivedTransfer<'_>,
        total_files: u64,
        current_file_index: u64,
        output_dir: &Path,
        relative_path: &str,
        hash: Hash,
    ) -> Result<()> {
        if relative_path.len() as u64 > self.limits.max_path_bytes {
            anyhow::bail!(
                "output path exceeds {} bytes: {relative_path}",
                self.limits.max_path_bytes
            );
        }
        let (pending_file, writer) = AtomicOutputFile::create(output_dir, relative_path)
            .map_err(VnidropError::filesystem)?;
        let (tx, rx) = async_channel::bounded::<io::Result<Option<Bytes>>>(2);
        let writer_task = std::thread::spawn(move || write_stream_to_blocking_writer(writer, rx));
        let mut stream = self.store.export_ranges(hash, 0..u64::MAX).stream();
        let mut file_size = 0;
        let mut exported = 0;

        while let Some(item) = stream.next().await {
            match item {
                ExportRangesItem::Size(size) => file_size = size,
                ExportRangesItem::Data(leaf) => {
                    if leaf.offset != exported {
                        anyhow::bail!(
                            "export stream for {relative_path} yielded out-of-order data"
                        );
                    }
                    exported += leaf.data.len() as u64;
                    tx.send(Ok(Some(leaf.data)))
                        .await
                        .map_err(|_| anyhow::anyhow!("export writer closed for {relative_path}"))?;
                    self.emit_transfer(
                        transfer.protocol_id,
                        "receive",
                        "export",
                        "progress",
                        json!({
                            "total_files": total_files,
                            "current_file_index": current_file_index,
                            "file_name": relative_path,
                            "file_size": file_size,
                            "exported": exported,
                        }),
                    );
                }
                ExportRangesItem::Error(error) => {
                    anyhow::bail!("export failed for {relative_path}: {error}");
                }
            }
        }

        tx.send(Ok(None))
            .await
            .map_err(|_| anyhow::anyhow!("export writer closed for {relative_path}"))?;
        let writer = wait_for_writer(writer_task)
            .await
            .map_err(VnidropError::internal)?
            .map_err(VnidropError::filesystem)?;
        tokio::task::spawn_blocking(move || writer.sync_all())
            .await
            .map_err(VnidropError::internal)?
            .map_err(VnidropError::filesystem)?;
        let locator = pending_file.target().to_string_lossy().to_string();
        pending_file.commit().map_err(VnidropError::filesystem)?;
        self.repository
            .record_received_artifact(ReceivedArtifactInsert {
                transfer_local_id: transfer.local_id,
                protocol_transfer_id: transfer.protocol_id,
                relative_path,
                locator_kind: ReceivedLocatorKind::FilesystemPath,
                locator: &locator,
                logical_size: exported,
            })
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }

    pub(super) async fn export_blob_to_sink(
        &self,
        transfer_id: u64,
        total_files: u64,
        current_file_index: u64,
        output_sink: &dyn ReceiveOutputSink,
        relative_path: &str,
        hash: Hash,
    ) -> Result<()> {
        if relative_path.len() as u64 > self.limits.max_path_bytes {
            anyhow::bail!(
                "output path exceeds {} bytes: {relative_path}",
                self.limits.max_path_bytes
            );
        }
        let relative_path = validated_relative_string(relative_path)?;
        let output_file = OutputSinkFile::start(output_sink, relative_path.clone())?;
        let mut stream = self.store.export_ranges(hash, 0..u64::MAX).stream();
        let mut file_size = 0;
        let mut exported = 0;

        while let Some(item) = stream.next().await {
            match item {
                ExportRangesItem::Size(size) => file_size = size,
                ExportRangesItem::Data(leaf) => {
                    if leaf.offset != exported {
                        anyhow::bail!(
                            "export stream for {relative_path} yielded out-of-order data"
                        );
                    }
                    exported += leaf.data.len() as u64;
                    output_file.write(leaf.data.to_vec())?;
                    self.emit_transfer(
                        transfer_id,
                        "receive",
                        "export",
                        "progress",
                        json!({
                            "total_files": total_files,
                            "current_file_index": current_file_index,
                            "file_name": relative_path,
                            "file_size": file_size,
                            "exported": exported,
                        }),
                    );
                    // Yield so outer `select!` cancel can land between chunks.
                    // Sink writes are synchronous (foreign FFI); without this,
                    // a single poll can drain the stream and miss cancellation.
                    tokio::task::yield_now().await;
                }
                ExportRangesItem::Error(error) => {
                    anyhow::bail!("export failed for {relative_path}: {error}");
                }
            }
        }

        output_file.finish()?;
        Ok(())
    }

    async fn export_blob_to_sink_v2(
        &self,
        transfer: ReceivedTransfer<'_>,
        total_files: u64,
        current_file_index: u64,
        output_sink: &dyn ReceiveOutputSinkV2,
        relative_path: &str,
        hash: Hash,
    ) -> Result<()> {
        if relative_path.len() as u64 > self.limits.max_path_bytes {
            anyhow::bail!(
                "output path exceeds {} bytes: {relative_path}",
                self.limits.max_path_bytes
            );
        }
        let relative_path = validated_relative_string(relative_path)?;
        let output_file = OutputSinkFileV2::start(output_sink, relative_path.clone())?;
        let mut stream = self.store.export_ranges(hash, 0..u64::MAX).stream();
        let mut file_size = 0;
        let mut exported = 0;

        while let Some(item) = stream.next().await {
            match item {
                ExportRangesItem::Size(size) => file_size = size,
                ExportRangesItem::Data(leaf) => {
                    if leaf.offset != exported {
                        anyhow::bail!(
                            "export stream for {relative_path} yielded out-of-order data"
                        );
                    }
                    exported += leaf.data.len() as u64;
                    output_file.write(leaf.data.to_vec())?;
                    self.emit_transfer(
                        transfer.protocol_id,
                        "receive",
                        "export",
                        "progress",
                        json!({
                            "total_files": total_files,
                            "current_file_index": current_file_index,
                            "file_name": relative_path,
                            "file_size": file_size,
                            "exported": exported,
                        }),
                    );
                    tokio::task::yield_now().await;
                }
                ExportRangesItem::Error(error) => {
                    anyhow::bail!("export failed for {relative_path}: {error}");
                }
            }
        }

        let published = output_file.finish()?;
        self.repository
            .record_received_artifact(ReceivedArtifactInsert {
                transfer_local_id: transfer.local_id,
                protocol_transfer_id: transfer.protocol_id,
                relative_path: &relative_path,
                locator_kind: published.locator_kind,
                locator: &published.locator,
                logical_size: exported,
            })
            .await
            .map_err(VnidropError::repository)?;
        Ok(())
    }
}
