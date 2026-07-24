use std::net::{Ipv4Addr, SocketAddr};

use data_encoding::BASE64URL_NOPAD;
use iroh::{RelayUrl, SecretKey};
use iroh_blobs::{ticket::BlobTicket, BlobFormat, Hash};
use serde_json::json;

use crate::{
    api::{CoreLimits, CoreRelayMode, TransferMetadata},
    ticket::{
        encode_persisted_sender_address, parse_persisted_sender_address, parse_transfer_ticket,
        parse_transfer_ticket_with_limits, ticket_matches_relay_profile, VnidropTicket,
    },
};

fn blob_ticket(hash_byte: u8) -> BlobTicket {
    let secret = SecretKey::generate();
    let addr = iroh::EndpointAddr::new(secret.public());
    BlobTicket::new(addr, Hash::new([hash_byte; 32]), BlobFormat::HashSeq)
}

#[test]
fn metadata_ticket_round_trips() {
    let blob_ticket = blob_ticket(7);
    let metadata = TransferMetadata::new(
        42,
        "Summer photos",
        Some("hammed".to_string()),
        blob_ticket.hash(),
        3,
        2048,
    );
    let encoded = VnidropTicket::new_with_relay_urls(blob_ticket.clone(), metadata.clone(), &[])
        .encode()
        .unwrap();
    let parsed = parse_transfer_ticket(&encoded).unwrap();

    assert_eq!(parsed.blob_ticket.hash(), blob_ticket.hash());
    assert_eq!(parsed.metadata.transfer_name, metadata.transfer_name);
}

#[test]
fn metadata_ticket_tolerates_wrapped_whitespace() {
    let blob_ticket = blob_ticket(9);
    let metadata = TransferMetadata::new(7, "Wrapped", None, blob_ticket.hash(), 1, 10);
    let encoded = VnidropTicket::new_with_relay_urls(blob_ticket.clone(), metadata, &[])
        .encode()
        .unwrap();
    let wrapped = encoded
        .as_bytes()
        .chunks(8)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("\n  ");

    let parsed = parse_transfer_ticket(&wrapped).unwrap();
    assert_eq!(parsed.blob_ticket.hash(), blob_ticket.hash());
}

#[test]
fn metadata_ticket_restores_backup_relays_without_losing_direct_addresses() {
    let secret = SecretKey::generate();
    let primary: RelayUrl = "https://a.relay.example.com".parse().unwrap();
    let backup: RelayUrl = "https://b.relay.example.com".parse().unwrap();
    let direct = SocketAddr::from((Ipv4Addr::LOCALHOST, 49152));
    let addr = iroh::EndpointAddr::new(secret.public())
        .with_relay_url(primary.clone())
        .with_ip_addr(direct);
    let blob_ticket = BlobTicket::new(addr, Hash::new([11; 32]), BlobFormat::HashSeq);
    let metadata = TransferMetadata::new(11, "Backed up", None, blob_ticket.hash(), 1, 10);

    let encoded = VnidropTicket::new_with_relay_urls(
        blob_ticket,
        metadata,
        &[primary.clone(), backup.clone()],
    )
    .encode()
    .unwrap();
    let parsed = parse_transfer_ticket(&encoded).unwrap();

    assert_eq!(
        parsed
            .blob_ticket
            .addr()
            .relay_urls()
            .cloned()
            .collect::<Vec<_>>(),
        vec![primary, backup]
    );
    assert_eq!(
        parsed
            .blob_ticket
            .addr()
            .ip_addrs()
            .copied()
            .collect::<Vec<_>>(),
        vec![direct]
    );
}

#[test]
fn saved_ticket_relay_profile_matching_is_mode_aware_and_order_insensitive() {
    let relay_a: RelayUrl = "https://a.relay.example.com".parse().unwrap();
    let relay_b: RelayUrl = "https://b.relay.example.com".parse().unwrap();
    let relay_c: RelayUrl = "https://c.relay.example.com".parse().unwrap();
    let blob_ticket = blob_ticket(13);
    let metadata = TransferMetadata::new(13, "Relay profile", None, blob_ticket.hash(), 1, 10);
    let custom_ticket = VnidropTicket::new_with_relay_urls(
        blob_ticket.clone(),
        metadata.clone(),
        &[relay_a.clone(), relay_b.clone()],
    )
    .encode()
    .unwrap();
    let automatic_ticket = VnidropTicket::new_with_relay_urls(blob_ticket, metadata, &[])
        .encode()
        .unwrap();
    let limits = CoreLimits::default();

    assert!(ticket_matches_relay_profile(
        &custom_ticket,
        &limits,
        CoreRelayMode::StrictCustom,
        &[relay_b.clone(), relay_a.clone()],
    )
    .unwrap());
    assert!(ticket_matches_relay_profile(
        &custom_ticket,
        &limits,
        CoreRelayMode::CustomWithDirectFallback,
        &[relay_b.clone(), relay_a.clone()],
    )
    .unwrap());
    assert!(!ticket_matches_relay_profile(
        &custom_ticket,
        &limits,
        CoreRelayMode::StrictCustom,
        &[relay_a.clone(), relay_c],
    )
    .unwrap());
    assert!(
        !ticket_matches_relay_profile(&custom_ticket, &limits, CoreRelayMode::Automatic, &[],)
            .unwrap()
    );
    assert!(ticket_matches_relay_profile(
        &automatic_ticket,
        &limits,
        CoreRelayMode::Automatic,
        &[],
    )
    .unwrap());
    assert!(!ticket_matches_relay_profile(
        &automatic_ticket,
        &limits,
        CoreRelayMode::StrictCustom,
        &[relay_a],
    )
    .unwrap());
    assert!(ticket_matches_relay_profile(
        &automatic_ticket,
        &limits,
        CoreRelayMode::LocalOnly,
        &[],
    )
    .unwrap());
    assert!(
        !ticket_matches_relay_profile(&custom_ticket, &limits, CoreRelayMode::LocalOnly, &[],)
            .unwrap()
    );
}

