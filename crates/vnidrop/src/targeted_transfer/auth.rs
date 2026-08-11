//! Bound post-approval authorization for a single targeted transfer.
//!
//! The pre-approval offer never carries this material. After explicit approval,
//! the sender issues a capability whose MAC binds the exact recipient, sender,
//! transfer, manifest, hashes, sizes, and protocol generation. Tampering with
//! the receiver identity invalidates the MAC; presenting an intact capability
//! from another endpoint still fails provider ACL and local identity checks.

use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
use serde::{Deserialize, Serialize};

use crate::error::VnidropError;

const AUTH_CONTEXT: &[u8] = b"vnidrop-targeted-auth-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TargetedAuthorization {
    pub(crate) transfer_id: String,
    pub(crate) protocol_transfer_id: u64,
    pub(crate) sender_endpoint_id: String,
    pub(crate) receiver_endpoint_id: String,
    pub(crate) manifest_id: String,
    pub(crate) content_hash: String,
    pub(crate) file_count: u64,
    pub(crate) total_size: u64,
    pub(crate) protocol_version: u16,
    pub(crate) transfer_name: String,
    /// BlobTicket string used only after approval to pull through existing sinks.
    pub(crate) blob_ticket: String,
    pub(crate) auth_secret: String,
    pub(crate) mac: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetedAuthorizationDraft {
    pub(crate) transfer_id: String,
    pub(crate) protocol_transfer_id: u64,
    pub(crate) sender_endpoint_id: String,
    pub(crate) receiver_endpoint_id: String,
    pub(crate) manifest_id: String,
    pub(crate) content_hash: String,
    pub(crate) file_count: u64,
    pub(crate) total_size: u64,
    pub(crate) protocol_version: u16,
    pub(crate) transfer_name: String,
    pub(crate) blob_ticket: String,
}

impl TargetedAuthorization {
    pub(crate) fn issue(draft: TargetedAuthorizationDraft) -> Result<Self, VnidropError> {
        let auth_secret = crate::grant::GrantSecret::generate().encode();
        let mut auth = Self {
            transfer_id: draft.transfer_id,
            protocol_transfer_id: draft.protocol_transfer_id,
            sender_endpoint_id: draft.sender_endpoint_id,
            receiver_endpoint_id: draft.receiver_endpoint_id,
            manifest_id: draft.manifest_id,
            content_hash: draft.content_hash,
            file_count: draft.file_count,
            total_size: draft.total_size,
            protocol_version: draft.protocol_version,
            transfer_name: draft.transfer_name,
            blob_ticket: draft.blob_ticket,
            auth_secret,
            mac: String::new(),
        };
        auth.mac = HEXLOWER.encode(&auth.compute_mac()?);
        Ok(auth)
    }

    pub(crate) fn encode(&self) -> Result<String, VnidropError> {
        let bytes = serde_json::to_vec(self).map_err(VnidropError::internal)?;
        Ok(format!("vndta1:{}", BASE64URL_NOPAD.encode(&bytes)))
    }

    pub(crate) fn decode(value: &str) -> Result<Self, VnidropError> {
        let encoded = value.strip_prefix("vndta1:").ok_or_else(|| {
            VnidropError::invalid_input(anyhow::anyhow!("not a targeted authorization"))
        })?;
        let bytes = BASE64URL_NOPAD.decode(encoded.as_bytes()).map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid targeted authorization encoding"))
        })?;
        let auth: Self = serde_json::from_slice(&bytes).map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid targeted authorization payload"))
        })?;
        auth.verify_integrity()?;
        Ok(auth)
    }

    pub(crate) fn verify_for_receiver(&self, local_endpoint_id: &str) -> Result<(), VnidropError> {
        self.verify_integrity()?;
        if self.receiver_endpoint_id != local_endpoint_id {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "targeted authorization is bound to a different receiver"
            )));
        }
        Ok(())
    }

    fn verify_integrity(&self) -> Result<(), VnidropError> {
        let expected = self.compute_mac()?;
        let presented = HEXLOWER.decode(self.mac.as_bytes()).map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid authorization mac"))
        })?;
        if presented.as_slice() != expected.as_slice() {
            return Err(VnidropError::permission(anyhow::anyhow!(
                "targeted authorization mac mismatch"
            )));
        }
        Ok(())
    }

    fn compute_mac(&self) -> Result<[u8; 32], VnidropError> {
        let secret_bytes = HEXLOWER.decode(self.auth_secret.as_bytes()).map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid authorization secret"))
        })?;
        let key: [u8; 32] = secret_bytes.try_into().map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid authorization secret length"))
        })?;
        let mut hasher = blake3::Hasher::new_keyed(&key);
        hasher.update(AUTH_CONTEXT);
        for field in [
            self.transfer_id.as_bytes(),
            self.sender_endpoint_id.as_bytes(),
            self.receiver_endpoint_id.as_bytes(),
            self.manifest_id.as_bytes(),
            self.content_hash.as_bytes(),
            self.transfer_name.as_bytes(),
            self.blob_ticket.as_bytes(),
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field);
        }
        hasher.update(&self.protocol_transfer_id.to_le_bytes());
        hasher.update(&self.file_count.to_le_bytes());
        hasher.update(&self.total_size.to_le_bytes());
        hasher.update(&self.protocol_version.to_le_bytes());
        Ok(*hasher.finalize().as_bytes())
    }
}
