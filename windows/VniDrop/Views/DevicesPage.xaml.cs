using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using VniDrop.Native;
using VniDrop.Platform;
using VniDrop.ViewModels;

namespace VniDrop.Views;

public sealed partial class DevicesPage : Page
{
    private AppViewModel Model => App.Window.Model;
    private string previous = "";
    private readonly HashSet<string> busy = [];
    public DevicesPage()
    {
        InitializeComponent(); Loaded += (_, _) => { Model.Updated += Update; Update(); };
        Unloaded += (_, _) => Model.Updated -= Update;
        SizeChanged += (_, e) => ContentStack.Width = Math.Min(980, e.NewSize.Width);
    }
    private string DeviceName(string id)
    {
        var device = Model.Snapshot?.Devices.FirstOrDefault(d => d.endpointId == id);
        return device?.localLabel ?? device?.remoteDisplayName ?? Strings.Get("saved_devices_unnamed");
    }
    private void Update()
    {
        if (Model.Snapshot is not { } s) return;
        var fingerprint = System.Text.Json.JsonSerializer.Serialize(new { s.Devices, s.Relationships, s.EligibleDevices, s.TargetedTransfers, s.BlockedDevices, Busy = busy.Order().ToArray() });
        if (fingerprint == previous) return;
        previous = fingerprint;
        Devices.Children.Clear(); Pending.Children.Clear(); Eligible.Children.Clear(); Transfers.Children.Clear(); Blocked.Children.Clear();
        Empty.Visibility = s.Devices.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        foreach (var device in s.Devices)
        {
            var name = DeviceName(device.endpointId);
            var menu = new MenuFlyout();
            AddMenu(menu, "saved_devices_label_action", () => LabelAsync(device));
            AddMenu(menu, "saved_devices_forget_action", () => ForgetAsync(device, false));
            AddMenu(menu, "saved_devices_block_action", () => ForgetAsync(device, true));
            var sendContent = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            sendContent.Children.Add(new SymbolIcon(Symbol.Send)); sendContent.Children.Add(new TextBlock { Text = Strings.Get("saved_devices_send_action") });
            var send = new Button { Content = sendContent };
            send.Click += async (_, _) => await App.Window.ShowDraftAsync(device);
            var row = Row(name, device.remoteDisplayName != name ? device.remoteDisplayName : null, send);
            row.ContextFlyout = menu; Devices.Children.Add(row);
        }
        foreach (var relationship in s.Relationships.Where(r => r.state is DeviceRelationshipState.PendingIncoming or DeviceRelationshipState.PendingOutgoing))
        {
            var incoming = relationship.state == DeviceRelationshipState.PendingIncoming;
            var actions = incoming ? new[] {
                Action("saved_devices_accept_pairing_action", relationship.remoteEndpointId, () => Model.Session.RunAsync(c => c.RespondToDevicePairing(relationship.remoteEndpointId, true))),
                Action("saved_devices_decline_action", relationship.remoteEndpointId, () => Model.Session.RunAsync(c => c.RespondToDevicePairing(relationship.remoteEndpointId, false))) } : Array.Empty<Button>();
            Pending.Children.Add(Row(DeviceName(relationship.remoteEndpointId), Strings.Get(incoming ? "saved_devices_pending_incoming" : "saved_devices_pending_outgoing"), actions));
        }
        foreach (var eligible in s.EligibleDevices)
        {
            Eligible.Children.Add(Row(eligible.remoteDisplayName ?? Strings.Get("approval_nearby_device"), null,
                Action("saved_devices_remember_action", eligible.peerEndpointId, () => Model.Session.RunAsync(c => c.RequestSavedDevicePairing(eligible.peerEndpointId))),
                Action("saved_devices_decline_action", eligible.peerEndpointId, () => Model.Session.RunAsync(c => c.DeclinePairingEligibility(eligible.peerEndpointId)))));
        }
        foreach (var transfer in s.TargetedTransfers.Where(t => t.state != TargetedTransferState.Deleted).OrderByDescending(t => t.createdAt))
        {
            var actions = new List<Button>();
            if (transfer.role == TargetedTransferRole.Receiver && transfer.state is TargetedTransferState.Approved or TargetedTransferState.Interrupted)
                actions.Add(Action(transfer.state == TargetedTransferState.Interrupted ? "saved_devices_transfer_resume" : "saved_devices_transfer_receive", transfer.id,
                    () => Model.Session.RunAsync(c => { if (transfer.state == TargetedTransferState.Interrupted) c.ResumeTargetedTransfer(transfer.id, Model.Preferences.ReceiveDirectory); else c.ReceiveTargetedTransfer(transfer.id, Model.Preferences.ReceiveDirectory); })));
            var terminal = transfer.state is TargetedTransferState.Completed or TargetedTransferState.Declined or TargetedTransferState.Cancelled or TargetedTransferState.Failed or TargetedTransferState.Deleted;
            if (!terminal) actions.Add(Action("saved_devices_transfer_cancel", "cancel:" + transfer.id, () => Model.Session.RunAsync(c => c.CancelTargetedTransfer(transfer.id))));
            else actions.Add(Action("saved_devices_transfer_delete", transfer.id, async () =>
            {
                if (await App.Window.DialogAsync(Strings.Get("button_delete_transfer"), Strings.Get("windows_delete_body"), Strings.Get("saved_devices_transfer_delete")) == ContentDialogResult.Primary)
                    await Model.Session.RunAsync(c => c.DeleteTargetedTransfer(transfer.id));
            }));
            var peer = transfer.role == TargetedTransferRole.Sender ? transfer.receiverEndpointId : transfer.senderEndpointId;
            var status = Strings.Get(transfer.state switch
            {
                TargetedTransferState.Preparing => "status_preparing",
                TargetedTransferState.Offering => "status_offering",
                TargetedTransferState.AwaitingApproval => "status_awaiting_approval",
                TargetedTransferState.Approved => "status_approved",
                TargetedTransferState.Connecting => "status_connecting",
                TargetedTransferState.Transferring => "status_transferring",
                TargetedTransferState.Interrupted => "status_interrupted",
                TargetedTransferState.Completed => "status_completed",
                TargetedTransferState.Declined => "status_declined",
                TargetedTransferState.Cancelled or TargetedTransferState.Deleted => "status_cancelled",
                TargetedTransferState.Failed => "status_failed",
                _ => throw new ArgumentOutOfRangeException(),
            });
            var details = Strings.Format(transfer.role == TargetedTransferRole.Sender ? "saved_devices_transfer_direction_outgoing" : "saved_devices_transfer_direction_incoming", ("device", DeviceName(peer)));
            Transfers.Children.Add(Row(transfer.transferName, $"{details} · {status}\n{Strings.Format("saved_devices_transfer_progress", ("verified", Strings.Size(transfer.verifiedBytes)), ("total", Strings.Size(transfer.totalSize)))}", actions.ToArray()));
        }
        foreach (var id in s.BlockedDevices)
            Blocked.Children.Add(Row(Strings.Format("saved_devices_endpoint", ("deviceId", id)), null, Action("windows_unblock", id, () => Model.Session.RunAsync(c => c.UnblockDevice(id)))));
        PendingSection.Visibility = Pending.Children.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        EligibleSection.Visibility = Eligible.Children.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        BlockedSection.Visibility = Blocked.Children.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        NoTransfers.Visibility = Transfers.Children.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }
    private static Grid Row(string title, string? detail, params Button[] actions)
    {
        var row = new Grid { ColumnSpacing = 12, Padding = new(0, 8, 0, 8) };
        row.ColumnDefinitions.Add(new() { Width = new(1, GridUnitType.Star) }); row.ColumnDefinitions.Add(new() { Width = GridLength.Auto });
        var text = new StackPanel { Spacing = 4, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(new TextBlock { Text = title, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], TextWrapping = TextWrapping.Wrap });
        if (detail is not null) text.Children.Add(new TextBlock { Text = detail, TextWrapping = TextWrapping.Wrap, Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
        row.Children.Add(text);
        var commands = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, VerticalAlignment = VerticalAlignment.Center };
        foreach (var action in actions) commands.Children.Add(action);
        Grid.SetColumn(commands, 1); row.Children.Add(commands); return row;
    }
    private Button Action(string label, string id, Func<Task> action)
    {
        var button = new Button { Content = Strings.Get(label), IsEnabled = !busy.Contains(id) };
        button.Click += async (_, _) =>
        {
            if (!busy.Add(id)) return;
            Update();
            try { await Model.PerformAsync(action); }
            finally { busy.Remove(id); Update(); }
        };
        return button;
    }
    private void AddMenu(MenuFlyout menu, string label, Func<Task> action)
    {
        var item = new MenuFlyoutItem { Text = Strings.Get(label) };
        item.Click += async (_, _) => await Model.PerformAsync(action); menu.Items.Add(item);
    }
    private async Task LabelAsync(SavedDevice device)
    {
        var input = new TextBox { Text = device.localLabel ?? "", Header = Strings.Get("saved_devices_label_title"), MaxLength = 100 };
        if (await App.Window.DialogAsync(Strings.Get("saved_devices_label_title"), input, Strings.Get("saved_devices_label_save")) == ContentDialogResult.Primary)
            await Model.Session.RunAsync(c => c.SetSavedDeviceLabel(device.endpointId, string.IsNullOrWhiteSpace(input.Text) ? null : input.Text.Trim()));
    }
    private async Task ForgetAsync(SavedDevice device, bool block)
    {
        var name = DeviceName(device.endpointId);
        if (await App.Window.DialogAsync(Strings.Get(block ? "saved_devices_block_confirm_title" : "saved_devices_forget_confirm_title"),
            Strings.Format(block ? "saved_devices_block_confirm_body" : "saved_devices_forget_confirm_body", ("device", name)), Strings.Get(block ? "saved_devices_block_action" : "saved_devices_forget_action")) != ContentDialogResult.Primary) return;
        await Model.Session.RunAsync(c => { if (block) c.BlockDevice(device.endpointId); else c.ForgetSavedDevice(device.endpointId); });
    }
}
