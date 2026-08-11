use std::io;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum VnidropError {
    #[error("initialization failed: {reason}")]
    Initialization { reason: String },
    #[error("ticket error: {reason}")]
    Ticket { reason: String },
    #[error("filesystem error: {reason}")]
    Filesystem { reason: String },
    #[error("filesystem permission denied: {reason}")]
    FilesystemPermission { reason: String },
    #[error("destination already exists: {reason}")]
    DestinationExists { reason: String },
    #[error("storage is full: {reason}")]
    StorageFull { reason: String },
    #[error("network error: {reason}")]
    Network { reason: String },
    #[error("device unavailable: {reason}")]
    DeviceUnavailable { reason: String },
    #[error("offer timed out: {reason}")]
    OfferTimeout { reason: String },
    #[error("relay policy incompatible: {reason}")]
    RelayPolicyIncompatible { reason: String },
    #[error("protocol incompatible: {reason}")]
    ProtocolIncompatible { reason: String },
    #[error("transfer error: {reason}")]
    Transfer { reason: String },
    #[error("permission error: {reason}")]
    Permission { reason: String },
    #[error("repository error: {reason}")]
    Repository { reason: String },
    #[error("operation cancelled: {reason}")]
    Cancelled { reason: String },
    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },
    #[error("invalid targeted transfer transition: {reason}")]
    InvalidTransition { reason: String },
    #[error("secure storage is locked: {reason}")]
    SecureStorageLocked { reason: String },
    #[error("secure storage item is missing: {reason}")]
    SecureStorageMissing { reason: String },
    #[error("secure storage item is corrupted: {reason}")]
    SecureStorageCorrupted { reason: String },
    #[error("secure storage is unavailable: {reason}")]
    SecureStorageUnavailable { reason: String },
    #[error("internal error: {reason}")]
    Internal { reason: String },
}

