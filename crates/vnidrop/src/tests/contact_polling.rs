use crate::runtime::should_poll;

const MINUTE_MS: i64 = 60 * 1_000;
const NOW: i64 = 1_700_000_000_000;

#[test]
fn a_device_never_polled_is_polled() {
    assert!(should_poll(None, NOW));
}

#[test]
fn a_device_polled_recently_is_skipped() {
    // Switching in and out of the app must not re-announce presence.
    assert!(!should_poll(Some(NOW), NOW));
    assert!(!should_poll(Some(NOW - MINUTE_MS), NOW));
    assert!(!should_poll(Some(NOW - 4 * MINUTE_MS), NOW));
}

#[test]
fn a_device_polled_before_the_window_is_polled_again() {
    assert!(should_poll(Some(NOW - 5 * MINUTE_MS), NOW));
    assert!(should_poll(Some(NOW - 60 * MINUTE_MS), NOW));
}

/// A clock that jumped backwards must not lock polling out forever.
#[test]
fn a_future_timestamp_is_treated_as_recent_rather_than_permanent() {
    assert!(!should_poll(Some(NOW + MINUTE_MS), NOW));
    assert!(should_poll(Some(NOW + MINUTE_MS), NOW + 6 * MINUTE_MS));
}
