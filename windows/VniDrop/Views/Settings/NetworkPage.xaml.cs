using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;

namespace VniDrop.Views.Settings;

public sealed partial class NetworkPage : Page
{
    private readonly ObservableCollection<RelayUrlEntry> relayUrls = [];
    private bool initializing = true;
    private bool saving;

    public NetworkPage()
    {
        InitializeComponent();
        UrlList.ItemsSource = relayUrls;
        LoadPreferences();
        initializing = false;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        initializing = true;
        LoadPreferences();
        initializing = false;
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        if (!saving)
        {
            App.Window.Model.DiscardPreferenceWriteFailure(
                PreferenceWriteScope.RelayMode | PreferenceWriteScope.RelayUrls);
        }
        base.OnNavigatedFrom(e);
    }

    private void LoadPreferences()
    {
        var preferences = App.Window.Model.Preferences;
        RadioFor(preferences.RelayMode).IsChecked = true;
        relayUrls.Clear();
        foreach (var url in preferences.RelayUrls) relayUrls.Add(new(url));
        if (relayUrls.Count == 0) relayUrls.Add(new(""));
        var endpointId = App.Window.Model.Snapshot?.Status.endpointId;
        DeviceId.Text = string.IsNullOrWhiteSpace(endpointId)
            ? Strings.Get("value_unavailable")
            : Strings.Format("approval_endpoint_id", ("deviceId", endpointId));
        Render();
    }

    private RadioButton RadioFor(CoreRelayMode mode) => mode switch
    {
        CoreRelayMode.Automatic => Automatic,
        CoreRelayMode.StrictCustom => StrictCustom,
        CoreRelayMode.CustomWithDirectFallback => CustomWithDirectFallback,
        CoreRelayMode.LocalOnly => LocalOnly,
        _ => throw new ArgumentOutOfRangeException(nameof(mode)),
    };

    private CoreRelayMode SelectedMode()
    {
        var tag = ModeChoices.Children.OfType<RadioButton>().FirstOrDefault(option => option.IsChecked == true)?.Tag as string;
        return Enum.TryParse<CoreRelayMode>(tag, out var mode) ? mode : CoreRelayMode.Automatic;
    }

    private string[] SelectedUrls() => relayUrls
        .Select(entry => entry.Url.Trim())
        .Where(url => url.Length > 0)
        .ToArray();

    private void ModeChanged(object sender, RoutedEventArgs e)
    {
        if (RelayUrls is not null) Render();
    }

    private void UrlChanged(object sender, TextChangedEventArgs e)
    {
        if (sender is TextBox { Tag: RelayUrlEntry entry } input) entry.Url = input.Text;
        if (!initializing) UpdateApplyState();
    }

    private void AddUrl(object sender, RoutedEventArgs e)
    {
        if (saving || relayUrls.Count >= RelaySettingsPolicy.MaximumUrls) return;
        relayUrls.Add(new(""));
        UpdateApplyState();
    }

    private void RemoveUrl(object sender, RoutedEventArgs e)
    {
        if (saving || sender is not FrameworkElement { Tag: RelayUrlEntry entry }) return;
        relayUrls.Remove(entry);
        if (relayUrls.Count == 0) relayUrls.Add(new(""));
        UpdateApplyState();
    }

    private void Render()
    {
        var mode = SelectedMode();
        RelayUrls.Visibility = mode is CoreRelayMode.StrictCustom or CoreRelayMode.CustomWithDirectFallback ? Visibility.Visible : Visibility.Collapsed;
        StrictWarning.IsOpen = mode == CoreRelayMode.StrictCustom;
        AddRelay.IsEnabled = !saving && relayUrls.Count < RelaySettingsPolicy.MaximumUrls;
        ModeDescription.Text = Strings.Get(mode switch
        {
            CoreRelayMode.Automatic => "relay_mode_automatic_description",
            CoreRelayMode.StrictCustom => "relay_mode_custom_description",
            CoreRelayMode.CustomWithDirectFallback => "relay_mode_custom_direct_fallback_description",
            CoreRelayMode.LocalOnly => "relay_mode_local_only_description",
            _ => throw new ArgumentOutOfRangeException(),
        });
        UpdateApplyState();
    }

    private void UpdateApplyState()
    {
        if (ApplyNetwork is null) return;
        var current = App.Window.Model.Preferences;
        var mode = SelectedMode();
        var valid = RelaySettingsPolicy.TryNormalize(mode, SelectedUrls(), current.RelayUrls, out var urls);
        ApplyNetwork.IsEnabled = !saving && (!valid || mode != current.RelayMode ||
            !urls.SequenceEqual(current.RelayUrls, StringComparer.Ordinal));
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private async void SaveNetwork(object sender, RoutedEventArgs e)
    {
        if (saving) return;
        var mode = SelectedMode();
        var current = App.Window.Model.Preferences;
        if (!RelaySettingsPolicy.TryNormalize(mode, SelectedUrls(), current.RelayUrls, out var urls))
        {
            App.Window.Model.Report(new InvalidDataException("windows_relay_invalid"));
            return;
        }

        saving = true;
        SetChoicesEnabled(false);
        UrlList.IsEnabled = false;
        AddRelay.IsEnabled = false;
        ApplyNetwork.Content = Strings.Get("relay_applying");
        Applying.Visibility = Visibility.Visible;
        Applying.IsActive = true;
        UpdateApplyState();
        try
        {
            await App.Window.Model.SavePreferencesAsync(current with
            {
                RelayMode = mode,
                RelayUrls = urls,
            }, PreferenceWriteScope.RelayMode | PreferenceWriteScope.RelayUrls);
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally
        {
            saving = false;
            SetChoicesEnabled(true);
            UrlList.IsEnabled = true;
            ApplyNetwork.Content = Strings.Get("relay_apply");
            Applying.IsActive = false;
            Applying.Visibility = Visibility.Collapsed;
            UpdateApplyState();
        }
    }

    private void SetChoicesEnabled(bool enabled)
    {
        Automatic.IsEnabled = StrictCustom.IsEnabled = CustomWithDirectFallback.IsEnabled = LocalOnly.IsEnabled = enabled;
    }
}

public sealed class RelayUrlEntry(string url)
{
    public string Url { get; set; } = url;
}