#[test]
fn persisted_sender_address_preserves_relays_and_accepts_legacy_blob_ticket() {
    let secret = SecretKey::generate();
    let primary: RelayUrl = "https://a.relay.example.com".parse().unwrap();
    let backup: RelayUrl = "https://b.relay.example.com".parse().unwrap();
    let direct = SocketAddr::from((Ipv4Addr::LOCALHOST, 49153));
    let addr = iroh::EndpointAddr::new(secret.public())
        .with_relay_url(primary.clone())
        .with_relay_url(backup)
        .with_ip_addr(direct);

    let encoded = encode_persisted_sender_address(&addr).unwrap();
    assert_eq!(parse_persisted_sender_address(&encoded).unwrap(), addr);

    let legacy_addr = iroh::EndpointAddr::new(secret.public())
        .with_relay_url(primary)
        .with_ip_addr(direct);
    let legacy = BlobTicket::new(
        legacy_addr.clone(),
        Hash::new([12; 32]),
        BlobFormat::HashSeq,
    )
    .to_string();
    assert_eq!(
        parse_persisted_sender_address(&legacy).unwrap(),
        legacy_addr
    );
}

#[test]
fn invalid_ticket_is_rejected() {
    assert!(parse_transfer_ticket("not-a-ticket").is_err());
}

#[test]
fn rejects_raw_blob_tickets() {
    let raw = blob_ticket(3).to_string();
    let error = parse_transfer_ticket(&raw).unwrap_err().to_string();
    assert!(
        error.contains("not a VniDrop ticket"),
        "raw BlobTicket must not be accepted as a transfer invitation: {error}"
    );
}

#[test]
fn rejects_unsupported_versions_and_mismatched_hashes() {
    let blob_ticket = blob_ticket(5);
    let payload = json!({
        "version": 2,
        "blob_ticket": blob_ticket.to_string(),
        "metadata": {
            "version": 1,
            "transfer_id": 7,
            "transfer_name": "bad version",
            "sender_name": null,
            "created_at": 1,
            "content_hash": blob_ticket.hash().to_string(),
            "file_count": 1,
            "total_size": 10
        }
    });
    let encoded = format!(
        "vnd1:{}",
        BASE64URL_NOPAD.encode(payload.to_string().as_bytes())
    );
    assert!(parse_transfer_ticket(&encoded)
        .unwrap_err()
        .to_string()
        .contains("unsupported VniDrop ticket version"));

    let payload = json!({
        "version": 1,
        "blob_ticket": blob_ticket.to_string(),
        "metadata": {
            "version": 1,
            "transfer_id": 7,
            "transfer_name": "bad hash",
            "sender_name": null,
            "created_at": 1,
            "content_hash": Hash::new([6; 32]).to_string(),
            "file_count": 1,
            "total_size": 10
        }
    });
    let encoded = format!(
        "vnd1:{}",
        BASE64URL_NOPAD.encode(payload.to_string().as_bytes())
    );
    assert!(parse_transfer_ticket(&encoded)
        .unwrap_err()
        .to_string()
        .contains("metadata hash does not match"));
}

#[test]
fn rejects_ticket_over_configured_size_limit() {
    let limits = CoreLimits {
        max_ticket_bytes: 8,
        ..CoreLimits::default()
    };
    assert!(parse_transfer_ticket_with_limits("not-a-ticket", &limits).is_err());
}

#[test]
fn parser_rejects_or_parses_generated_inputs_without_panicking() {
    let alphabet = b"vnd1:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_ /\\\n";
    let mut state = 0x9e37_79b9u32;
    for len in 0..512usize {
        let mut input = String::with_capacity(len);
        for _ in 0..len {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            input.push(alphabet[state as usize % alphabet.len()] as char);
        }
        let _ = parse_transfer_ticket(&input);
    }
}
