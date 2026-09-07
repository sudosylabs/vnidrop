use super::*;

#[test]
fn consent_state_overrides_eligibility_and_names_never_fall_back_to_identity() {
    let eligibility = PairingEligibilitySummary {
        peer_endpoint_id: "peer".into(),
        remote_display_name: Some(" Remote ".into()),
        session_id: "session".into(),
        protocol_version: 2,
        created_at: 1,
        expires_at: 100,
    };
    let mut data = Devices {
        eligible: vec![eligibility],
        ..Default::default()
    };
    assert!(data.list(100).is_empty());
    assert_eq!(data.list(2)[0].name.as_deref(), Some("Remote"));
    data.relationships.push(DeviceRelationship {
        remote_endpoint_id: "peer".into(),
        state: DeviceRelationshipState::PendingOutgoing,
        generation: 3,
        minimum_protocol_version: 2,
        created_at: 1,
        updated_at: 2,
    });
    assert_eq!(
        data.list(2)[0].state,
        DeviceState::Outgoing { generation: 3 }
    );
    assert!(!data.list(2)[0].needs_attention());
    data.relationships[0].state = DeviceRelationshipState::PendingIncoming;
    assert!(data.list(2)[0].needs_attention());
    data.relationships[0].state = DeviceRelationshipState::Saved;
    data.saved.push(SavedDevice {
        endpoint_id: "peer".into(),
        local_label: Some(" My laptop ".into()),
        remote_display_name: Some("Remote".into()),
        created_at: 1,
        last_authenticated_at: None,
    });
    assert_eq!(data.list(2)[0].name.as_deref(), Some("My laptop"));
    data.saved[0].local_label = Some(" ".into());
    assert_eq!(data.list(2)[0].name.as_deref(), Some("Remote"));
    data.blocked.push("peer".into());
    assert_eq!(data.list(2)[0].state, DeviceState::Blocked);
    data.saved.clear();
    data.eligible.clear();
    assert_eq!(data.list(2)[0].name, None);
    data.blocked.clear();
    data.relationships[0].state = DeviceRelationshipState::Revoked;
    assert!(data.list(2).is_empty());
}
