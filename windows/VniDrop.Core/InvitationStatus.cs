namespace VniDrop.Core;

public enum InvitationStatus
{
    Unknown,
    Importing,
    Sharing,
    Receiving,
    Done,
    Cancelled,
    Stopped,
    Failed,
}

public enum ReceiverStatus
{
    Unknown,
    Requested,
    Accepted,
    Completed,
    Refused,
    Expired,
    Failed,
}

public static class InvitationStatusCodec
{
    public static InvitationStatus Parse(string status) => status switch
    {
        "importing" => InvitationStatus.Importing,
        "sharing" => InvitationStatus.Sharing,
        "receiving" => InvitationStatus.Receiving,
        "done" => InvitationStatus.Done,
        "cancelled" => InvitationStatus.Cancelled,
        "stopped" => InvitationStatus.Stopped,
        "failed" => InvitationStatus.Failed,
        _ => InvitationStatus.Unknown,
    };

    public static ReceiverStatus ParseReceiver(string status) => status switch
    {
        "requested" => ReceiverStatus.Requested,
        "accepted" => ReceiverStatus.Accepted,
        "completed" => ReceiverStatus.Completed,
        "refused" => ReceiverStatus.Refused,
        "expired" => ReceiverStatus.Expired,
        "failed" => ReceiverStatus.Failed,
        _ => ReceiverStatus.Unknown,
    };
}
