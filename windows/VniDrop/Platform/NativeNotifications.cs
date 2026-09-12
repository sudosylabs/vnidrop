using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using VniDrop.Core;

namespace VniDrop.Platform;

public sealed class NativeNotifications
{
    public static bool Available { get; private set; }
    private static Action? activate;
    private static bool registered;
    private Dictionary<string, string>? previous;

    public static void Register(Action onActivate)
    {
        if (registered || !AppNotificationManager.IsSupported()) return;
        activate = onActivate;
        try
        {
            AppNotificationManager.Default.NotificationInvoked += Invoked;
            AppNotificationManager.Default.Register();
            Available = registered = true;
        }
        catch (System.Runtime.InteropServices.COMException)
        {
            AppNotificationManager.Default.NotificationInvoked -= Invoked;
            activate = null;
            Available = false;
        }
    }
    private static void Invoked(AppNotificationManager sender, AppNotificationActivatedEventArgs args) => activate?.Invoke();
    public static void Unregister()
    {
        if (!registered) return;
        AppNotificationManager.Default.NotificationInvoked -= Invoked;
        try { AppNotificationManager.Default.Unregister(); }
        finally { activate = null; Available = registered = false; }
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
        foreach (var notice in SavedDevicesReadModel.Derive(SavedDevicesReadInputs.FromSnapshot(snapshot)).Notifications)
            Notice(
                notice.Id,
                notice.State,
                notice.Kind is { } kind ? Strings.Get(SavedDevicesReadModel.TitleKey(kind)) : null,
                notice.Kind == SavedDeviceNotificationKind.PairingRequest
                    ? Strings.Get("saved_devices_attention_title")
                    : notice.TransferName ?? Strings.Get("receive_unknown_transfer"));
        previous = current;
    }
}
