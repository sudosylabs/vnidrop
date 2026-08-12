//! Receiver-side targeted-transfer execution and completion.

use std::sync::Arc;

use anyhow::Result;
use iroh_blobs::ticket::BlobTicket;

use super::{receive::ReceiveTarget, targeted::BlobTicketParse, CoreInner};
use crate::{
    api::{saved_device_capabilities, TargetedTransferState},
    error::VnidropError,
    secure_secret::{SecretHandle, SecretKind},
    targeted_transfer::{
        auth_secret_material,
        protocol::{CompleteTargetedTransfer, CompletionResponse, TargetedTransferProtocol},
        reconstruct_authorization, TargetedAuthorization, TargetedAuthorizationDraft,
        TargetedTransferRow,
    },
};

impl CoreInner {
    pub(super) async fn receive_targeted_transfer(
        self: &Arc<Self>,
        transfer_id: String,
        output_dir: String,
    ) -> Result<(), VnidropError> {
        let output_dir =
            crate::filesystem::platform_path(&output_dir).map_err(VnidropError::filesystem)?;
        self.receive_targeted_to_target(transfer_id, ReceiveTarget::Directory(output_dir))
            .await
    }

    pub(super) async fn receive_targeted_transfer_with_output_sink(
        self: &Arc<Self>,
        transfer_id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSink>,
    ) -> Result<(), VnidropError> {
        self.receive_targeted_to_target(transfer_id, ReceiveTarget::OutputSink(output_sink))
            .await
    }

    pub(super) async fn receive_targeted_transfer_with_output_sink_v2(
        self: &Arc<Self>,
        transfer_id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSinkV2>,
    ) -> Result<(), VnidropError> {
        self.receive_targeted_to_target(transfer_id, ReceiveTarget::OutputSinkV2(output_sink))
            .await
    }

    pub(super) async fn resume_targeted_transfer(
        self: &Arc<Self>,
        id: String,
        output_dir: String,
    ) -> Result<(), VnidropError> {
        let output_dir =
            crate::filesystem::platform_path(&output_dir).map_err(VnidropError::filesystem)?;
        self.resume_targeted_to_target(id, ReceiveTarget::Directory(output_dir))
            .await
    }

    pub(super) async fn resume_targeted_transfer_with_output_sink(
        self: &Arc<Self>,
        id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSink>,
    ) -> Result<(), VnidropError> {
        self.resume_targeted_to_target(id, ReceiveTarget::OutputSink(output_sink))
            .await
    }

    pub(super) async fn resume_targeted_transfer_with_output_sink_v2(
        self: &Arc<Self>,
        id: String,
        output_sink: Arc<dyn crate::ReceiveOutputSinkV2>,
    ) -> Result<(), VnidropError> {
        self.resume_targeted_to_target(id, ReceiveTarget::OutputSinkV2(output_sink))
            .await
    }

    async fn receive_targeted_to_target(
        self: &Arc<Self>,
        transfer_id: String,
        target: ReceiveTarget,
    ) -> Result<(), VnidropError> {
        let auth = self.load_receiver_authorization(&transfer_id).await?;
        self.run_targeted_receive(&auth, target).await
    }

