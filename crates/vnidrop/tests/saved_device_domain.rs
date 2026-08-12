mod support;

use support::TestNode;
use vnidrop::{
    saved_device_capabilities, DeviceRelationship, DeviceRelationshipState, SavedDevice,
    SavedDeviceCapabilities, ShareMetadataInput, ShareSource, SourceKind, TargetedTransfer,
    TargetedTransferState, TransferAccessMode, VnidropError,
};

#[test]
fn saved_device_protocols_are_explicitly_versioned() {
    assert_eq!(
        saved_device_capabilities(),
        SavedDeviceCapabilities {
            domain_contract_version: 1,
            relationship_protocol_version: 1,
            targeted_transfer_protocol_version: 3,
        }
    );
}

#[test]
fn saved_devices_relationships_and_targeted_transfers_are_distinct_contracts() {
    let device = SavedDevice {
        endpoint_id: "receiver-endpoint".to_string(),
        local_label: Some("Kitchen tablet".to_string()),
        remote_display_name: Some("Tablet".to_string()),
        created_at: 1_000,
        last_authenticated_at: Some(2_000),
    };
    let relationship = DeviceRelationship {
        remote_endpoint_id: device.endpoint_id.clone(),
        state: DeviceRelationshipState::Saved,
        generation: 4,
        minimum_protocol_version: 1,
        created_at: 1_000,
        updated_at: 2_000,
    };
    let transfer = TargetedTransfer {
        id: "targeted-transfer-id".to_string(),
        sender_endpoint_id: "sender-endpoint".to_string(),
        receiver_endpoint_id: device.endpoint_id.clone(),
        manifest_id: "immutable-manifest-id".to_string(),
        transfer_name: "Holiday photos".to_string(),
        file_count: 2,
        total_size: 42,
        verified_bytes: 0,
        state: TargetedTransferState::AwaitingApproval,
        created_at: 3_000,
        updated_at: 3_000,
    };

    assert_eq!(relationship.remote_endpoint_id, device.endpoint_id);
    assert_eq!(relationship.state, DeviceRelationshipState::Saved);
    assert_eq!(
        transfer.receiver_endpoint_id,
        relationship.remote_endpoint_id
    );
    assert_eq!(transfer.state, TargetedTransferState::AwaitingApproval);
}

#[test]
fn targeted_transfer_transitions_are_validated_by_the_domain() {
    use TargetedTransferState as State;

    let valid = [
        (State::Preparing, State::Offering),
        (State::Offering, State::AwaitingApproval),
        (State::AwaitingApproval, State::Approved),
        (State::AwaitingApproval, State::Declined),
        (State::Approved, State::Connecting),
        (State::Approved, State::Deleted),
        (State::Connecting, State::Transferring),
        (State::Connecting, State::Interrupted),
        (State::Transferring, State::Completed),
        (State::Transferring, State::Interrupted),
        (State::Interrupted, State::Connecting),
        (State::Completed, State::Deleted),
        (State::Declined, State::Deleted),
        (State::Cancelled, State::Deleted),
        (State::Failed, State::Deleted),
    ];
    for (current, next) in valid {
        current
            .validate_transition_to(next)
            .unwrap_or_else(|error| panic!("{current:?} -> {next:?} failed: {error}"));
    }

    let error = State::Completed
        .validate_transition_to(State::Transferring)
        .unwrap_err();
    assert!(matches!(error, VnidropError::InvalidTransition { .. }));
    assert_eq!(
        error.to_string(),
        "invalid targeted transfer transition: completed -> transferring"
    );
}

#[test]
fn saved_device_domain_seam_does_not_change_multi_receiver_shares() {
    let source_dir = tempfile::tempdir().unwrap();
    let first_output = tempfile::tempdir().unwrap();
    let second_output = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("shared.txt");
    std::fs::write(&source_path, b"shared with both receivers").unwrap();
    let sender = TestNode::new();
    let first_receiver = TestNode::new();
    let second_receiver = TestNode::new();
    let share = sender
        .core
        .share_files(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source_path.to_string_lossy().into_owned(),
                display_name: Some("shared.txt".to_string()),
                is_directory: false,
            }],
            ShareMetadataInput {
                transfer_id: 90_001,
                transfer_name: Some("Existing share".to_string()),
                sender_name: Some("Sender".to_string()),
                access_mode: TransferAccessMode::Public,
            },
        )
        .unwrap();

    for (receiver, output) in [
        (&first_receiver, first_output.path()),
        (&second_receiver, second_output.path()),
    ] {
        receiver
            .core
            .receive(
                share.ticket.clone(),
                output.to_string_lossy().into_owned(),
                Some("Receiver".to_string()),
            )
            .unwrap();
        assert_eq!(
            std::fs::read(output.join("shared.txt")).unwrap(),
            b"shared with both receivers"
        );
    }
}
