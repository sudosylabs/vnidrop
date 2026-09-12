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
        void Notice(string id, string state, string? title, string body, bool pending = false)
        {
            var primed = previous is not null;
            var already = primed && previous!.GetValueOrDefault(id) == state;
            var shown = false;
            if (enabled && Available && primed && !already && title is not null)
            {
                try
                {
                    AppNotificationManager.Default.Show(new AppNotificationBuilder().AddText(title).AddText(body).BuildNotification());
                    shown = true;
                }
                catch (System.Runtime.InteropServices.COMException) { Available = false; }
            }
            if (SavedDevicesReadModel.RememberNotice(pending, shown) || already)
            {
                current[id] = state;
            }
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
                SavedDeviceNoticeBody(notice),
                notice.Pending);
        previous = current;
    }

    private static string SavedDeviceNoticeBody(SavedDeviceNotificationFact notice)
    {
        var device = notice.DeviceName ?? Strings.Get("saved_devices_unnamed");
        var transfer = notice.TransferName ?? Strings.Get("receive_unknown_transfer");
        return notice.Kind switch
        {
            SavedDeviceNotificationKind.PairingRequest =>
                Strings.Format(SavedDevicesReadModel.BodyKey(notice.Kind.Value), ("device", device)),
            SavedDeviceNotificationKind.TargetedOffer =>
                Strings.Format(SavedDevicesReadModel.BodyKey(notice.Kind.Value), ("device", device), ("transferName", transfer)),
            SavedDeviceNotificationKind.TargetedReceiveCompleted or SavedDeviceNotificationKind.TargetedReceiveFailed =>
                Strings.Format(SavedDevicesReadModel.BodyKey(notice.Kind.Value), ("transferName", transfer)),
            SavedDeviceNotificationKind.TargetedSendCompleted or SavedDeviceNotificationKind.TargetedSendFailed =>
                Strings.Format(SavedDevicesReadModel.BodyKey(notice.Kind.Value), ("receiver", device), ("transferName", transfer)),
            _ => transfer,
        };
    }
}
