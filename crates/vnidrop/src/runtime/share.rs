use std::sync::Arc;

use anyhow::{Context, Result};
use futures_lite::StreamExt as _;
use iroh_blobs::{
    api::{blobs::AddProgressItem, TempTag},
    format::collection::Collection,
    ticket::BlobTicket,
    BlobFormat,
};
use n0_future::BufferedStreamExt;
use serde_json::json;
use tokio::sync::oneshot;

use super::{share_tag_name, ActiveTransfer, CoreInner};
use crate::{
    access_policy::mode_to_storage,
    api::TransferMetadata,
    api::{ShareMetadataInput, ShareResult, ShareSource},
    error::VnidropError,
    filesystem::{
        collect_import_files_with_limits, default_collection_name,
        read_stream_from_blocking_reader, TransferImport,
    },
    repository::TransferUpsert,
    ticket::VnidropTicket,
    transfer_state::{TransferDirection, TransferStatus},
    util::non_empty,
};

impl CoreInner {
    pub(super) async fn share_files(
        self: &Arc<Self>,
        sources: Vec<ShareSource>,
        metadata: ShareMetadataInput,
    ) -> Result<ShareResult> {
        let _permit = self
            .transfer_slots
            .acquire()
            .await
            .context("transfer limiter is closed")
            .map_err(VnidropError::internal)?;
        let transfer_id = metadata.transfer_id;
        if sources.is_empty() {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "at least one source is required"
            ))
            .into());
        }
        if sources.len() as u64 > self.limits.max_sources {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "source count {} exceeds limit {}",
                sources.len(),
                self.limits.max_sources
            ))
            .into());
        }
        self.limits
            .validate_metadata_text("transfer name", metadata.transfer_name.as_deref())
            .map_err(VnidropError::invalid_input)?;
        self.limits
            .validate_metadata_text("sender name", metadata.sender_name.as_deref())
            .map_err(VnidropError::invalid_input)?;
        self.repository
            .insert_transfer(TransferUpsert {
                transfer_id,
                peer_id: None,
                direction: TransferDirection::Send,
                status: TransferStatus::Importing,
                transfer_name: metadata.transfer_name.as_deref(),
                content_hash: None,
                ticket: None,
                file_count: 0,
                total_size: 0,
                access_mode: mode_to_storage(&metadata.access_mode),
            })
            .await
            .map_err(VnidropError::repository)?;
        let (cancel, mut cancelled) = oneshot::channel();
        self.active_transfers
            .lock()
            .expect("active_transfers")
            .insert(
                transfer_id,
                ActiveTransfer {
                    direction: TransferDirection::Send,
                    cancel,
                },
            );
        let (result, was_cancelled) = tokio::select! {
            result = self.share_files_inner(sources, metadata) => {
                (result.map_err(VnidropError::transfer), false)
            },
            _ = &mut cancelled => (Err(VnidropError::cancelled("transfer cancelled")), true),
        };
        self.active_transfers
            .lock()
            .expect("active_transfers")
            .remove(&transfer_id);
        if let Err(error) = &result {
            if was_cancelled {
                self.emit_transfer(transfer_id, "send", "lifecycle", "cancelled", json!({}));
            } else {
                self.emit_transfer(
                    transfer_id,
                    "send",
                    "error",
                    "failed",
                    json!({ "code": error.code(), "reason": error.reason() }),
                );
                let _ = self
                    .repository
                    .transition_transfer_status(
                        transfer_id,
                        TransferStatus::Importing,
                        TransferStatus::Failed,
                    )
                    .await;
            }
        }
        result.map_err(anyhow::Error::new)
    }

    pub(super) async fn share_files_inner(
        self: &Arc<Self>,
        sources: Vec<ShareSource>,
        metadata: ShareMetadataInput,
    ) -> Result<ShareResult> {
        let access_mode = metadata.access_mode.clone();
        self.emit_transfer(
            metadata.transfer_id,
            "send",
            "import",
            "started",
            json!({ "source_count": sources.len() }),
        );
        let import = self.import_sources(metadata.transfer_id, sources).await?;
        let blob_ticket =
            BlobTicket::new(self.endpoint.addr(), import.root_hash, BlobFormat::HashSeq);
        let transfer_name = metadata
            .transfer_name
            .and_then(non_empty)
            .unwrap_or_else(|| import.default_name.clone());
        let ticket_metadata = TransferMetadata::new(
            metadata.transfer_id,
            transfer_name.clone(),
            metadata.sender_name,
            import.root_hash,
            import.file_count,
            import.total_size,
        );
        let ticket = VnidropTicket::new_with_relay_urls(
            blob_ticket,
            ticket_metadata,
            &self.custom_relay_urls,
        )
        .encode()
        .context("failed to encode VniDrop transfer ticket")?;
        let content_hash = import.root_hash.to_string();
        let local_id = self
            .repository
            .transfer_local_id(metadata.transfer_id)
            .await
            .map_err(VnidropError::repository)?;
        let tag_name = share_tag_name(&local_id);
        self.store
            .tags()
            .set(
                &tag_name,
                (import.root_hash, iroh_blobs::BlobFormat::HashSeq),
            )
            .await?;

        // Persist the completed share before exposing it through the provider.
        // The remaining in-memory registrations are infallible and can be
        // reconstructed from SQLite if the process exits immediately after.
        if let Err(error) = self
            .repository
            .complete_share_import(TransferUpsert {
                transfer_id: metadata.transfer_id,
                peer_id: None,
                direction: TransferDirection::Send,
                status: TransferStatus::Sharing,
                transfer_name: Some(&transfer_name),
                content_hash: Some(&content_hash),
                ticket: Some(&ticket),
                file_count: import.file_count,
                total_size: import.total_size,
                access_mode: mode_to_storage(&access_mode),
            })
            .await
        {
            let _ = self.store.tags().delete(&tag_name).await;
            return Err(VnidropError::repository(error).into());
        }
        // Map root + every collection member so provider ACL cannot fail-open
        // on child blob hashes that are not the collection root.
        self.register_share_hashes(
            metadata.transfer_id,
            std::iter::once(import.root_hash).chain(import.member_hashes.iter().copied()),
        )
        .await;
        self.access_policy
            .set_mode(metadata.transfer_id, access_mode)
            .await;
        self.active_shares
            .lock()
            .await
            .insert(metadata.transfer_id, ());
        drop(import.tag);

        // Tickets are capabilities: never persist the full string in events.
        self.emit_transfer(
            metadata.transfer_id,
            "send",
            "ticket",
            "created",
            json!({
                "hash": import.root_hash.to_string(),
                "total_size": import.total_size,
                "file_count": import.file_count,
            }),
        );

        Ok(ShareResult {
            transfer_id: metadata.transfer_id,
            ticket,
            hash: import.root_hash.to_string(),
            transfer_name,
            file_count: import.file_count,
            total_size: import.total_size,
        })
    }

    pub(super) async fn import_sources(
        self: &Arc<Self>,
        transfer_id: u64,
        sources: Vec<ShareSource>,
    ) -> Result<TransferImport> {
        let files = collect_import_files_with_limits(sources, &self.limits)?;
        let default_name = default_collection_name(&files);
        let parallelism = num_cpus::get().min(8);
        let mut names_and_tags = n0_future::stream::iter(files)
            .map(|file| {
                let core = self.clone();
                async move {
                    let reader = file.source.open()?;
                    let stream = read_stream_from_blocking_reader(reader);
                    let import = core.store.add_stream(stream).await;
                    let (tag, size) = core
                        .consume_add_progress(
                            transfer_id,
                            file.collection_name.clone(),
                            import.stream().await,
                        )
                        .await?;
                    Result::<_>::Ok((file.collection_name, tag, size))
                }
            })
            .buffered_unordered(parallelism)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        names_and_tags.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
        let total_size = names_and_tags
            .iter()
            .try_fold(0u64, |total, (_, _, size)| total.checked_add(*size))
            .context("collection size overflow")?;
        if total_size > self.limits.max_total_bytes {
            anyhow::bail!(
                "collection size {total_size} exceeds transfer limit {}",
                self.limits.max_total_bytes
            );
        }
        let (collection, tags) = names_and_tags
            .into_iter()
            .map(|(name, tag, _)| ((name, tag.hash()), tag))
            .unzip::<_, _, Collection, Vec<_>>();
        let member_hashes = collection.iter().map(|(_, hash)| *hash).collect::<Vec<_>>();
        let collection_tag = collection.clone().store(&self.store).await?;
        let root_hash = collection_tag.hash();
        let file_count = tags.len() as u64;
        drop(tags);

        self.emit_transfer(
            transfer_id,
            "send",
            "import",
            "done",
            json!({ "file_count": file_count, "total_size": total_size }),
        );

        Ok(TransferImport {
            tag: collection_tag,
            root_hash,
            member_hashes,
            total_size,
            file_count,
            default_name,
        })
    }

    pub(super) async fn consume_add_progress(
        &self,
        transfer_id: u64,
        file_name: String,
        mut stream: impl futures_lite::Stream<Item = AddProgressItem> + Unpin,
    ) -> Result<(TempTag, u64)> {
        let mut size = 0;
        loop {
            let Some(item) = stream.next().await else {
                anyhow::bail!("import stream ended without a tag");
            };
            match item {
                AddProgressItem::Size(item_size) => {
                    if item_size > self.limits.max_total_bytes {
                        anyhow::bail!(
                            "file size {item_size} exceeds transfer limit {}",
                            self.limits.max_total_bytes
                        );
                    }
                    size = item_size;
                }
                AddProgressItem::CopyProgress(offset) => {
                    self.emit_transfer(
                        transfer_id,
                        "send",
                        "import",
                        "copy-progress",
                        json!({ "file_name": file_name, "offset": offset, "size": size }),
                    );
                }
                AddProgressItem::CopyDone => {
                    self.emit_transfer(
                        transfer_id,
                        "send",
                        "import",
                        "copy-done",
                        json!({ "file_name": file_name, "size": size }),
                    );
                }
                AddProgressItem::OutboardProgress(offset) => {
                    self.emit_transfer(
                        transfer_id,
                        "send",
                        "import",
                        "outboard-progress",
                        json!({ "file_name": file_name, "offset": offset }),
                    );
                }
                AddProgressItem::Done(tag) => return Ok((tag, size)),
                AddProgressItem::Error(error) => anyhow::bail!("import failed: {error}"),
            }
        }
    }
}
