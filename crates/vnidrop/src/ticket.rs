use std::{collections::BTreeSet, str::FromStr};

use anyhow::{Context, Result};
use data_encoding::BASE64URL_NOPAD;
use iroh::{EndpointAddr, RelayUrl};
use iroh_blobs::ticket::BlobTicket;
use serde::{Deserialize, Serialize};

use crate::api::{CoreLimits, CoreNetworkConfig, CoreRelayMode, TransferMetadata};

const VNIDROP_TICKET_PREFIX: &str = "vnd1:";
const VNIDROP_TICKET_VERSION: u8 = 1;
const PERSISTED_SENDER_ADDRESS_PREFIX: &str = "vndaddr1:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VnidropTicket {
    version: u8,
    blob_ticket: String,
    // BlobTicket's current wire format retains only one relay URL. The outer
    // envelope carries backups so new receivers can rebuild the full address.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relay_urls: Vec<String>,
    metadata: TransferMetadata,
}

impl VnidropTicket {
    pub(crate) fn new_with_relay_urls(
        blob_ticket: BlobTicket,
        metadata: TransferMetadata,
        relay_urls: &[RelayUrl],
    ) -> Self {
        Self {
            version: VNIDROP_TICKET_VERSION,
            blob_ticket: blob_ticket.to_string(),
            relay_urls: relay_urls.iter().map(ToString::to_string).collect(),
            metadata,
        }
    }

    pub(crate) fn encode(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self)?;
        Ok(format!(
            "{VNIDROP_TICKET_PREFIX}{}",
            BASE64URL_NOPAD.encode(&bytes)
        ))
    }

    fn decode(value: &str) -> Result<Self> {
        let normalized = normalize_ticket_input(value);
        let encoded = normalized
            .strip_prefix(VNIDROP_TICKET_PREFIX)
            .context("not a VniDrop ticket")?;
        let bytes = BASE64URL_NOPAD
            .decode(encoded.as_bytes())
            .context("invalid VniDrop ticket encoding")?;
        serde_json::from_slice(&bytes).context("invalid VniDrop ticket payload")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedTransferTicket {
    pub(crate) blob_ticket: BlobTicket,
    pub(crate) metadata: TransferMetadata,
    pub(crate) advertised_custom_relay_urls: Vec<RelayUrl>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSenderAddress {
    addr: EndpointAddr,
}

pub(crate) fn encode_persisted_sender_address(addr: &EndpointAddr) -> Result<String> {
    let bytes = serde_json::to_vec(&PersistedSenderAddress { addr: addr.clone() })?;
    Ok(format!(
        "{PERSISTED_SENDER_ADDRESS_PREFIX}{}",
        BASE64URL_NOPAD.encode(&bytes)
    ))
}

pub(crate) fn parse_persisted_sender_address(value: &str) -> Result<EndpointAddr> {
    if let Some(encoded) = value.strip_prefix(PERSISTED_SENDER_ADDRESS_PREFIX) {
        let bytes = BASE64URL_NOPAD
            .decode(encoded.as_bytes())
            .context("invalid persisted sender address encoding")?;
        let persisted: PersistedSenderAddress =
            serde_json::from_slice(&bytes).context("invalid persisted sender address payload")?;
        return Ok(persisted.addr);
    }

    // Rows created before multi-relay invitations stored a raw BlobTicket.
    let legacy = BlobTicket::from_str(value).context("invalid legacy sender BlobTicket")?;
    Ok(legacy.addr().clone())
}

#[cfg(test)]
pub(crate) fn parse_transfer_ticket(value: &str) -> Result<ParsedTransferTicket> {
    parse_transfer_ticket_with_limits(value, &CoreLimits::default())
}

pub(crate) fn parse_transfer_ticket_with_limits(
    value: &str,
    limits: &CoreLimits,
) -> Result<ParsedTransferTicket> {
    if value.len() as u64 > limits.max_ticket_bytes {
        anyhow::bail!(
            "ticket is {} bytes, limit is {}",
            value.len(),
            limits.max_ticket_bytes
        );
    }
    let normalized = normalize_ticket_input(value);
    if !normalized.starts_with(VNIDROP_TICKET_PREFIX) {
        anyhow::bail!("not a VniDrop ticket; expected a vnd1: invitation");
    }
    let ticket = VnidropTicket::decode(&normalized)?;
    if ticket.version != VNIDROP_TICKET_VERSION {
        anyhow::bail!("unsupported VniDrop ticket version {}", ticket.version);
    }
    if ticket.metadata.version != VNIDROP_TICKET_VERSION {
        anyhow::bail!(
            "unsupported VniDrop metadata version {}",
            ticket.metadata.version
        );
    }
    if ticket.metadata.transfer_id == 0 {
        anyhow::bail!("VniDrop ticket metadata is missing a valid transfer id");
    }
    if ticket.metadata.transfer_name.trim().is_empty() {
        anyhow::bail!("VniDrop ticket metadata is missing a transfer name");
    }
    limits.validate_metadata_text(
        "transfer name",
        Some(ticket.metadata.transfer_name.as_str()),
    )?;
    limits.validate_metadata_text("sender name", ticket.metadata.sender_name.as_deref())?;
    let mut blob_ticket = BlobTicket::from_str(&ticket.blob_ticket)
        .context("invalid BlobTicket inside VniDrop ticket")?;
    if ticket.metadata.content_hash != blob_ticket.hash().to_string() {
        anyhow::bail!("VniDrop ticket metadata hash does not match BlobTicket hash");
    }
    let advertised_custom_relay_urls = if ticket.relay_urls.is_empty() {
        Vec::new()
    } else {
        CoreNetworkConfig {
            mode: CoreRelayMode::StrictCustom,
            relay_urls: ticket.relay_urls,
        }
        .validated_relay_urls()
        .context("invalid relay URLs inside VniDrop ticket")?
    };
    if !advertised_custom_relay_urls.is_empty() {
        let (mut addr, hash, format) = blob_ticket.into_parts();
        for relay_url in advertised_custom_relay_urls.iter().cloned() {
            addr = addr.with_relay_url(relay_url);
        }
        blob_ticket = BlobTicket::new(addr, hash, format);
    }
    Ok(ParsedTransferTicket {
        blob_ticket,
        metadata: ticket.metadata,
        advertised_custom_relay_urls,
    })
}

pub(crate) fn ticket_matches_relay_profile(
    value: &str,
    limits: &CoreLimits,
    relay_mode: CoreRelayMode,
    custom_relay_urls: &[RelayUrl],
) -> Result<bool> {
    let parsed = parse_transfer_ticket_with_limits(value, limits)?;
    match relay_mode {
        CoreRelayMode::Automatic => Ok(parsed.advertised_custom_relay_urls.is_empty()),
        CoreRelayMode::StrictCustom | CoreRelayMode::CustomWithDirectFallback => {
            let advertised = parsed
                .advertised_custom_relay_urls
                .into_iter()
                .collect::<BTreeSet<_>>();
            let configured = custom_relay_urls.iter().cloned().collect::<BTreeSet<_>>();
            Ok(advertised == configured)
        }
        CoreRelayMode::LocalOnly => Ok(parsed.advertised_custom_relay_urls.is_empty()
            && parsed.blob_ticket.addr().relay_urls().next().is_none()),
    }
}

fn normalize_ticket_input(value: &str) -> String {
    // Tickets are commonly copied from text views or chat apps that insert line
    // breaks.  Strip whitespace only; other corrupt characters should still be
    // rejected by the base64 decoder.
    value.chars().filter(|char| !char.is_whitespace()).collect()
}
