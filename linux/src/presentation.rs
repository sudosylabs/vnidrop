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

pub fn status_key(status: &str) -> &'static str {
    match status {
        "importing" => "status_preparing",
        "sharing" => "status_available",
        "receiving" => "status_receiving",
        "done" => "status_completed",
        "cancelled" => "status_cancelled",
        "stopped" => "status_stopped",
        "failed" => "status_failed",
        _ => "progress_working",
    }
}

pub fn is_active(transfer: &StoredTransfer) -> bool {
    matches!(
        transfer.status.as_str(),
        "importing" | "sharing" | "receiving"
    )
}

pub fn invitation(transfer: &StoredTransfer) -> Option<&str> {
    (transfer.direction == "send" && transfer.status == "sharing")
        .then_some(transfer.ticket.as_deref())
        .flatten()
}
