using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.ViewModels;

namespace VniDrop.Views;

public sealed partial class DevicesPage : Page
{
    private readonly HashSet<string> busy = [];
    private SavedDeviceItem[] savedItems = [];
    private DeviceTransferItem[] transferItems = [];
    private string? selectedEndpointId;
    private bool isWide;
    private bool rendering;
    private bool subscribed;

    public DevicesPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
        SizeChanged += PageSizeChanged;
        DetailsView.BusyChanged += DetailsBusyChanged;
        DetailsView.DeviceUnavailable += DetailsUnavailable;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (!subscribed)
        {
            App.Window.Model.Updated += ModelUpdated;
            subscribed = true;
        }
        Update();
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        if (subscribed)
        {
            App.Window.Model.Updated -= ModelUpdated;
            subscribed = false;
        }
    }

    private void ModelUpdated()
    {
        if (!DispatcherQueue.HasThreadAccess)
        {
            DispatcherQueue.TryEnqueue(Update);
            return;
        }
        Update();
    }

    private void Update()
    {
        if (App.Window.Model.Snapshot is not { } snapshot)
        {
            return;
        }

        var pendingIds = snapshot.Relationships
            .Where(relationship => relationship.state is DeviceRelationshipState.PendingIncoming or DeviceRelationshipState.PendingOutgoing)
            .Select(relationship => relationship.remoteEndpointId)
            .ToHashSet(StringComparer.Ordinal);

        var pending = snapshot.Relationships
            .Where(relationship => relationship.state is DeviceRelationshipState.PendingIncoming or DeviceRelationshipState.PendingOutgoing)
            .Select(relationship => DeviceActionRowItem.Pending(
                relationship,
                DeviceName(snapshot, relationship.remoteEndpointId),
                busy.Contains($"relationship:{relationship.remoteEndpointId}")))
            .ToArray();
        var eligible = snapshot.EligibleDevices
            .Where(candidate => !pendingIds.Contains(candidate.peerEndpointId))
            .Select(candidate => DeviceActionRowItem.Eligible(
                candidate,
                string.IsNullOrWhiteSpace(candidate.remoteDisplayName)
                    ? DeviceName(snapshot, candidate.peerEndpointId)
                    : candidate.remoteDisplayName,
                busy.Contains($"eligibility:{candidate.peerEndpointId}")))
            .ToArray();
        var blocked = snapshot.BlockedDevices
            .Select(endpoint => DeviceActionRowItem.Blocked(endpoint, busy.Contains($"blocked:{endpoint}")))
            .ToArray();

        transferItems = snapshot.TargetedTransfers
            .Where(transfer => transfer.state != TargetedTransferState.Deleted)
            .OrderByDescending(transfer => transfer.updatedAt)
            .Select(transfer => new DeviceTransferItem(
                transfer,
                DeviceName(snapshot, PeerId(transfer)),
                DetailsView.IsBusy($"transfer:{transfer.id}"),
                App.Window.Model.IsTargetedReceiveRunning(transfer.id)))
            .ToArray();
        savedItems = snapshot.Devices
            .Select(device => new SavedDeviceItem(
                device,
                transferItems.Count(transfer => transfer.PeerId == device.endpointId),
                DetailsView.IsBusy($"device:{device.endpointId}")))
            .ToArray();

        rendering = true;
        try
        {
            PendingItems.ItemsSource = pending;
            PendingSection.Visibility = pending.Length == 0 ? Visibility.Collapsed : Visibility.Visible;
            EligibleItems.ItemsSource = eligible;
            EligibleSection.Visibility = eligible.Length == 0 ? Visibility.Collapsed : Visibility.Visible;
            BlockedItems.ItemsSource = blocked;
            BlockedSection.Visibility = blocked.Length == 0 ? Visibility.Collapsed : Visibility.Visible;
            SavedDevices.ItemsSource = savedItems;
            SavedEmpty.Visibility = savedItems.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        }
        finally
        {
            rendering = false;
        }

        ApplyResponsiveLayout();
    }

    private void PageSizeChanged(object sender, SizeChangedEventArgs e) => ApplyResponsiveLayout();

    private void ApplyResponsiveLayout()
    {
        if (ActualWidth <= 0)
        {
            return;
        }

        PageRoot.Padding = (Thickness)Application.Current.Resources[
            ActualWidth >= 840
                ? "VniDropPagePadding"
                : ActualWidth >= 560
                    ? "VniDropPagePaddingMedium"
                    : "VniDropPagePaddingCompact"];
        var maximumWidth = (double)Application.Current.Resources["VniDropPageMaxWidth"];
        var availableWidth = Math.Max(0, PageRoot.ActualWidth - PageRoot.Padding.Left - PageRoot.Padding.Right);
        BodyGrid.Width = Math.Min(maximumWidth, availableWidth);
        isWide = BodyGrid.Width >= 960;

        var showDetails = isWide && savedItems.Length > 0;
        PaneDividerColumn.Width = showDetails ? new GridLength(37) : new GridLength(0);
        DetailsColumn.Width = showDetails ? new GridLength(420) : new GridLength(0);
        PaneDivider.Visibility = showDetails ? Visibility.Visible : Visibility.Collapsed;
        DetailsHost.Visibility = showDetails ? Visibility.Visible : Visibility.Collapsed;

        if (showDetails)
        {
            var selected = savedItems.FirstOrDefault(item => item.Id == selectedEndpointId) ?? savedItems[0];
            selectedEndpointId = selected.Id;
            rendering = true;
            SavedDevices.SelectedItem = selected;
            rendering = false;
            DetailsView.ShowDevice(selected.Id);
        }
        else
        {
            DetailsView.ShowDevice(null);
        }

        var savedIds = savedItems.Select(item => item.Id).ToHashSet(StringComparer.Ordinal);
        var visibleTransfers = showDetails
            ? transferItems.Where(transfer => !savedIds.Contains(transfer.PeerId)).ToArray()
            : transferItems;
        GlobalTransfers.ItemsSource = visibleTransfers;
        TransfersSection.Visibility = visibleTransfers.Length == 0 ? Visibility.Collapsed : Visibility.Visible;
    }

    private void DeviceSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (rendering || !isWide || SavedDevices.SelectedItem is not SavedDeviceItem selected)
        {
            return;
        }

        selectedEndpointId = selected.Id;
        DetailsView.ShowDevice(selected.Id);
    }

    private void DeviceClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not SavedDeviceItem item)
        {
            return;
        }

        selectedEndpointId = item.Id;
        if (isWide)
        {
            DetailsView.ShowDevice(item.Id);
        }
        else
        {
            App.Window.NavigateTo(typeof(DeviceDetailsPage), item.Id);
        }
    }

    private void DeviceRowLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: SavedDeviceItem item } target)
        {
            target.ContextFlyout = DetailsView.BuildDeviceMenu(item);
        }
    }

    private void DeviceMore(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: SavedDeviceItem item } target)
        {
            DetailsView.BuildDeviceMenu(item).ShowAt(target);
        }
    }

    private async void ActionPrimary(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceActionRowItem item })
        {
            await ExecutePageActionAsync(item, item.PrimaryAction);
        }
    }

    private async void ActionSecondary(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceActionRowItem item })
        {
            await ExecutePageActionAsync(item, item.SecondaryAction);
        }
    }

    private async Task ExecutePageActionAsync(DeviceActionRowItem item, DevicePageAction action)
    {
        if (action == DevicePageAction.None || !busy.Add(item.Key))
        {
            return;
        }

        Update();
        try
        {
            await App.Window.Model.PerformAsync(() => App.Window.Model.Session.RunAsync(core =>
            {
                switch (action)
                {
                    case DevicePageAction.AcceptPairing:
                        core.RespondToDevicePairing(item.EndpointId, true);
                        break;
                    case DevicePageAction.DeclinePairing:
                        core.RespondToDevicePairing(item.EndpointId, false);
                        break;
                    case DevicePageAction.RequestPairing:
                        core.RequestSavedDevicePairing(item.EndpointId);
                        break;
                    case DevicePageAction.DeclineEligibility:
                        core.DeclinePairingEligibility(item.EndpointId);
                        break;
                    case DevicePageAction.Unblock:
                        core.UnblockDevice(item.EndpointId);
                        break;
                }
            }));
        }
        finally
        {
            busy.Remove(item.Key);
            Update();
        }
    }

    private async void GlobalTransferPrimary(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceTransferItem item })
        {
            await DetailsView.ExecuteTransferAsync(item, item.PrimaryAction);
        }
    }

    private void GlobalTransferMore(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceTransferItem item } target)
        {
            DetailsView.BuildTransferMenu(item).ShowAt(target);
        }
    }

    private void GlobalTransferRowLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: DeviceTransferItem item } target)
        {
            target.ContextFlyout = DetailsView.BuildTransferMenu(item);
        }
    }

    private void DetailsBusyChanged(object? sender, EventArgs e) => Update();

    private void DetailsUnavailable(object? sender, EventArgs e)
    {
        selectedEndpointId = null;
        if (DispatcherQueue.HasThreadAccess)
        {
            Update();
        }
        else
        {
            DispatcherQueue.TryEnqueue(Update);
        }
    }

    private static string DeviceName(CoreSnapshot snapshot, string endpointId)
    {
        if (snapshot.Devices.FirstOrDefault(device => device.endpointId == endpointId) is { } saved)
        {
            if (!string.IsNullOrWhiteSpace(saved.localLabel))
            {
                return saved.localLabel!;
            }
            if (!string.IsNullOrWhiteSpace(saved.remoteDisplayName))
            {
                return saved.remoteDisplayName;
            }
        }

        var eligibleName = snapshot.EligibleDevices
            .FirstOrDefault(candidate => candidate.peerEndpointId == endpointId)
            ?.remoteDisplayName;
        return string.IsNullOrWhiteSpace(eligibleName) ? endpointId : eligibleName;
    }

    private static string PeerId(TargetedTransfer transfer)
    {
        return transfer.role == TargetedTransferRole.Sender
            ? transfer.receiverEndpointId
            : transfer.senderEndpointId;
    }
}