impl VnidropError {
    pub(crate) fn initialization(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::Initialization { reason })
    }

    pub(crate) fn ticket(error: impl Into<anyhow::Error>) -> Self {
        Self::Ticket {
            reason: error.into().to_string(),
        }
    }

    pub(crate) fn filesystem(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        Self::classify(error, |reason| Self::Filesystem { reason })
    }

    pub(crate) fn network(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::Network { reason })
    }

    pub(crate) fn device_unavailable(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::DeviceUnavailable { reason })
    }

    pub(crate) fn offer_timeout(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::OfferTimeout { reason })
    }

    pub(crate) fn relay_policy_incompatible(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::RelayPolicyIncompatible {
            reason,
        })
    }

    pub(crate) fn protocol_incompatible(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::ProtocolIncompatible { reason })
    }

    pub(crate) fn transfer(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        Self::classify(error, |reason| Self::Transfer { reason })
    }

    pub(crate) fn permission(error: impl Into<anyhow::Error>) -> Self {
        Self::classify(error.into(), |reason| Self::Permission { reason })
    }

    pub(crate) fn repository(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::Repository { reason })
    }

    pub(crate) fn cancelled(reason: impl Into<String>) -> Self {
        Self::Cancelled {
            reason: reason.into(),
        }
    }

    pub(crate) fn invalid_input(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::InvalidInput { reason })
    }

    pub(crate) fn internal(error: impl Into<anyhow::Error>) -> Self {
        Self::from_error(error.into(), |reason| Self::Internal { reason })
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Initialization { .. } => "initialization",
            Self::Ticket { .. } => "invalid_ticket",
            Self::Filesystem { .. } => "filesystem",
            Self::FilesystemPermission { .. } => "filesystem_permission_denied",
            Self::DestinationExists { .. } => "destination_exists",
            Self::StorageFull { .. } => "storage_full",
            Self::Network { .. } => "network",
            Self::DeviceUnavailable { .. } => "device_unavailable",
            Self::OfferTimeout { .. } => "offer_timeout",
            Self::RelayPolicyIncompatible { .. } => "relay_policy_incompatible",
            Self::ProtocolIncompatible { .. } => "protocol_incompatible",
            Self::Transfer { .. } => "transfer",
            Self::Permission { .. } => "permission_denied",
            Self::Repository { .. } => "repository",
            Self::Cancelled { .. } => "cancelled",
            Self::InvalidInput { .. } => "invalid_input",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::SecureStorageLocked { .. } => "secure_storage_locked",
            Self::SecureStorageMissing { .. } => "secure_storage_missing",
            Self::SecureStorageCorrupted { .. } => "secure_storage_corrupted",
            Self::SecureStorageUnavailable { .. } => "secure_storage_unavailable",
            Self::Internal { .. } => "internal",
        }
    }

    pub(crate) fn reason(&self) -> &str {
        match self {
            Self::Initialization { reason }
            | Self::Ticket { reason }
            | Self::Filesystem { reason }
            | Self::FilesystemPermission { reason }
            | Self::DestinationExists { reason }
            | Self::StorageFull { reason }
            | Self::Network { reason }
            | Self::DeviceUnavailable { reason }
            | Self::OfferTimeout { reason }
            | Self::RelayPolicyIncompatible { reason }
            | Self::ProtocolIncompatible { reason }
            | Self::Transfer { reason }
            | Self::Permission { reason }
            | Self::Repository { reason }
            | Self::Cancelled { reason }
            | Self::InvalidInput { reason }
            | Self::InvalidTransition { reason }
            | Self::SecureStorageLocked { reason }
            | Self::SecureStorageMissing { reason }
            | Self::SecureStorageCorrupted { reason }
            | Self::SecureStorageUnavailable { reason }
            | Self::Internal { reason } => reason,
        }
    }

    fn classify(error: anyhow::Error, fallback: impl FnOnce(String) -> Self) -> Self {
        let reason = error.to_string();
        if let Some(existing) = error.chain().find_map(|cause| cause.downcast_ref::<Self>()) {
            return existing.with_reason(reason);
        }
        if let Some(io_error) = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<io::Error>())
        {
            return match io_error.kind() {
                io::ErrorKind::AlreadyExists => Self::DestinationExists { reason },
                io::ErrorKind::PermissionDenied => Self::FilesystemPermission { reason },
                io::ErrorKind::StorageFull => Self::StorageFull { reason },
                _ => Self::Filesystem { reason },
            };
        }
        if error
            .chain()
            .any(|cause| cause.downcast_ref::<sqlx::Error>().is_some())
        {
            return Self::Repository { reason };
        }
        fallback(reason)
    }

    fn from_error(error: anyhow::Error, fallback: impl FnOnce(String) -> Self) -> Self {
        let reason = error.to_string();
        if let Some(existing) = error.chain().find_map(|cause| cause.downcast_ref::<Self>()) {
            existing.with_reason(reason)
        } else {
            fallback(reason)
        }
    }

    fn with_reason(&self, reason: String) -> Self {
        match self {
            Self::Initialization { .. } => Self::Initialization { reason },
            Self::Ticket { .. } => Self::Ticket { reason },
            Self::Filesystem { .. } => Self::Filesystem { reason },
            Self::FilesystemPermission { .. } => Self::FilesystemPermission { reason },
            Self::DestinationExists { .. } => Self::DestinationExists { reason },
            Self::StorageFull { .. } => Self::StorageFull { reason },
            Self::Network { .. } => Self::Network { reason },
            Self::DeviceUnavailable { .. } => Self::DeviceUnavailable { reason },
            Self::OfferTimeout { .. } => Self::OfferTimeout { reason },
            Self::RelayPolicyIncompatible { .. } => Self::RelayPolicyIncompatible { reason },
            Self::ProtocolIncompatible { .. } => Self::ProtocolIncompatible { reason },
            Self::Transfer { .. } => Self::Transfer { reason },
            Self::Permission { .. } => Self::Permission { reason },
            Self::Repository { .. } => Self::Repository { reason },
            Self::Cancelled { .. } => Self::Cancelled { reason },
            Self::InvalidInput { .. } => Self::InvalidInput { reason },
            Self::InvalidTransition { .. } => Self::InvalidTransition { reason },
            Self::SecureStorageLocked { .. } => Self::SecureStorageLocked { reason },
            Self::SecureStorageMissing { .. } => Self::SecureStorageMissing { reason },
            Self::SecureStorageCorrupted { .. } => Self::SecureStorageCorrupted { reason },
            Self::SecureStorageUnavailable { .. } => Self::SecureStorageUnavailable { reason },
            Self::Internal { .. } => Self::Internal { reason },
        }
    }
}

impl From<anyhow::Error> for VnidropError {
    fn from(error: anyhow::Error) -> Self {
        Self::classify(error, |reason| Self::Internal { reason })
    }
}

impl From<io::Error> for VnidropError {
    fn from(error: io::Error) -> Self {
        Self::filesystem(error)
    }
}
