using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Native;
using VniDrop.Platform;
using VniDrop.Services;
using VniDrop.ViewModels;

namespace VniDrop.Controls;

public sealed partial class DeviceDetailsView : UserControl
{
    private readonly HashSet<string> busy = [];
    private string? endpointId;
    private SavedDevice? device;
    private bool hadDevice;
    private bool receiveTrackingSubscribed;

    public DeviceDetailsView()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    public event EventHandler? BusyChanged;

    public event EventHandler? DeviceUnavailable;

    public string? EndpointId => endpointId;

    public bool IsBusy(string key) => busy.Contains(key);

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (!receiveTrackingSubscribed)
        {
            App.Window.Model.TargetedReceiveChanged += TargetedReceiveChanged;
            receiveTrackingSubscribed = true;
        }
        Refresh();
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        if (receiveTrackingSubscribed)
        {
            App.Window.Model.TargetedReceiveChanged -= TargetedReceiveChanged;
            receiveTrackingSubscribed = false;
        }
    }

    private void TargetedReceiveChanged()
    {
        if (!DispatcherQueue.HasThreadAccess)
        {
            DispatcherQueue.TryEnqueue(TargetedReceiveChanged);
            return;
        }

        Refresh();
        BusyChanged?.Invoke(this, EventArgs.Empty);
    }

    public void ShowDevice(string? id)
    {
        if (!string.Equals(endpointId, id, StringComparison.Ordinal))
        {
            hadDevice = false;
        }
        endpointId = id;
        Refresh();
    }

    public void Refresh()
    {
        if (string.IsNullOrWhiteSpace(endpointId) || App.Window.Model.Snapshot is not { } snapshot)
        {
            device = null;
            Root.Visibility = Visibility.Collapsed;
            return;
        }

        device = snapshot.Devices.FirstOrDefault(candidate => candidate.endpointId == endpointId);
        if (device is null)
        {
            Root.Visibility = Visibility.Collapsed;
            if (hadDevice)
            {
                hadDevice = false;
                DeviceUnavailable?.Invoke(this, EventArgs.Empty);
            }
            return;
        }

        hadDevice = true;
        Root.Visibility = Visibility.Visible;
        var name = DisplayName(device);
        DeviceNameText.Text = name;
        MoreActionsTitle.Text = Strings.Format("saved_devices_more_actions", ("device", name));
        var hasAuthenticatedName = !string.IsNullOrWhiteSpace(device.localLabel)
            && !string.Equals(device.localLabel, device.remoteDisplayName, StringComparison.Ordinal);
        AuthenticatedNameText.Text = hasAuthenticatedName
            ? Strings.Format("saved_devices_authenticated_name", ("name", device.remoteDisplayName))
            : string.Empty;
        AuthenticatedNameText.Visibility = hasAuthenticatedName ? Visibility.Visible : Visibility.Collapsed;
        EndpointText.Text = Strings.Format("saved_devices_endpoint", ("deviceId", device.endpointId));

        var deviceBusy = busy.Contains(DeviceKey(device.endpointId));
        DeviceBusyIndicator.IsActive = deviceBusy;
        DeviceBusyIndicator.Visibility = deviceBusy ? Visibility.Visible : Visibility.Collapsed;
        SendAction.IsEnabled = !deviceBusy;
        LabelAction.IsEnabled = !deviceBusy;
        ForgetAction.IsEnabled = !deviceBusy;
        BlockAction.IsEnabled = !deviceBusy;
        LabelAction.Value = string.IsNullOrWhiteSpace(device.localLabel) ? string.Empty : device.localLabel!;

        var transfers = snapshot.TargetedTransfers
            .Where(transfer => transfer.state != TargetedTransferState.Deleted && PeerId(transfer) == device.endpointId)
            .OrderByDescending(transfer => transfer.updatedAt)
            .Select(transfer => new DeviceTransferItem(
                transfer,
                name,
                busy.Contains(TransferKey(transfer.id)),
                App.Window.Model.IsTargetedReceiveRunning(transfer.id)))
            .ToArray();
        Transfers.ItemsSource = transfers;
        NoTransfersText.Visibility = transfers.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    public MenuFlyout BuildDeviceMenu(SavedDeviceItem item)
    {
        var menu = new MenuFlyout();
        var send = MenuItem(Strings.Get("saved_devices_send_action"), "\uE724");
        send.IsEnabled = !IsBusy(DeviceKey(item.Id));
        send.Click += async (_, _) => await App.Window.ShowDraftAsync(item.Device);
        menu.Items.Add(send);

        var label = MenuItem(Strings.Get("saved_devices_label_action"), "\uE70F");
        label.IsEnabled = !IsBusy(DeviceKey(item.Id));
        label.Click += async (_, _) => await LabelAsync(item.Device);
        menu.Items.Add(label);
        menu.Items.Add(new MenuFlyoutSeparator());

        var forget = MenuItem(Strings.Get("saved_devices_forget_action"), "\uE74D", critical: true);
        forget.IsEnabled = !IsBusy(DeviceKey(item.Id));
        forget.Click += async (_, _) => await ForgetAsync(item.Device, block: false);
        menu.Items.Add(forget);

        var block = MenuItem(Strings.Get("saved_devices_block_action"), "\uE733", critical: true);
        block.IsEnabled = !IsBusy(DeviceKey(item.Id));
        block.Click += async (_, _) => await ForgetAsync(item.Device, block: true);
        menu.Items.Add(block);
        return menu;
    }

    public MenuFlyout BuildTransferMenu(DeviceTransferItem item)
    {
        var menu = new MenuFlyout();
        if (item.PrimaryAction != DeviceTransferAction.None)
        {
            var primary = MenuItem(item.PrimaryText, "\uE8FB");
            primary.IsEnabled = item.IsPrimaryEnabled;
            primary.Click += async (_, _) => await ExecuteTransferAsync(item, item.PrimaryAction);
            menu.Items.Add(primary);
            menu.Items.Add(new MenuFlyoutSeparator());
        }

        var secondary = MenuItem(
            item.SecondaryText,
            item.SecondaryAction == DeviceTransferAction.Delete ? "\uE74D" : "\uE711",
            critical: item.SecondaryAction is DeviceTransferAction.Cancel or DeviceTransferAction.Delete);
        secondary.IsEnabled = item.IsSecondaryEnabled;
        secondary.Click += async (_, _) => await ExecuteTransferAsync(item, item.SecondaryAction);
        menu.Items.Add(secondary);
        return menu;
    }

    public async Task LabelAsync(SavedDevice savedDevice)
    {
        var input = new TextBox
        {
            Text = savedDevice.localLabel ?? string.Empty,
            PlaceholderText = savedDevice.remoteDisplayName,
            MinWidth = 320,
        };
        var result = await App.Window.DialogAsync(
            Strings.Get("saved_devices_label_title"),
            input,
            Strings.Get("saved_devices_label_save"));
        if (result != ContentDialogResult.Primary)
        {
            return;
        }

        var label = string.IsNullOrWhiteSpace(input.Text) ? null : input.Text.Trim();
        await RunBusyAsync(
            DeviceKey(savedDevice.endpointId),
            () => App.Window.Model.Session.RunAsync(core => core.SetSavedDeviceLabel(
                savedDevice.endpointId,
                label)));
    }

    public async Task ForgetAsync(SavedDevice savedDevice, bool block)
    {
        var name = DisplayName(savedDevice);
        var result = await App.Window.DialogAsync(
            Strings.Get(block ? "saved_devices_block_confirm_title" : "saved_devices_forget_confirm_title"),
            Strings.Format(
                block ? "saved_devices_block_confirm_body" : "saved_devices_forget_confirm_body",
                ("device", name)),
            Strings.Get(block ? "saved_devices_block_action" : "saved_devices_forget_action"),
            intent: DialogIntent.Destructive);
        if (result != ContentDialogResult.Primary)
        {
            return;
        }

        await RunBusyAsync(
            DeviceKey(savedDevice.endpointId),
            () => App.Window.Model.Session.RunAsync(core =>
            {
                if (block)
                {
                    core.BlockDevice(savedDevice.endpointId);
                }
                else
                {
                    core.ForgetSavedDevice(savedDevice.endpointId);
                }
            }));
    }

    public async Task ExecuteTransferAsync(DeviceTransferItem item, DeviceTransferAction action)
    {
        if (action == DeviceTransferAction.None)
        {
            return;
        }

        if (action == DeviceTransferAction.Delete)
        {
            var result = await App.Window.DialogAsync(
                Strings.Get("button_delete_transfer"),
                Strings.Get("windows_delete_body"),
                Strings.Get("saved_devices_transfer_delete"),
                intent: DialogIntent.Destructive);
            if (result != ContentDialogResult.Primary)
            {
                return;
            }
        }

        if (action is DeviceTransferAction.Receive or DeviceTransferAction.Resume)
        {
            App.Window.Model.StartTargetedReceive(item.Id, action == DeviceTransferAction.Resume);
            return;
        }

        await RunBusyAsync(
            TransferKey(item.Id),
            () => App.Window.Model.Session.RunAsync(core =>
            {
                if (action == DeviceTransferAction.Cancel)
                {
                    core.CancelTargetedTransfer(item.Id);
                }
                else if (action == DeviceTransferAction.Delete)
                {
                    core.DeleteTargetedTransfer(item.Id);
                }
            }));
    }

    private async void SendFiles(object sender, RoutedEventArgs e)
    {
        if (device is not null)
        {
            await App.Window.ShowDraftAsync(device);
        }
    }

    private async void EditLabel(object sender, RoutedEventArgs e)
    {
        if (device is not null)
        {
            await LabelAsync(device);
        }
    }

    private async void Forget(object sender, RoutedEventArgs e)
    {
        if (device is not null)
        {
            await ForgetAsync(device, block: false);
        }
    }

    private async void Block(object sender, RoutedEventArgs e)
    {
        if (device is not null)
        {
            await ForgetAsync(device, block: true);
        }
    }

    private async void TransferPrimary(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceTransferItem item })
        {
            await ExecuteTransferAsync(item, item.PrimaryAction);
        }
    }

    private void TransferMore(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceTransferItem item } target)
        {
            BuildTransferMenu(item).ShowAt(target);
        }
    }

    private void TransferRowLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceTransferItem item } target)
        {
            target.ContextFlyout = BuildTransferMenu(item);
        }
    }

    private async Task RunBusyAsync(string key, Func<Task> operation)
    {
        if (!busy.Add(key))
        {
            return;
        }

        BusyChanged?.Invoke(this, EventArgs.Empty);
        Refresh();
        try
        {
            await App.Window.Model.PerformAsync(operation);
        }
        finally
        {
            busy.Remove(key);
            BusyChanged?.Invoke(this, EventArgs.Empty);
            Refresh();
        }
    }

    private static MenuFlyoutItem MenuItem(string text, string glyph, bool critical = false)
    {
        var item = new MenuFlyoutItem
        {
            Text = text,
            Icon = new FontIcon { Glyph = glyph },
        };
        if (!critical)
        {
            return item;
        }

        var resources = Application.Current.Resources;
        item.Resources["MenuFlyoutItemForeground"] = resources["VniDropCriticalTextBrush"];
        item.Resources["MenuFlyoutItemBackgroundPointerOver"] = resources["VniDropCriticalSubtlePointerOverBrush"];
        item.Resources["MenuFlyoutItemForegroundPointerOver"] = resources["VniDropCriticalInteractiveForegroundBrush"];
        item.Resources["MenuFlyoutItemBackgroundPressed"] = resources["VniDropCriticalSubtlePressedBrush"];
        item.Resources["MenuFlyoutItemForegroundPressed"] = resources["VniDropCriticalInteractiveForegroundBrush"];
        return item;
    }

    private static string DisplayName(SavedDevice savedDevice)
    {
        if (!string.IsNullOrWhiteSpace(savedDevice.localLabel))
        {
            return savedDevice.localLabel!;
        }
        return string.IsNullOrWhiteSpace(savedDevice.remoteDisplayName)
            ? savedDevice.endpointId
            : savedDevice.remoteDisplayName;
    }

    private static string PeerId(TargetedTransfer transfer)
    {
        return transfer.role == TargetedTransferRole.Sender
            ? transfer.receiverEndpointId
            : transfer.senderEndpointId;
    }

    private static string DeviceKey(string id) => $"device:{id}";

    private static string TransferKey(string id) => $"transfer:{id}";
}
