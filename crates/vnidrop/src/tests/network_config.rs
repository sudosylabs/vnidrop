use std::time::{Duration, Instant};

use iroh::{endpoint::presets, Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey};

use crate::{
    api::{
        default_core_network_config, CoreNetworkConfig, CoreRelayMode, MAX_CUSTOM_RELAYS,
        MAX_RELAY_URL_BYTES,
    },
    runtime::{filter_peer_addr_for_relay_mode, wait_for_relay, RelayStatus},
};

#[test]
fn default_network_config_uses_automatic_relays() {
    assert_eq!(
        default_core_network_config(),
        CoreNetworkConfig {
            mode: CoreRelayMode::Automatic,
            relay_urls: Vec::new(),
        }
    );
    default_core_network_config()
        .validated_relay_urls()
        .unwrap();
}

#[test]
fn relay_mode_and_url_list_must_be_consistent() {
    let automatic_with_url = CoreNetworkConfig {
        mode: CoreRelayMode::Automatic,
        relay_urls: vec!["https://relay.example.com".to_string()],
    };
    assert!(automatic_with_url.validated_relay_urls().is_err());

    for mode in [
        CoreRelayMode::StrictCustom,
        CoreRelayMode::CustomWithDirectFallback,
    ] {
        let custom_without_url = CoreNetworkConfig {
            mode,
            relay_urls: Vec::new(),
        };
        assert!(custom_without_url.validated_relay_urls().is_err());
    }

    let local_only_with_url = CoreNetworkConfig {
        mode: CoreRelayMode::LocalOnly,
        relay_urls: vec!["https://relay.example.com".to_string()],
    };
    assert!(local_only_with_url.validated_relay_urls().is_err());
}

#[test]
fn custom_relay_urls_allow_https_and_loopback_http() {
    let config = CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: vec![
            "https://relay.example.com".to_string(),
            "http://localhost:3340".to_string(),
            "http://127.0.0.1:3341".to_string(),
            "http://[::1]:3342".to_string(),
        ],
    };

    assert_eq!(config.validated_relay_urls().unwrap().len(), 4);
}

#[test]
fn custom_relay_urls_reject_unsafe_or_ambiguous_values() {
    for value in [
        "http://relay.example.com",
        "https://user:password@relay.example.com",
        "https://relay.example.com/path",
        "https://relay.example.com?token=secret",
        "https://relay.example.com#fragment",
        "https://relay.example.com:0",
        "https://relay.exa\tmple.com",
        "https://@relay.example.com",
        " https://relay.example.com",
    ] {
        let config = CoreNetworkConfig {
            mode: CoreRelayMode::StrictCustom,
            relay_urls: vec![value.to_string()],
        };
        assert!(
            config.validated_relay_urls().is_err(),
            "{value} should be rejected"
        );
    }
}

#[test]
fn custom_relay_urls_are_bounded_and_unique_after_normalization() {
    let duplicates = CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: vec![
            "https://relay.example.com".to_string(),
            "https://relay.example.com/".to_string(),
        ],
    };
    assert!(duplicates.validated_relay_urls().is_err());

    let too_many = CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: (0..=MAX_CUSTOM_RELAYS)
            .map(|index| format!("https://relay-{index}.example.com"))
            .collect(),
    };
    assert!(too_many.validated_relay_urls().is_err());

    let too_long = CoreNetworkConfig {
        mode: CoreRelayMode::StrictCustom,
        relay_urls: vec![format!(
            "https://{}.example.com",
            "a".repeat(MAX_RELAY_URL_BYTES)
        )],
    };
    assert!(too_long.validated_relay_urls().is_err());
}

#[test]
fn strict_custom_mode_filters_peer_relays_but_retains_direct_addresses() {
    let allowed: RelayUrl = "https://allowed.relay.example.com".parse().unwrap();
    let disallowed: RelayUrl = "https://disallowed.relay.example.com".parse().unwrap();
    let direct = "192.0.2.1:4433".parse().unwrap();
    let addr = EndpointAddr::new(SecretKey::generate().public())
        .with_relay_url(allowed.clone())
        .with_relay_url(disallowed.clone())
        .with_ip_addr(direct);

    let filtered = filter_peer_addr_for_relay_mode(
        &addr,
        CoreRelayMode::StrictCustom,
        std::slice::from_ref(&allowed),
    )
    .unwrap();
    assert_eq!(
        filtered.relay_urls().cloned().collect::<Vec<_>>(),
        vec![allowed.clone()]
    );
    assert_eq!(
        filtered.ip_addrs().copied().collect::<Vec<_>>(),
        vec![direct]
    );
    assert_eq!(
        filter_peer_addr_for_relay_mode(&addr, CoreRelayMode::Automatic, &[]).unwrap(),
        addr
    );

    let disallowed_only =
        EndpointAddr::new(SecretKey::generate().public()).with_relay_url(disallowed);
    assert!(filter_peer_addr_for_relay_mode(
        &disallowed_only,
        CoreRelayMode::StrictCustom,
        std::slice::from_ref(&allowed),
    )
    .is_err());

    let fallback_filtered = filter_peer_addr_for_relay_mode(
        &addr,
        CoreRelayMode::CustomWithDirectFallback,
        std::slice::from_ref(&allowed),
    )
    .unwrap();
    assert_eq!(fallback_filtered, filtered);

    let local_only = filter_peer_addr_for_relay_mode(&addr, CoreRelayMode::LocalOnly, &[]).unwrap();
    assert_eq!(local_only.relay_urls().count(), 0);
    assert_eq!(
        local_only.ip_addrs().copied().collect::<Vec<_>>(),
        vec![direct]
    );
    assert!(
        filter_peer_addr_for_relay_mode(&disallowed_only, CoreRelayMode::LocalOnly, &[]).is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_wait_enforces_only_strict_custom_mode() {
    let relay_url: RelayUrl = "http://127.0.0.1:9".parse().unwrap();
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::custom([relay_url.clone()]))
        .bind()
        .await
        .unwrap();
    let started = Instant::now();

    let error = wait_for_relay(
        &endpoint,
        CoreRelayMode::StrictCustom,
        std::slice::from_ref(&relay_url),
        Duration::from_millis(50),
    )
    .await
    .unwrap_err();

    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(error.to_string().contains(relay_url.as_str()));
    assert!(error.to_string().contains("verify the URLs"));

    let automatic_status = wait_for_relay(
        &endpoint,
        CoreRelayMode::Automatic,
        &[],
        Duration::from_millis(50),
    )
    .await
    .unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(automatic_status, RelayStatus::Unreachable);

    let fallback_status = wait_for_relay(
        &endpoint,
        CoreRelayMode::CustomWithDirectFallback,
        std::slice::from_ref(&relay_url),
        Duration::from_millis(50),
    )
    .await
    .unwrap();
    assert_eq!(fallback_status, RelayStatus::Unreachable);

    let local_only_status = wait_for_relay(
        &endpoint,
        CoreRelayMode::LocalOnly,
        &[],
        Duration::from_millis(50),
    )
    .await
    .unwrap();
    assert_eq!(local_only_status, RelayStatus::Disabled);
    endpoint.close().await;
}