    async fn resume_targeted_to_target(
        self: &Arc<Self>,
        id: String,
        target: ReceiveTarget,
    ) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        let row = store.get_row(&id).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("unknown targeted transfer"))
        })?;
        if !matches!(
            row.state,
            TargetedTransferState::Approved
                | TargetedTransferState::Connecting
                | TargetedTransferState::Transferring
                | TargetedTransferState::Interrupted
        ) {
            return Err(VnidropError::InvalidTransition {
                reason: format!(
                    "cannot resume from {}",
                    crate::targeted_transfer::state_as_str(row.state)
                ),
            });
        }
        let auth = self.load_receiver_authorization(&id).await?;
        self.run_targeted_receive(&auth, target).await
    }

    async fn load_receiver_authorization(
        &self,
        transfer_id: &str,
    ) -> Result<TargetedAuthorization, VnidropError> {
        let store = self.targeted_store();
        let row = store.get_row(transfer_id).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("unknown targeted transfer"))
        })?;
        let encoded = self.load_stored_authorization(&row).await?.ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!(
                "targeted transfer has no durable authorization"
            ))
        })?;
        let auth = TargetedAuthorization::decode(&encoded)?;
        auth.verify_for_receiver(&self.endpoint.id().to_string())?;
        Ok(auth)
    }

    async fn run_targeted_receive(
        self: &Arc<Self>,
        auth: &TargetedAuthorization,
        target: ReceiveTarget,
    ) -> Result<(), VnidropError> {
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        {
            let mut active = self
                .active_targeted_transfers
                .lock()
                .expect("active_targeted_transfers");
            match active.entry(auth.transfer_id.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(crate::runtime::ActiveTransfer {
                        direction: crate::transfer_state::TransferDirection::Receive,
                        cancel,
                    });
                }
                std::collections::hash_map::Entry::Occupied(_) => {
                    return Err(VnidropError::InvalidTransition {
                        reason: "targeted receive is already active".to_string(),
                    });
                }
            }
        }
        let result = self
            .run_registered_targeted_receive(auth, target, cancelled)
            .await;
        self.active_targeted_transfers
            .lock()
            .expect("active_targeted_transfers")
            .remove(&auth.transfer_id);
        result
    }

    async fn run_registered_targeted_receive(
        self: &Arc<Self>,
        auth: &TargetedAuthorization,
        target: ReceiveTarget,
        cancelled: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), VnidropError> {
        let store = self.targeted_store();
        if let Some(row) = store.get_row(&auth.transfer_id).await? {
            match row.state {
                TargetedTransferState::Approved | TargetedTransferState::Interrupted => {
                    store
                        .set_state(
                            &auth.transfer_id,
                            row.state,
                            TargetedTransferState::Connecting,
                        )
                        .await?;
                    self.emit_targeted_lifecycle(&auth.transfer_id, "connecting");
                    store
                        .set_state(
                            &auth.transfer_id,
                            TargetedTransferState::Connecting,
                            TargetedTransferState::Transferring,
                        )
                        .await?;
                    self.emit_targeted_lifecycle(&auth.transfer_id, "transferring");
                }
                TargetedTransferState::Connecting | TargetedTransferState::Transferring => {
                    return Err(VnidropError::InvalidTransition {
                        reason: "targeted receive is already active".to_string(),
                    });
                }
                other => {
                    return Err(VnidropError::InvalidTransition {
                        reason: format!(
                            "cannot receive from {}",
                            crate::targeted_transfer::state_as_str(other)
                        ),
                    });
                }
            }
        }

        let blob_ticket = BlobTicket::from_str_compat(&auth.blob_ticket)
            .map_err(|error| VnidropError::ticket(anyhow::anyhow!(error)))?;
        let receive_result = self
            .receive_targeted_payload(
                &auth.transfer_id,
                auth.protocol_transfer_id,
                auth.file_count,
                auth.total_size,
                blob_ticket,
                target,
                cancelled,
            )
            .await;

        match receive_result {
            Ok(()) => {
                let row = store.get_row(&auth.transfer_id).await?.ok_or_else(|| {
                    VnidropError::internal(anyhow::anyhow!("targeted transfer missing"))
                })?;
                store
                    .complete_receiver_and_enqueue(&auth.transfer_id, row.total_size)
                    .await?;
                self.emit_targeted_lifecycle(&auth.transfer_id, "completed");
                if self.acknowledge_targeted_completion(auth).await.is_ok() {
                    store.clear_pending_completion(&auth.transfer_id).await?;
                }
                Ok(())
            }
            Err(error) => {
                match store.get_row(&auth.transfer_id).await {
                    Ok(Some(row))
                        if matches!(
                            row.state,
                            TargetedTransferState::Connecting | TargetedTransferState::Transferring
                        ) =>
                    {
                        match store
                            .set_state_from_any(
                                &auth.transfer_id,
                                TargetedTransferState::Interrupted,
                            )
                            .await
                        {
                            Ok(()) => {
                                self.emit_targeted_lifecycle(&auth.transfer_id, "interrupted")
                            }
                            Err(state_error) => {
                                tracing::warn!(transfer_id = %auth.transfer_id, %state_error, "failed to persist targeted receive interruption")
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(state_error) => {
                        tracing::warn!(transfer_id = %auth.transfer_id, %state_error, "failed to load targeted receive state after error")
                    }
                }
                Err(VnidropError::transfer(error))
            }
        }
    }

    pub(super) async fn acknowledge_targeted_completion(
        &self,
        auth: &TargetedAuthorization,
    ) -> Result<(), VnidropError> {
        #[cfg(test)]
        if self
            .suppress_targeted_completion
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(VnidropError::device_unavailable(anyhow::anyhow!(
                "completion delivery suppressed by test"
            )));
        }
        let addr = self
            .device_relationships
            .peer_addr(&auth.sender_endpoint_id)
            .await?;
        let client = TargetedTransferProtocol::client(self.endpoint.clone(), addr);
        let response = tokio::time::timeout(
            self.connection_timeout(),
            client.complete_transfer(CompleteTargetedTransfer {
                transfer_id: auth.transfer_id.clone(),
                verified_bytes: auth.total_size,
                authorization: auth.encode()?,
            }),
        )
        .await
        .map_err(|_| VnidropError::device_unavailable(anyhow::anyhow!("completion timed out")))?
        .map_err(|error| VnidropError::network(anyhow::anyhow!(error)))?;
        if response != CompletionResponse::Recorded {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "sender rejected targeted completion"
            )));
        }
        Ok(())
    }

    pub(super) async fn persist_sender_authorization_and_approve(
        &self,
        authorization: &TargetedAuthorization,
    ) -> Result<(), VnidropError> {
        let custody =
            self.secret_custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "targeted authorization requires protected custody".to_string(),
                })?;
        let handle = custody
            .protect(
                SecretKind::TargetedAuthorization,
                auth_secret_material(authorization)?,
                None,
            )
            .await?;
        if let Err(error) = self
            .targeted_store()
            .finalize_sender_authorization_and_enqueue(
                &authorization.transfer_id,
                &authorization.blob_ticket,
                handle.as_str(),
            )
            .await
        {
            if let Err(cleanup_error) = custody.remove(&handle).await {
                tracing::warn!(%cleanup_error, "failed to roll back targeted sender authorization secret");
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn load_stored_authorization(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<Option<String>, VnidropError> {
        let (Some(handle), Some(blob_ticket)) = (
            row.authorization_secret_handle.as_ref(),
            row.blob_ticket.as_ref(),
        ) else {
            return Ok(None);
        };
        let custody =
            self.secret_custody
                .as_ref()
                .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                    reason: "targeted authorization requires protected custody".to_string(),
                })?;
        let material = custody
            .load(&SecretHandle::from_stored(handle.clone()))
            .await?;
        let auth = reconstruct_authorization(
            TargetedAuthorizationDraft {
                transfer_id: row.id.clone(),
                protocol_transfer_id: row.protocol_transfer_id,
                sender_endpoint_id: row.sender_endpoint_id.clone(),
                receiver_endpoint_id: row.receiver_endpoint_id.clone(),
                manifest_id: row.manifest_id.clone(),
                content_hash: row.content_hash.clone(),
                file_count: row.file_count,
                total_size: row.total_size,
                protocol_version: saved_device_capabilities().targeted_transfer_protocol_version,
                transfer_name: row.transfer_name.clone(),
                blob_ticket: blob_ticket.clone(),
            },
            &material,
        )?;
        Ok(Some(auth.encode()?))
    }
}
