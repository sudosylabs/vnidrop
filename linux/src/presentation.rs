use std::ops::Deref;
use vnidrop::{CoreEvent, ReceiverRequest, StoredTransfer};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationDirection {
    Send,
    Receive,
    Unknown,
}

impl From<&str> for InvitationDirection {
    fn from(direction: &str) -> Self {
        match direction {
            "send" => Self::Send,
            "receive" => Self::Receive,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(transparent)]
pub struct InvitationTransfer {
    value: StoredTransfer,
    #[serde(skip)]
    pub role: InvitationDirection,
    #[serde(skip)]
    pub lifecycle: InvitationStatus,
}

impl From<StoredTransfer> for InvitationTransfer {
    fn from(value: StoredTransfer) -> Self {
        Self {
            role: value.direction.as_str().into(),
            lifecycle: value.status.as_str().into(),
            value,
        }
    }
}

impl Deref for InvitationTransfer {
    type Target = StoredTransfer;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(transparent)]
pub struct InvitationReceiver {
    value: ReceiverRequest,
    #[serde(skip)]
    pub delivery: ReceiverStatus,
}

impl From<ReceiverRequest> for InvitationReceiver {
    fn from(value: ReceiverRequest) -> Self {
        Self {
            delivery: value.status.as_str().into(),
            value,
        }
    }
}

impl Deref for InvitationReceiver {
    type Target = ReceiverRequest;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub fn status_key(status: InvitationStatus) -> &'static str {
    match status {
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

pub fn is_active(transfer: &InvitationTransfer) -> bool {
    matches!(
        transfer.lifecycle,
        InvitationStatus::Importing | InvitationStatus::Sharing | InvitationStatus::Receiving
    )
}

pub fn invitation(transfer: &InvitationTransfer) -> Option<&str> {
    (transfer.role == InvitationDirection::Send && transfer.lifecycle == InvitationStatus::Sharing)
        .then_some(transfer.ticket.as_deref())
        .flatten()
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn completed_invitations_and_legacy_receiver_cancellations_keep_distinct_states() {
        assert_eq!(status_key("done".into()), "status_completed");
        assert_eq!(status_key("completed".into()), "progress_working");
        assert_eq!(ReceiverStatus::from("completed"), ReceiverStatus::Completed);
        assert_eq!(ReceiverStatus::from("aborted"), ReceiverStatus::Cancelled);
        assert_eq!(ReceiverStatus::from("new-status"), ReceiverStatus::Unknown);
    }
}
