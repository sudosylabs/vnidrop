use super::*;
use crate::{persistence, CoreEvent, CoreEventSink};

struct NoopSink;

impl CoreEventSink for NoopSink {
    fn on_event(&self, _: CoreEvent) {}
}

#[tokio::test]
async fn peer_lock_registry_reclaims_inactive_peers_and_preserves_live_locks() {
    let temp = tempfile::tempdir().unwrap();
    let stores = persistence::open_all(temp.path()).await.unwrap();
    let endpoint = Endpoint::builder(iroh::endpoint::presets::Minimal)
        .bind()
        .await
        .unwrap();
    let endpoint_id = endpoint.id().to_string();
    let hub = Arc::new(EventHub::start(stores.invitation, Arc::new(NoopSink), 8, 8));
    let eligibility = PairingEligibilityService::new(
        stores.eligibility,
        stores.relationships.clone(),
        None,
        hub.clone(),
        endpoint_id.clone(),
    );
    let service = DeviceRelationshipService::new(
        stores.relationships,
        stores.blocked,
        None,
        eligibility,
        hub.clone(),
        endpoint_id,
        endpoint.clone(),
        CoreRelayMode::LocalOnly,
        Vec::new(),
        4,
        1000,
    );

    let active = service.lock_peer("active-peer").await;
    let guard = active.lock().await;
    for index in 0..100 {
        service.lock_peer(&format!("inactive-{index}")).await;
    }
    let same_peer = service.lock_peer("active-peer").await;
    assert!(same_peer.try_lock().is_err());
    assert!(service.peer_locks.lock().await.len() <= 8);
    drop(guard);
    endpoint.close().await;
    hub.shutdown().await;
}
