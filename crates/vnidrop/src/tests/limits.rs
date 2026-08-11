use crate::api::CoreLimits;

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
    assert_eq!(limits.max_saved_devices, 256);
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
