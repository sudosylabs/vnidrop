use vnidrop::VnidropError;

pub type Result<T> = std::result::Result<T, &'static str>;

pub fn message_key(error: VnidropError) -> &'static str {
    match error {
        VnidropError::Initialization { .. } => "error_initialization",
        VnidropError::Ticket { .. } => "error_invalid_ticket",
        VnidropError::Filesystem { .. } | VnidropError::FilesystemPermission { .. } => {
            "error_filesystem"
        }
        VnidropError::DestinationExists { .. } => "error_destination_exists",
        VnidropError::StorageFull { .. } => "error_storage_full",
        VnidropError::Network { .. }
        | VnidropError::DeviceUnavailable { .. }
        | VnidropError::OfferTimeout { .. } => "error_network",
        VnidropError::RelayPolicyIncompatible { .. }
        | VnidropError::ProtocolIncompatible { .. } => "error_transfer",
        VnidropError::Transfer { .. } => "error_transfer",
        VnidropError::Permission { .. } => "error_permission",
        VnidropError::Repository { .. } => "error_repository",
        VnidropError::Cancelled { .. } => "progress_cancelled",
        VnidropError::InvalidInput { .. } | VnidropError::InvalidTransition { .. } => {
            "error_invalid_input"
        }
        VnidropError::SecureStorageLocked { .. } => "linux_keyring_locked",
        VnidropError::SecureStorageMissing { .. } | VnidropError::SecureStorageCorrupted { .. } => {
            "linux_identity_unavailable"
        }
        VnidropError::SecureStorageUnavailable { .. } => "linux_keyring_unavailable",
        VnidropError::Internal { .. } => "error_generic",
    }
}
