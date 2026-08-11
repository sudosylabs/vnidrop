#[path = "tests/access_policy.rs"]
mod access_policy_tests;
#[path = "tests/contact_polling.rs"]
mod contact_polling_tests;
#[path = "tests/contacts.rs"]
mod contacts_tests;
#[path = "tests/control_plane.rs"]
mod control_plane_tests;
#[path = "tests/device_relationship.rs"]
mod device_relationship_tests;
#[path = "tests/error.rs"]
mod error_tests;
#[path = "tests/filesystem.rs"]
mod filesystem_tests;
#[path = "tests/grant.rs"]
mod grant_tests;
#[path = "tests/handshake.rs"]
mod handshake_tests;
#[path = "tests/limits.rs"]
mod limits_tests;
#[path = "tests/network_config.rs"]
mod network_config_tests;
#[path = "tests/pairing_eligibility.rs"]
mod pairing_eligibility_tests;
#[path = "tests/platform_contract_linux.rs"]
mod platform_contract_linux_tests;
#[path = "tests/repository.rs"]
mod repository_tests;
#[path = "tests/runtime.rs"]
mod runtime_tests;
#[path = "tests/secret.rs"]
mod secret_tests;
#[path = "tests/secure_secret_android.rs"]
mod secure_secret_android_tests;
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "tests/secure_secret_apple.rs"]
mod secure_secret_apple_tests;
#[path = "tests/secure_secret_linux.rs"]
mod secure_secret_linux_tests;
#[path = "tests/secure_secret.rs"]
mod secure_secret_tests;
#[cfg(target_os = "windows")]
#[path = "tests/secure_secret_windows.rs"]
mod secure_secret_windows_tests;
#[path = "tests/targeted_transfer.rs"]
mod targeted_transfer_tests;
#[path = "tests/ticket.rs"]
mod ticket_tests;
#[path = "tests/transfer_state.rs"]
mod transfer_state_tests;
