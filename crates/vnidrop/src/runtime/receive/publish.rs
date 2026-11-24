use super::*;

#[derive(Clone, Copy)]
struct ReceivedTransfer<'a> {
    protocol_id: u64,
    local_id: Option<&'a str>,
    targeted_id: Option<&'a str>,
    sink_destination: Option<&'a str>,
}

struct OutputSinkFile<'a> {
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

struct OutputSinkFileV2<'a> {
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
    pub(in crate::runtime) async fn export_collection(
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
            local_id: Some(&transfer_local_id),
            targeted_id: None,
            sink_destination: None,
        };
        self.export_received_collection(received_transfer, total_files, target, collection)
            .await
    }

    pub(in crate::runtime) async fn export_targeted_collection(
        &self,
        targeted_id: &str,
        transfer_id: u64,
        total_files: u64,
        target: ReceiveTarget,
        collection: Collection,
    ) -> Result<()> {
        let destination = match &target {
            ReceiveTarget::OutputSinkV2(_, Some(verifier)) => Some(verifier.destination_key()),
            _ => None,
        };
        if destination
            .as_ref()
            .is_some_and(|key| key.is_empty() || key.len() as u64 > self.limits.max_path_bytes)
        {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "invalid output destination key"
            ))
            .into());
        }
        let transfer = ReceivedTransfer {
            protocol_id: transfer_id,
            local_id: None,
            targeted_id: Some(targeted_id),
            sink_destination: destination.as_deref(),
        };
        self.export_received_collection(transfer, total_files, target, collection)
            .await
    }

    async fn export_received_collection(
        &self,
        received_transfer: ReceivedTransfer<'_>,
        total_files: u64,
        target: ReceiveTarget,
        collection: Collection,
    ) -> Result<()> {
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
                        received_transfer.protocol_id,
                        total_files,
                        i as u64,
                        output_sink.as_ref(),
                        name.as_ref(),
                        *hash,
                    )
                    .await?;
                }
                ReceiveTarget::OutputSinkV2(output_sink, verifier) => {
                    if let (Some(id), Some(destination), Some(verifier)) = (
                        received_transfer.targeted_id,
                        received_transfer.sink_destination,
                        verifier,
                    ) {
                        if self
                            .verify_targeted_sink_output(
                                id,
                                destination,
                                name,
                                *hash,
                                verifier.clone(),
                            )
                            .await?
                        {
                            continue;
                        }
                    }
                    self.export_blob_to_sink_v2(
                        received_transfer,
                        total_files,
                        i as u64,
                        (output_sink.as_ref(), verifier.as_deref()),
                        name,
                        *hash,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn verify_targeted_sink_output(
        &self,
        id: &str,
        destination: &str,
        relative_path: &str,
        hash: Hash,
        verifier: Arc<dyn TargetedOutputVerifier>,
    ) -> Result<bool> {
        let relative_path = validated_relative_string(relative_path)?;
        let Some((locator, published)) = self
            .targeted_transfers
            .sink_output_checkpoint(id, &relative_path, destination)
            .await?
        else {
            return Ok(false);
        };
        let iroh_blobs::api::proto::BlobStatus::Complete { size } =
            self.store.blobs().status(hash).await?
        else {
            anyhow::bail!("verified blob is unavailable for receive resumption");
        };
        let timeout = self.connection_timeout();
        tokio::task::spawn_blocking(move || {
            let Some(descriptor) =
                verifier.open_published_output(relative_path, locator, published)?
            else {
                return Ok(false);
            };
            Ok(crate::filesystem::verify_owned_descriptor(
                descriptor, hash, size, timeout,
            )?)
        })
        .await?
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
        let checkpoint = if let Some(id) = transfer.targeted_id {
            let path = std::path::absolute(safe_output_path(output_dir, relative_path)?)
                .map_err(VnidropError::filesystem)?;
            let locator = path.to_string_lossy().into_owned();
            if let Some(identity) = self
                .targeted_transfers
                .receive_output_identity(id, &locator)
                .await?
            {
                let verified = tokio::task::spawn_blocking(move || {
                    verify_published_output(&path, &identity, hash)
                })
                .await
                .map_err(VnidropError::internal)?
                .map_err(VnidropError::filesystem)?;
                if verified {
                    return Ok(());
                }
            }
            Some((id, locator))
        } else {
            None
        };
        let (pending_file, writer) =
            AtomicOutputFile::create_tracked(output_dir, relative_path, Some(&self.app_data_dir))
                .map_err(VnidropError::filesystem)?;
        let (tx, rx) = async_channel::bounded::<io::Result<Option<Bytes>>>(2);
        let writer_task = std::thread::spawn(move || write_stream_to_blocking_writer(writer, rx));
        let stream = self.store.export_ranges(hash, 0..u64::MAX).stream();
        let mut export = BufferedExport::new(stream, relative_path);
        while let Some(chunk) = export.next_chunk().await? {
            tx.send(Ok(Some(Bytes::from(chunk))))
                .await
                .map_err(|_| anyhow::anyhow!("export writer closed for {relative_path}"))?;
            export.report_progress(self, transfer.protocol_id, total_files, current_file_index);
        }

        tx.send(Ok(None))
            .await
            .map_err(|_| anyhow::anyhow!("export writer closed for {relative_path}"))?;
        let writer = wait_for_writer(writer_task)
            .await
            .map_err(VnidropError::internal)?
            .map_err(VnidropError::filesystem)?;
        let track_output = checkpoint.is_some();
        let identity = tokio::task::spawn_blocking(move || {
            writer.sync_all()?;
            track_output
                .then(|| output_file_identity(&writer))
                .transpose()
        })
        .await
        .map_err(VnidropError::internal)?
        .map_err(VnidropError::filesystem)?;
        if let (Some((id, locator)), Some(identity)) = (&checkpoint, &identity) {
            // Rename and hard-link preserve this identity, so a crash just before
            // or after publication still leaves an unambiguous checkpoint.
            self.targeted_transfers
                .record_receive_output(id, locator, identity)
                .await?;
        }
        let locator = pending_file.target().to_string_lossy().to_string();
        if let Err(error) = pending_file.commit().map_err(VnidropError::filesystem) {
            if matches!(error, VnidropError::DestinationExists { .. }) {
                if let Some((id, target)) = checkpoint {
                    self.targeted_transfers
                        .forget_receive_output(id, &target)
                        .await?;
                }
            }
            return Err(error.into());
        }
        if let Some(local_id) = transfer.local_id {
            self.repository
                .record_received_artifact(ReceivedArtifactInsert {
                    transfer_local_id: local_id,
                    protocol_transfer_id: transfer.protocol_id,
                    relative_path,
                    locator_kind: ReceivedLocatorKind::FilesystemPath,
                    locator: &locator,
                    logical_size: export.exported_bytes(),
                })
                .await
                .map_err(VnidropError::repository)?;
        }
        Ok(())
    }

    pub(in crate::runtime) async fn export_blob_to_sink(
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
        let stream = self.store.export_ranges(hash, 0..u64::MAX).stream();
        let mut export = BufferedExport::new(stream, &relative_path);
        while let Some(chunk) = export.next_chunk().await? {
            output_file.write(chunk)?;
            export.report_progress(self, transfer_id, total_files, current_file_index);
            // The synchronous foreign write may have been the only blocking
            // call in this poll, so yield before publishing or writing again.
            tokio::task::yield_now().await;
        }

        output_file.finish()?;
        Ok(())
    }

    async fn export_blob_to_sink_v2(
        &self,
        transfer: ReceivedTransfer<'_>,
        total_files: u64,
        current_file_index: u64,
        sink: (
            &dyn ReceiveOutputSinkV2,
            Option<&dyn TargetedOutputVerifier>,
        ),
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
        let output_file = OutputSinkFileV2::start(sink.0, relative_path.clone())?;
        let stream = self.store.export_ranges(hash, 0..u64::MAX).stream();
        let mut export = BufferedExport::new(stream, &relative_path);
        while let Some(chunk) = export.next_chunk().await? {
            output_file.write(chunk)?;
            export.report_progress(self, transfer.protocol_id, total_files, current_file_index);
            tokio::task::yield_now().await;
        }

        let checkpoint = transfer
            .targeted_id
            .zip(transfer.sink_destination)
            .zip(sink.1);
        if let Some(((id, destination), verifier)) = checkpoint {
            let locator = verifier.pending_output_locator(relative_path.clone())?;
            self.targeted_transfers
                .record_sink_output(id, &relative_path, destination, &locator, false)
                .await?;
        }
        let published = match output_file.finish() {
            Ok(published) => published,
            Err(error) => {
                if matches!(
                    error.downcast_ref::<VnidropError>(),
                    Some(VnidropError::DestinationExists { .. })
                ) {
                    if let Some(((id, destination), _)) = checkpoint {
                        self.targeted_transfers
                            .forget_sink_output(id, &relative_path, destination)
                            .await?;
                    }
                }
                return Err(error);
            }
        };
        if let Some(((id, destination), _)) = checkpoint {
            self.targeted_transfers
                .record_sink_output(id, &relative_path, destination, &published.locator, true)
                .await?;
        }
        if let Some(local_id) = transfer.local_id {
            self.repository
                .record_received_artifact(ReceivedArtifactInsert {
                    transfer_local_id: local_id,
                    protocol_transfer_id: transfer.protocol_id,
                    relative_path: &relative_path,
                    locator_kind: published.locator_kind,
                    locator: &published.locator,
                    logical_size: export.exported_bytes(),
                })
                .await
                .map_err(VnidropError::repository)?;
        }
        Ok(())
    }
}
