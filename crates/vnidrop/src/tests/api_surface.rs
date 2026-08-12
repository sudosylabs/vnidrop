//! Public API surface after prototype contact/offer removal.

#[test]
fn public_api_exposes_saved_device_surface_without_prototype_contact_entry_points() {
    let facade = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/runtime/facade.rs"
    ));
    let api = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/api.rs"));
    let lib = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));

    for forbidden in [
        "initialize_with_experimental_saved_devices",
        "experimental_saved_device_capabilities",
        "ExperimentalSavedDeviceCapabilities",
        "fn list_contacts(",
        "fn send_to_contact(",
        "fn poll_contacts_for_offers(",
        "fn offer_transfer_to_contact(",
        "fn list_held_offers(",
        "fn list_pending_offers(",
        "fn respond_to_offer(",
        "fn list_pending_pairings(",
        "fn allow_device_to_reach_me(",
        "fn respond_to_pairing(",
        "fn forget_contact(",
        "fn forget_all_contacts(",
        "fn block_contact(",
        "fn unblock_contact(",
        "fn list_blocked_contacts(",
        "fn set_contact_label(",
        "fn set_grant_lifetime(",
        "struct ContactSummary",
        "struct ContactSendResult",
        "struct HeldOfferSummary",
        "struct IncomingOffer",
        "struct PendingPairing",
        "enum GrantLifetimeSetting",
    ] {
        assert!(
            !facade.contains(forbidden),
            "facade must not expose prototype entry point {forbidden}"
        );
        assert!(
            !api.contains(forbidden),
            "api.rs must not define prototype type {forbidden}"
        );
        assert!(
            !lib.contains(forbidden),
            "lib.rs must not re-export prototype symbol {forbidden}"
        );
    }

    for required in [
        "fn list_saved_devices(",
        "fn list_device_relationships(",
        "fn request_saved_device_pairing(",
        "fn create_targeted_transfer(",
        "fn list_pending_targeted_offers(",
        "fn block_device(",
        "fn forget_saved_device(",
        "fn share_files(",
        "fn receive(",
        "saved_device_capabilities",
    ] {
        assert!(
            facade.contains(required) || api.contains(required) || lib.contains(required),
            "public surface must keep {required}"
        );
    }

    let caps = crate::saved_device_capabilities();
    assert_eq!(caps.domain_contract_version, 1);
    assert_eq!(caps.relationship_protocol_version, 1);
    assert_eq!(caps.targeted_transfer_protocol_version, 3);
}
