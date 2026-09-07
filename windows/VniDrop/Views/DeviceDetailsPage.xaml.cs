using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Native;

namespace VniDrop.Views;

public sealed partial class DeviceDetailsPage : Page
{
    private string? endpointId;
    private bool subscribed;

    public DeviceDetailsPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
        Details.DeviceUnavailable += DeviceUnavailable;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        endpointId = e.Parameter as string;
        Update();
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
        if (string.IsNullOrWhiteSpace(endpointId))
        {
            return;
        }

        Details.ShowDevice(endpointId);
        if (App.Window.Model.Snapshot?.Devices.FirstOrDefault(device => device.endpointId == endpointId) is { } device)
        {
            Header.Title = DisplayName(device);
        }
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private void DeviceUnavailable(object? sender, EventArgs e) => App.Window.GoBack();

    private static string DisplayName(SavedDevice device)
    {
        if (!string.IsNullOrWhiteSpace(device.localLabel))
        {
            return device.localLabel!;
        }
        return string.IsNullOrWhiteSpace(device.remoteDisplayName)
            ? device.endpointId
            : device.remoteDisplayName;
    }
}
