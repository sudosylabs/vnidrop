use iroh_blobs::format::collection::Collection;

use crate::{api::CoreLimits, runtime::check_collection_metadata_size};

#[test]
fn default_limits_are_valid() {
    CoreLimits::default().validate().unwrap();
}

#[test]
fn default_limits_bound_ticket_and_approval_pressure() {
    let limits = CoreLimits::default();
    assert!(limits.max_ticket_bytes <= 256 * 1024);
    assert!(limits.max_pending_approvals <= 64);
    assert!(limits.max_total_bytes <= 256 * 1024 * 1024 * 1024);
    assert!(limits.max_pending_offers <= 16);
}

#[test]
fn zero_limit_is_rejected() {
    let limits = CoreLimits {
        max_sources: 0,
        ..CoreLimits::default()
    };
    assert!(limits.validate().is_err());
}

#[test]
fn zero_saved_device_limit_is_rejected() {
    let limits = CoreLimits {
        max_saved_devices: 0,
        ..CoreLimits::default()
    };
    assert!(limits.validate().is_err());
}

#[test]
fn collection_metadata_limit_accounts_for_wire_overhead() {
    let path = "x".repeat(128);
    let collection: Collection = [(path, iroh_blobs::Hash::new(b"data"))]
        .into_iter()
        .collect();
    let metadata_bytes = collection.to_blobs().next().unwrap().len() as u64;
    assert!(metadata_bytes > 128);
    assert_eq!(
        check_collection_metadata_size(&[metadata_bytes, 4], 128).unwrap(),
        metadata_bytes
    );
    assert!(check_collection_metadata_size(&[10_000, 4], 128).is_err());
    assert!(check_collection_metadata_size(&[], 128).is_err());
}
