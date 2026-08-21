use std::io;

use crate::VnidropError;

#[test]
fn transfer_boundary_classifies_operating_system_failures() {
    let already_exists = VnidropError::transfer(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "target exists",
    ));
    let permission = VnidropError::transfer(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "access denied",
    ));
    let storage = VnidropError::transfer(io::Error::new(io::ErrorKind::StorageFull, "disk full"));
    let filesystem =
        VnidropError::transfer(io::Error::new(io::ErrorKind::NotFound, "folder missing"));

    assert!(matches!(
        already_exists,
        VnidropError::DestinationExists { .. }
    ));
    assert!(matches!(
        permission,
        VnidropError::FilesystemPermission { .. }
    ));
    assert!(matches!(storage, VnidropError::StorageFull { .. }));
    assert!(matches!(filesystem, VnidropError::Filesystem { .. }));
}

#[test]
fn transfer_boundary_preserves_typed_errors_through_context() {
    let error = anyhow::Error::new(VnidropError::Network {
        reason: "connection reset".to_string(),
    })
    .context("download failed");

    let classified = VnidropError::transfer(error);

    assert!(matches!(
        classified,
        VnidropError::Network { ref reason } if reason == "download failed"
    ));
    assert_eq!(classified.code(), "network");
}

#[test]
fn initialization_boundary_preserves_secure_storage_failures() {
    let error = anyhow::Error::new(VnidropError::SecureStorageLocked {
        reason: "credential store is locked".to_string(),
    })
    .context("endpoint identity could not be loaded");

    let classified = VnidropError::initialization(error);

    assert!(matches!(
        classified,
        VnidropError::SecureStorageLocked { ref reason }
            if reason == "endpoint identity could not be loaded"
    ));
}

#[test]
fn transfer_boundary_classifies_database_failures() {
    let transfer = VnidropError::transfer(sqlx::Error::RowNotFound);
    let approval = VnidropError::permission(sqlx::Error::RowNotFound);

    assert!(matches!(transfer, VnidropError::Repository { .. }));
    assert!(matches!(approval, VnidropError::Repository { .. }));
}

#[test]
fn saved_device_failures_remain_distinguishable() {
    let unavailable = VnidropError::device_unavailable(anyhow::anyhow!("offline"));
    let timeout = VnidropError::offer_timeout(anyhow::anyhow!("no answer"));
    let relay = VnidropError::relay_policy_incompatible(anyhow::anyhow!("profiles differ"));
    let protocol = VnidropError::protocol_incompatible(anyhow::anyhow!("downgrade"));

    assert_eq!(unavailable.code(), "device_unavailable");
    assert_eq!(timeout.code(), "offer_timeout");
    assert_eq!(relay.code(), "relay_policy_incompatible");
    assert_eq!(protocol.code(), "protocol_incompatible");
    assert!(matches!(
        unavailable,
        VnidropError::DeviceUnavailable { .. }
    ));
    assert!(matches!(timeout, VnidropError::OfferTimeout { .. }));
    assert!(matches!(
        relay,
        VnidropError::RelayPolicyIncompatible { .. }
    ));
    assert!(matches!(
        protocol,
        VnidropError::ProtocolIncompatible { .. }
    ));
}
