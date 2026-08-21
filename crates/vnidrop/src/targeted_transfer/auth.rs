//! Bound post-approval authorization for a single targeted transfer.
//!
//! The pre-approval offer never carries this material. After explicit approval,
//! the sender issues a capability whose MAC binds the exact recipient, sender,
//! transfer, manifest, hashes, sizes, and protocol generation. Tampering with
//! the receiver identity invalidates the MAC; presenting an intact capability
//! from another endpoint still fails provider ACL and local identity checks.

use std::fmt;

use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
use serde::{Deserialize, Serialize};

use crate::{error::VnidropError, secure_secret::SecretMaterial};

const AUTH_CONTEXT: &[u8] = b"vnidrop-targeted-auth-v1";

#[derive(Clone, Serialize, Deserialize)]
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

#[derive(Clone)]
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

macro_rules! redacted_debug {
    ($($type:ty),+ $(,)?) => {
        $(
            impl fmt::Debug for $type {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str(concat!(stringify!($type), "(redacted)"))
                }
            }
        )+
    };
}

redacted_debug!(TargetedAuthorization, TargetedAuthorizationDraft);

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

    pub(crate) fn secret_bytes(&self) -> Result<[u8; 32], VnidropError> {
        let secret_bytes = HEXLOWER.decode(self.auth_secret.as_bytes()).map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid authorization secret"))
        })?;
        secret_bytes.try_into().map_err(|_| {
            VnidropError::invalid_input(anyhow::anyhow!("invalid authorization secret length"))
        })
    }
}

pub(crate) fn auth_secret_material(
    auth: &TargetedAuthorization,
) -> Result<SecretMaterial, VnidropError> {
    SecretMaterial::new(auth.secret_bytes()?.to_vec())
}

/// Rebuild a bound authorization from durable row fields + custody secret.
pub(crate) fn reconstruct_authorization(
    draft: TargetedAuthorizationDraft,
    secret_material: &SecretMaterial,
) -> Result<TargetedAuthorization, VnidropError> {
    let auth_secret = HEXLOWER.encode(secret_material.as_bytes());
    let mut auth = TargetedAuthorization {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targeted_authorization_debug_is_redacted() {
        let draft = TargetedAuthorizationDraft {
            transfer_id: "transfer-id".to_string(),
            protocol_transfer_id: 1,
            sender_endpoint_id: "sender-endpoint".to_string(),
            receiver_endpoint_id: "receiver-endpoint".to_string(),
            manifest_id: "manifest-id".to_string(),
            content_hash: "content-hash".to_string(),
            file_count: 1,
            total_size: 1,
            protocol_version: 1,
            transfer_name: "private-name".to_string(),
            blob_ticket: "private-ticket".to_string(),
        };
        assert_eq!(format!("{draft:?}"), "TargetedAuthorizationDraft(redacted)");

        let authorization = TargetedAuthorization::issue(draft).unwrap();
        assert_eq!(
            format!("{authorization:?}"),
            "TargetedAuthorization(redacted)"
        );
    }
}
