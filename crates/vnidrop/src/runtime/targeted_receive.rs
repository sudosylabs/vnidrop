//! Receiver-side targeted-transfer execution and completion.

use std::sync::Arc;

use anyhow::Result;
use iroh_blobs::ticket::BlobTicket;

use super::{receive::ReceiveTarget, targeted::BlobTicketParse, CoreInner};
use crate::{
    api::TargetedTransferState,
    error::VnidropError,
    targeted_transfer::{
        protocol::{CompleteTargetedTransfer, CompletionResponse, TargetedTransferProtocol},
        TargetedAuthorization, TargetedTransferRow,
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
        let store = &self.targeted_transfers;
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
        let store = &self.targeted_transfers;
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
        let store = &self.targeted_transfers;
        if let Some(row) = store.get_row(&auth.transfer_id).await? {
            match row.state {
                TargetedTransferState::Approved | TargetedTransferState::Interrupted => {
                    store.begin_receive(&auth.transfer_id, row.state).await?;
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
                    .complete_receiver(&auth.transfer_id, row.total_size)
                    .await?;
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
                        match store.interrupt_receive(&auth.transfer_id).await {
                            Ok(()) => {}
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
        self.targeted_transfers
            .protect_sender_authorization(authorization)
            .await
    }

    pub(crate) async fn load_stored_authorization(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<Option<String>, VnidropError> {
        self.targeted_transfers.load_authorization(row).await
    }
}
