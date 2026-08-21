//! Public Saved-device API surface.

#[test]
fn public_api_exposes_saved_device_surface() {
    let facade = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/runtime/facade.rs"
    ));
    let api = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/api.rs"));
    let lib = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));

    for required in [
        "fn list_saved_devices(",
        "fn list_device_relationships(",
        "fn request_saved_device_pairing(",
        "fn new_targeted_transfer_preparation(",
        "fn list_pending_targeted_offers(",
        "fn block_device(",
        "fn forget_saved_device(",
        "fn share_files(",
        "fn receive(",
        "saved_device_capabilities",
        "fn reset_unrecoverable_identity_with_limits_and_network_config(",
    ] {
        assert!(
            facade.contains(required) || api.contains(required) || lib.contains(required),
            "public surface must keep {required}"
        );
    }
}
