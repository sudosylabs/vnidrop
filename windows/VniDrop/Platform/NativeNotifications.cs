using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using VniDrop.Core;
using VniDrop.Native;

namespace VniDrop.Platform;

public sealed class NativeNotifications
{
    public static bool Available { get; private set; }
    private static Action? activate;
    private static bool registered;
    private Dictionary<string, string>? previous;

    public static void Register(Action onActivate)
    {
        activate = onActivate;
        try
        {
            AppNotificationManager.Default.NotificationInvoked += Invoked;
            AppNotificationManager.Default.Register();
            Available = registered = true;
        }
        catch (System.Runtime.InteropServices.COMException) { Available = false; }
    }
    private static void Invoked(AppNotificationManager sender, AppNotificationActivatedEventArgs args) => activate?.Invoke();
    public static void Unregister()
    {
        if (!registered) return;
        AppNotificationManager.Default.NotificationInvoked -= Invoked;
        AppNotificationManager.Default.Unregister(); Available = registered = false;
    }

    public void Update(CoreSnapshot snapshot, bool enabled)
    {
        var current = new Dictionary<string, string>();
        void Notice(string id, string state, string? title, string body)
        {
            current[id] = state;
            if (!enabled || !Available || previous is null || previous.GetValueOrDefault(id) == state || title is null) return;
            try { AppNotificationManager.Default.Show(new AppNotificationBuilder().AddText(title).AddText(body).BuildNotification()); }
            catch (System.Runtime.InteropServices.COMException) { Available = false; }
        }
        foreach (var transfer in snapshot.Transfers.Where(t => t.direction == "receive"))
            Notice("receive:" + transfer.localId, transfer.status, transfer.status switch
            { "done" => Strings.Get("notifications_receive_completed_title"), "failed" => Strings.Get("notifications_receive_failed_title"), _ => null },
                transfer.transferName ?? Strings.Get("receive_unknown_transfer"));
        foreach (var request in snapshot.Requests)
            Notice("request:" + request.id, request.status, request.status switch
            { "requested" => Strings.Get("approval_connection_request"), "completed" => Strings.Get("notifications_receiver_completed_title"), "failed" => Strings.Get("notifications_receiver_failed_title"), _ => null }, request.transferName);
        foreach (var offer in snapshot.Offers)
            Notice("offer:" + offer.transferId, "pending", Strings.Get("receive_review_title"), offer.transferName);
        foreach (var relationship in snapshot.Relationships)
            Notice("pairing:" + relationship.remoteEndpointId, relationship.state.ToString(), relationship.state == DeviceRelationshipState.PendingIncoming ? Strings.Get("saved_devices_pending_incoming") : null, Strings.Get("saved_devices_attention_title"));
        foreach (var transfer in snapshot.TargetedTransfers)
            Notice("targeted:" + transfer.id, transfer.state.ToString(), transfer.state switch
            { TargetedTransferState.Completed => Strings.Get("notifications_receive_completed_title"), TargetedTransferState.Failed => Strings.Get("notifications_receive_failed_title"), _ => null }, transfer.transferName);
        previous = current;
    }
}
