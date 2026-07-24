mod support;

use std::str::FromStr;

use data_encoding::BASE64URL_NOPAD;
use iroh::{EndpointAddr, RelayUrl};
use iroh_blobs::ticket::BlobTicket;
use serde_json::Value;
use support::{receive_with_response, share_path, TestNode, TestRelay};
use vnidrop::{CoreNetworkConfig, CoreRelayMode};

fn custom_config(relay_urls: &[&str]) -> CoreNetworkConfig {
    CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: relay_urls.iter().map(ToString::to_string).collect(),
    }
}

fn local_only_config() -> CoreNetworkConfig {
    CoreNetworkConfig {
        mode: CoreRelayMode::LocalOnly,
        relay_urls: Vec::new(),
    }
}

fn read_blob_ticket(ticket: &str) -> (Value, BlobTicket) {
    let encoded = ticket.strip_prefix("vnd1:").unwrap();
    let payload = BASE64URL_NOPAD.decode(encoded.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(&payload).unwrap();
    let blob_ticket = BlobTicket::from_str(value["blob_ticket"].as_str().unwrap()).unwrap();
    let (mut addr, hash, format) = blob_ticket.into_parts();
    if let Some(relay_urls) = value["relay_urls"].as_array() {
        for relay_url in relay_urls {
            addr = addr.with_relay_url(relay_url.as_str().unwrap().parse().unwrap());
        }
    }
    let blob_ticket = BlobTicket::new(addr, hash, format);
    (value, blob_ticket)
}

#[test]
fn local_only_mode_advertises_direct_addresses_and_transfers_on_lan() {
    let sender = TestNode::with_network_config(local_only_config());
    let receiver = TestNode::with_network_config(local_only_config());
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("local-only.txt");
    std::fs::write(&source_path, b"direct on the local network").unwrap();

    let share = share_path(&sender.core, &source_path, 402, "local-only.txt", false);
    let (ticket_value, blob_ticket) = read_blob_ticket(&share.ticket);
    assert!(ticket_value.get("relay_urls").is_none());
    assert_eq!(blob_ticket.addr().relay_urls().count(), 0);
    assert!(!sender.core.status().addr.contains("iroh.link"));

    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        share.ticket,
        output_dir.path(),
        true,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(output_dir.path().join("local-only.txt")).unwrap(),
        b"direct on the local network"
    );
}

fn with_relay_only_address(ticket: &str, relay_url: &str) -> String {
    let (mut value, blob_ticket) = read_blob_ticket(ticket);
    let relay_url: RelayUrl = relay_url.parse().unwrap();
    let relay_only_addr = EndpointAddr::new(blob_ticket.addr().id).with_relay_url(relay_url);
    let relay_only_ticket =
        BlobTicket::new(relay_only_addr, blob_ticket.hash(), blob_ticket.format());
    value["blob_ticket"] = Value::String(relay_only_ticket.to_string());
    let payload = serde_json::to_vec(&value).unwrap();
    format!("vnd1:{}", BASE64URL_NOPAD.encode(&payload))
}

#[test]
fn strict_custom_relay_is_advertised_and_transfers_without_direct_ticket_addresses() {
    let relay = TestRelay::start();
    let backup_relay = "http://127.0.0.1:9";
    let relay_urls = [relay.url.as_str(), backup_relay];
    let sender = TestNode::with_network_config(custom_config(&relay_urls));
    let receiver = TestNode::with_network_config(custom_config(&[relay.url.as_str()]));
    let source_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("custom-relay.txt");
    std::fs::write(&source_path, b"through the custom relay").unwrap();

    let share = share_path(&sender.core, &source_path, 401, "custom-relay.txt", false);
    let (ticket_value, blob_ticket) = read_blob_ticket(&share.ticket);
    let advertised_relays: Vec<_> = blob_ticket
        .addr()
        .relay_urls()
        .map(ToString::to_string)
        .collect();
    let configured_relay = RelayUrl::from_str(&relay.url).unwrap().to_string();
    let configured_backup = RelayUrl::from_str(backup_relay).unwrap().to_string();
    let envelope_relays = ticket_value["relay_urls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        envelope_relays,
        vec![configured_relay.clone(), configured_backup.clone()]
    );
    let mut configured_relays = vec![configured_relay.clone(), configured_backup];
    configured_relays.sort();
    assert_eq!(advertised_relays, configured_relays.clone());
    assert!(!advertised_relays.iter().any(|url| url.contains("n0")));
    assert!(!sender.core.status().addr.contains("iroh.link"));

    let relay_only_ticket = with_relay_only_address(&share.ticket, &relay.url);
    let (_, relay_only_blob_ticket) = read_blob_ticket(&relay_only_ticket);
    assert_eq!(relay_only_blob_ticket.addr().ip_addrs().count(), 0);
    assert_eq!(
        relay_only_blob_ticket
            .addr()
            .relay_urls()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        configured_relays
    );

    receive_with_response(
        &sender.core,
        share.transfer_id,
        receiver.core.arc(),
        relay_only_ticket,
        output_dir.path(),
        true,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(output_dir.path().join("custom-relay.txt")).unwrap(),
        b"through the custom relay"
    );
}
