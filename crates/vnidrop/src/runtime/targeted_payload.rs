//! Shared blob download and export path for targeted transfers.

use std::sync::Arc;

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use iroh_blobs::{
    api::remote::GetProgressItem, format::collection::Collection,
    get::request::get_hash_seq_and_sizes, ticket::BlobTicket,
};
use serde_json::json;
use tokio::sync::oneshot;

use super::{receive::ReceiveTarget, CoreInner};
use crate::{error::VnidropError, ticket::filter_peer_addr_for_relay_mode};

impl CoreInner {
    #[allow(
        clippy::too_many_arguments,
        reason = "targeted receive binds immutable authorization fields plus its cancel token"
    )]
    pub(super) async fn receive_targeted_payload(
        self: &Arc<Self>,
        targeted_transfer_id: &str,
        transfer_id: u64,
        expected_file_count: u64,
        expected_payload_size: u64,
        mut blob_ticket: BlobTicket,
        target: ReceiveTarget,
        mut cancelled: oneshot::Receiver<()>,
    ) -> Result<()> {
        let sender_addr = filter_peer_addr_for_relay_mode(
            blob_ticket.addr(),
            self.relay_mode,
            &self.custom_relay_urls,
        )
        .map_err(VnidropError::network)?;
        blob_ticket = BlobTicket::new(sender_addr, blob_ticket.hash(), blob_ticket.format());
        let _permit = tokio::select! {
            biased;
            _ = &mut cancelled => return Err(VnidropError::cancelled("transfer cancelled").into()),
            permit = self.transfer_slots.acquire() => permit
                .context("transfer limiter is closed")
                .map_err(VnidropError::internal)?,
        };
        tokio::select! {
            biased;
            _ = &mut cancelled => Err(VnidropError::cancelled("transfer cancelled").into()),
            result = self.download_targeted_payload(
                targeted_transfer_id,
                transfer_id,
                expected_file_count,
                expected_payload_size,
                blob_ticket,
                target,
            ) => result,
        }
    }

    async fn download_targeted_payload(
        &self,
        targeted_transfer_id: &str,
        transfer_id: u64,
        expected_file_count: u64,
        expected_payload_size: u64,
        blob_ticket: BlobTicket,
        target: ReceiveTarget,
    ) -> Result<()> {
        if let ReceiveTarget::Directory(output_dir) = &target {
            tokio::fs::create_dir_all(output_dir)
                .await
                .map_err(VnidropError::filesystem)?;
        }
        let connection = self
            .endpoint
            .connect(blob_ticket.addr().clone(), iroh_blobs::ALPN)
            .await
            .map_err(VnidropError::network)?;
        let hash_and_format = blob_ticket.hash_and_format();
        let (hash_seq, sizes) =
            get_hash_seq_and_sizes(&connection, &hash_and_format.hash, 1024 * 1024 * 32, None)
                .await
                .context("failed to get targeted payload sizes")
                .map_err(VnidropError::network)?;
        let remote_size = sizes
            .iter()
            .try_fold(0u64, |total, size| total.checked_add(*size))
            .context("remote collection size overflow")?;
        let total_files = sizes.len().saturating_sub(1) as u64;
        if total_files != expected_file_count {
            anyhow::bail!(
                "targeted payload file count {total_files} does not match authorized count {expected_file_count}"
            );
        }
        let hash_sequence_bytes = hash_seq.len() as u64 * 32;
        let collection_metadata_bytes = sizes.first().copied().unwrap_or(0);
        let payload_size = sizes
            .iter()
            .skip(1)
            .try_fold(0u64, |total, size| total.checked_add(*size))
            .context("remote targeted payload size overflow")?;
        if payload_size != expected_payload_size {
            anyhow::bail!(
                "targeted payload size {payload_size} does not match authorized size {expected_payload_size}"
            );
        }
        if total_files > self.limits.max_collection_files {
            anyhow::bail!(
                "remote collection has {total_files} files, limit is {}",
                self.limits.max_collection_files
            );
        }
        if remote_size > self.limits.max_total_bytes {
            anyhow::bail!(
                "remote collection size {remote_size} exceeds limit {}",
                self.limits.max_total_bytes
            );
        }
        let download_tag = self.store.tags().temp_tag(hash_and_format).await?;
        let get = self.store.remote().fetch(connection, hash_and_format);
        let mut stream = get.stream();
        loop {
            let Some(item) = stream.next().await else {
                anyhow::bail!("targeted download ended without completion");
            };
            match item {
                GetProgressItem::Progress(_) => {
                    let verified = self
                        .store
                        .remote()
                        .local(hash_and_format)
                        .await?
                        .local_bytes()
                        .saturating_sub(hash_sequence_bytes)
                        .saturating_sub(collection_metadata_bytes)
                        .min(payload_size);
                    if self
                        .targeted_store()
                        .advance_verified_bytes(targeted_transfer_id, verified)
                        .await?
                    {
                        self.emit_transfer(
                            transfer_id,
                            "receive",
                            "download",
                            "progress",
                            json!({ "downloaded": verified, "total_size": payload_size }),
                        );
                        self.emit_targeted_lifecycle(targeted_transfer_id, "progress");
                    }
                }
                GetProgressItem::Done(_) => {
                    let verified = self
                        .store
                        .remote()
                        .local(hash_and_format)
                        .await?
                        .local_bytes()
                        .saturating_sub(hash_sequence_bytes)
                        .saturating_sub(collection_metadata_bytes)
                        .min(payload_size);
                    if verified != payload_size {
                        anyhow::bail!(
                            "targeted payload completed with {verified} verified bytes, expected {payload_size}"
                        );
                    }
                    if self
                        .targeted_store()
                        .advance_verified_bytes(targeted_transfer_id, verified)
                        .await?
                    {
                        self.emit_transfer(
                            transfer_id,
                            "receive",
                            "download",
                            "progress",
                            json!({ "downloaded": verified, "total_size": payload_size }),
                        );
                        self.emit_targeted_lifecycle(targeted_transfer_id, "progress");
                    }
                    break;
                }
                GetProgressItem::Error(error) => {
                    return Err(VnidropError::network(anyhow::anyhow!(
                        "targeted download failed: {error}"
                    ))
                    .into());
                }
            }
        }
        let collection = Collection::load(hash_and_format.hash, self.store.as_ref()).await?;
        self.export_collection_untracked(transfer_id, total_files, target, collection)
            .await?;
        drop(download_tag);
        Ok(())
    }
}
