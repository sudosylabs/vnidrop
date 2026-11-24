use super::*;
use crate::{persistence, CoreEvent, CoreEventSink};

struct NoopSink;

impl CoreEventSink for NoopSink {
    fn on_event(&self, _: CoreEvent) {}
}

async fn test_service() -> (Arc<DeviceRelationshipService>, tempfile::TempDir) {
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
    let service = Arc::new(DeviceRelationshipService::new(
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
    ));
    (service, temp)
}

#[tokio::test]
async fn peer_lock_registry_reclaims_inactive_peers_and_preserves_live_locks() {
    let (service, _temp) = test_service().await;
    let active = service.lock_peer("active-peer").await;
    let guard = active.lock().await;
    for index in 0..100 {
        service.lock_peer(&format!("inactive-{index}")).await;
    }
    let same_peer = service.lock_peer("active-peer").await;
    assert!(same_peer.try_lock().is_err());
    assert!(service.peer_locks.lock().await.len() <= 8);
    drop(guard);
    service.endpoint.close().await;
    service.event_hub.shutdown().await;
}

fn relationship(peer: &str, state: DeviceRelationshipState) -> RelationshipUpsert<'_> {
    let now = now_ms();
    RelationshipUpsert {
        remote_endpoint_id: peer,
        state,
        generation: 1,
        minimum_protocol_version: saved_device_capabilities().relationship_protocol_version,
        session_id: Some("original-session"),
        remote_display_name: None,
        last_authenticated_at: None,
        issued_grant_handle: None,
        held_grant_handle: None,
        issued_grant_id: None,
        held_grant_id: None,
        peer_ack: false,
        local_ack: false,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn pairing_cannot_become_saved_without_both_grants() {
    let (service, _temp) = test_service().await;
    let peer = "new-pairing";
    service
        .store
        .upsert(relationship(peer, DeviceRelationshipState::PendingOutgoing))
        .await
        .unwrap();
    assert!(service.activate_saved(peer).await.is_err());
    assert_eq!(
        service.find_row(peer).await.unwrap().unwrap().state,
        DeviceRelationshipState::PendingOutgoing
    );
    service.endpoint.close().await;
    service.event_hub.shutdown().await;
}

#[tokio::test]
async fn pairing_request_without_a_valid_capability_cannot_get_already_saved() {
    let (service, _temp) = test_service().await;
    let peer = "previously-saved";
    service
        .store
        .upsert(relationship(peer, DeviceRelationshipState::Saved))
        .await
        .unwrap();
    let reply = service
        .handle_pairing_request(
            peer.to_owned(),
            PairingRequest {
                session_id: "unverified-session".to_owned(),
                capability: vec![7; 32],
                protocol_version: saved_device_capabilities().relationship_protocol_version,
                generation: 2,
            },
        )
        .await;
    assert!(matches!(reply, PairingRequestResponse::Rejected));
    let existing = service.find_row(peer).await.unwrap().unwrap();
    assert_eq!(existing.generation, 1);
    assert_eq!(existing.state, DeviceRelationshipState::Saved);
    service.endpoint.close().await;
    service.event_hub.shutdown().await;
}
