//! Protected authorization storage owned by the Targeted transfer module.

use std::sync::Arc;

use crate::{
    api::{saved_device_capabilities, TargetedTransferState},
    error::VnidropError,
    invitation::Repository,
    secure_secret::{SecretCustody, SecretHandle, SecretKind},
};

use super::{
    auth_secret_material, reconstruct_authorization, TargetedAuthorization,
    TargetedAuthorizationDraft, TargetedTransferRole, TargetedTransferRow, TargetedTransferStore,
};

type EmitLifecycle = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub(super) struct AuthorizationCustody {
    store: TargetedTransferStore,
    repository: Repository,
    custody: Option<Arc<SecretCustody>>,
    emit_lifecycle: EmitLifecycle,
}

impl AuthorizationCustody {
    pub(super) fn new(
        store: TargetedTransferStore,
        repository: Repository,
        custody: Option<Arc<SecretCustody>>,
        emit_lifecycle: EmitLifecycle,
    ) -> Self {
        Self {
            store,
            repository,
            custody,
            emit_lifecycle,
        }
    }

    pub(super) async fn persist_receiver(
        &self,
        authorization: TargetedAuthorization,
    ) -> Result<bool, VnidropError> {
        let custody = self.require_custody()?;
        if let Some(row) = self.store.get_row(&authorization.transfer_id).await? {
            let exact = row.role == TargetedTransferRole::Receiver
                && matches!(
                    row.state,
                    TargetedTransferState::Approved
                        | TargetedTransferState::Connecting
                        | TargetedTransferState::Transferring
                        | TargetedTransferState::Interrupted
                        | TargetedTransferState::Completed
                )
                && row.protocol_transfer_id == authorization.protocol_transfer_id
                && row.sender_endpoint_id == authorization.sender_endpoint_id
                && row.receiver_endpoint_id == authorization.receiver_endpoint_id
                && row.manifest_id == authorization.manifest_id
                && row.content_hash == authorization.content_hash
                && row.transfer_name == authorization.transfer_name
                && row.file_count == authorization.file_count
                && row.total_size == authorization.total_size
                && row.blob_ticket.as_deref() == Some(authorization.blob_ticket.as_str());
            let Some(handle) = row.authorization_secret_handle else {
                return Err(VnidropError::SecureStorageMissing {
                    reason: "receiver authorization handle is missing".to_string(),
                });
            };
            if !exact {
                return Err(VnidropError::permission(anyhow::anyhow!(
                    "targeted authorization conflicts with receiver state"
                )));
            }
            let material = custody.load(&SecretHandle::from_stored(handle)).await?;
            let rebuilt = reconstruct_authorization(
                TargetedAuthorizationDraft {
                    transfer_id: row.id,
                    protocol_transfer_id: row.protocol_transfer_id,
                    sender_endpoint_id: row.sender_endpoint_id,
                    receiver_endpoint_id: row.receiver_endpoint_id,
                    manifest_id: row.manifest_id,
                    content_hash: row.content_hash,
                    file_count: row.file_count,
                    total_size: row.total_size,
                    protocol_version: authorization.protocol_version,
                    transfer_name: row.transfer_name,
                    blob_ticket: row.blob_ticket.expect("checked blob ticket"),
                },
                &material,
            )?;
            if rebuilt.encode()? != authorization.encode()? {
                return Err(VnidropError::permission(anyhow::anyhow!(
                    "protected receiver authorization does not match delivery"
                )));
            }
            return Ok(false);
        }
        let invitation_collision = self
            .repository
            .list_transfers()
            .await
            .map_err(VnidropError::repository)?
            .into_iter()
            .any(|transfer| transfer.transfer_id == authorization.protocol_transfer_id);
        if invitation_collision {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "targeted transfer protocol id collides with invitation work"
            )));
        }
        let handle = custody
            .protect(
                SecretKind::TargetedAuthorization,
                auth_secret_material(&authorization)?,
                None,
            )
            .await?;
        let created = match self
            .store
            .persist_receiver_authorization_and_consume_intent(&authorization, handle.as_str())
            .await
        {
            Ok(created) => created,
            Err(error) => {
                if let Err(cleanup_error) = custody.remove(&handle).await {
                    tracing::warn!(%cleanup_error, "failed to roll back receiver authorization secret");
                }
                return Err(error);
            }
        };
        if created {
            (self.emit_lifecycle)("approved");
        }
        Ok(created)
    }

    pub(super) async fn protect_sender(
        &self,
        authorization: &TargetedAuthorization,
    ) -> Result<(), VnidropError> {
        let custody = self.require_custody()?;
        let handle = custody
            .protect(
                SecretKind::TargetedAuthorization,
                auth_secret_material(authorization)?,
                None,
            )
            .await?;
        if let Err(error) = self
            .store
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
        (self.emit_lifecycle)("approved");
        Ok(())
    }

    pub(super) async fn load(
        &self,
        row: &TargetedTransferRow,
    ) -> Result<Option<String>, VnidropError> {
        let (Some(handle), Some(blob_ticket)) = (
            row.authorization_secret_handle.as_ref(),
            row.blob_ticket.as_ref(),
        ) else {
            return Ok(None);
        };
        let material = self
            .require_custody()?
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

    fn require_custody(&self) -> Result<&SecretCustody, VnidropError> {
        self.custody
            .as_deref()
            .ok_or_else(|| VnidropError::SecureStorageUnavailable {
                reason: "targeted authorization requires protected custody".to_string(),
            })
    }
}
