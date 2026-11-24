use vnidrop::{CoreEvent, StoredTransfer};

pub fn activity_key(event: &CoreEvent) -> Option<&'static str> {
    match (event.phase.as_str(), event.kind.as_str()) {
        ("import", "started") => Some("transfer_event_preparing"),
        ("ticket", "created") => Some("transfer_event_ready"),
        ("network", "connecting" | "connected") => Some("transfer_event_connecting"),
        ("download", "found-collection") => Some("transfer_event_downloading"),
        ("lifecycle", "done") => Some("progress_completed"),
        ("lifecycle", "cancelled" | "share-stopped") | (_, "share-stopped") => {
            Some("transfer_event_stopped")
        }
        (_, "receiver-requested") => Some("transfer_event_requested"),
        (_, "receiver-accepted" | "receiver-auto-approved") => Some("transfer_event_approved"),
        (_, "receiver-refused") => Some("transfer_event_refused"),
        (_, "receiver-completed") => Some("transfer_event_completed"),
        (_, "failed") => Some("transfer_event_failed"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationStatus {
    Importing,
    Sharing,
    Receiving,
    Done,
    Cancelled,
    Stopped,
    Failed,
    Unknown,
}

impl From<&str> for InvitationStatus {
    fn from(status: &str) -> Self {
        match status {
            "importing" => Self::Importing,
            "sharing" => Self::Sharing,
            "receiving" => Self::Receiving,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "stopped" => Self::Stopped,
            "failed" => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverStatus {
    Requested,
    Accepted,
    Completed,
    Refused,
    Expired,
    Failed,
    Cancelled,
    Unknown,
}

impl From<&str> for ReceiverStatus {
    fn from(status: &str) -> Self {
        match status {
            "requested" => Self::Requested,
            "accepted" => Self::Accepted,
            "completed" => Self::Completed,
            "refused" => Self::Refused,
            "expired" => Self::Expired,
            "failed" => Self::Failed,
            "cancelled" | "aborted" => Self::Cancelled,
            _ => Self::Unknown,
        }
    }
}

pub fn status_key(status: &str) -> &'static str {
    match InvitationStatus::from(status) {
        InvitationStatus::Importing => "status_preparing",
        InvitationStatus::Sharing => "status_available",
        InvitationStatus::Receiving => "status_receiving",
        InvitationStatus::Done => "status_completed",
        InvitationStatus::Cancelled => "status_cancelled",
        InvitationStatus::Stopped => "status_stopped",
        InvitationStatus::Failed => "status_failed",
        InvitationStatus::Unknown => "progress_working",
    }
}

pub fn is_active(transfer: &StoredTransfer) -> bool {
    matches!(
        InvitationStatus::from(transfer.status.as_str()),
        InvitationStatus::Importing | InvitationStatus::Sharing | InvitationStatus::Receiving
    )
}

pub fn invitation(transfer: &StoredTransfer) -> Option<&str> {
    (transfer.direction == "send" && transfer.status == "sharing")
        .then_some(transfer.ticket.as_deref())
        .flatten()
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn completed_invitations_and_legacy_receiver_cancellations_keep_distinct_states() {
        assert_eq!(status_key("done"), "status_completed");
        assert_eq!(status_key("completed"), "progress_working");
        assert_eq!(ReceiverStatus::from("completed"), ReceiverStatus::Completed);
        assert_eq!(ReceiverStatus::from("aborted"), ReceiverStatus::Cancelled);
        assert_eq!(ReceiverStatus::from("new-status"), ReceiverStatus::Unknown);
    }
}
