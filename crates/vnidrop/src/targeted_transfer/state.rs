use crate::{api::TargetedTransferState, error::VnidropError};

impl TargetedTransferState {
    /// Validates a durable state change without exposing foreign state mutation.
    pub fn validate_transition_to(self, next: Self) -> Result<(), VnidropError> {
        let allowed = matches!(
            (self, next),
            (
                Self::Preparing,
                Self::Offering | Self::Cancelled | Self::Failed
            ) | (
                Self::Offering,
                Self::AwaitingApproval | Self::Cancelled | Self::Failed
            ) | (
                Self::AwaitingApproval,
                Self::Approved | Self::Declined | Self::Cancelled | Self::Failed
            ) | (
                Self::Approved,
                Self::Connecting | Self::Cancelled | Self::Failed
            ) | (
                Self::Connecting,
                Self::Transferring | Self::Interrupted | Self::Cancelled | Self::Failed
            ) | (
                Self::Transferring,
                Self::Completed | Self::Interrupted | Self::Cancelled | Self::Failed
            ) | (
                Self::Interrupted,
                Self::Connecting | Self::Cancelled | Self::Failed | Self::Deleted
            ) | (
                Self::Completed | Self::Declined | Self::Cancelled | Self::Failed,
                Self::Deleted
            )
        );
        if allowed {
            Ok(())
        } else {
            Err(VnidropError::InvalidTransition {
                reason: format!("{} -> {}", self.as_str(), next.as_str()),
            })
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Offering => "offering",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Approved => "approved",
            Self::Connecting => "connecting",
            Self::Transferring => "transferring",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Declined => "declined",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }
}
