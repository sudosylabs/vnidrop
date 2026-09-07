use vnidrop::StoredTransfer;

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
