using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Windows.Storage.Pickers;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;

namespace VniDrop.Views;

public sealed partial class SettingsPage : Page
{
    private bool saving;
    private bool initializing = true;
    private WindowsStorageUsage? storage;
    private FrameworkElement[] sections = [];
    public SettingsPage()
    {
        InitializeComponent();
        sections = [PreferencesSection, AppearanceSection, NotificationsSection, StorageSection, NetworkSection, AboutSection, BugReportSection];
        var preferences = App.Window.Model.Preferences;
        Username.Text = preferences.Username; ReceiveFolder.Text = preferences.ReceiveDirectory; Notifications.IsOn = preferences.Notifications;
        Notifications.IsEnabled = NativeNotifications.Available;
        NotificationUnavailable.IsOpen = !NativeNotifications.Available;
        (preferences.Theme switch { "Light" => LightTheme, "Dark" => DarkTheme, _ => SystemTheme }).IsChecked = true;
        foreach (var mode in Enum.GetValues<CoreRelayMode>()) Relay.Items.Add(new ComboBoxItem { Tag = mode, Content = Strings.Get(mode switch
        { CoreRelayMode.Automatic => "relay_mode_automatic", CoreRelayMode.StrictCustom => "relay_mode_custom", CoreRelayMode.CustomWithDirectFallback => "relay_mode_custom_direct_fallback", CoreRelayMode.LocalOnly => "relay_mode_local_only", _ => throw new ArgumentOutOfRangeException() }) });
        Relay.SelectedIndex = (int)preferences.RelayMode; Urls.Text = string.Join(Environment.NewLine, preferences.RelayUrls);
        VersionValue.Text = typeof(SettingsPage).Assembly.GetName().Version?.ToString(3) ?? Strings.Get("value_unavailable");
        OsValue.Text = Environment.OSVersion.VersionString;
        Sections.SelectedIndex = 0;
        Loaded += async (_, _) => { initializing = false; await LoadStorageAsync(); };
        SizeChanged += (_, e) =>
        {
            var navigationWidth = e.NewSize.Width < 720 ? 190 : 240;
            NavColumn.Width = new GridLength(navigationWidth);
            ContentGrid.Width = Math.Max(280, Math.Min(720, e.NewSize.Width - navigationWidth - 24));
        };
        UpdateRelayUrls();
    }
    private void SectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (Sections.SelectedItem is not ListViewItem selected) return;
        foreach (var section in sections) section.Visibility = section.Name.StartsWith((string)selected.Tag, StringComparison.Ordinal) ? Visibility.Visible : Visibility.Collapsed;
        ContentScroll.ChangeView(null, 0, null, true);
    }
    private void Show(string name)
    {
        foreach (var section in sections) section.Visibility = section.Name.StartsWith(name, StringComparison.Ordinal) ? Visibility.Visible : Visibility.Collapsed;
        ContentScroll.ChangeView(null, 0, null, true);
    }
    private async void ChooseFolder(object sender, RoutedEventArgs e)
    {
        try { var folder = await new FolderPicker(App.Window.AppWindow.Id).PickSingleFolderAsync(); if (folder is not null) ReceiveFolder.Text = folder.Path; }
        catch (Exception ex) { App.Window.Model.Report(ex); }
    }
    private void ResetFolder(object sender, RoutedEventArgs e) => ReceiveFolder.Text = WindowsFiles.DownloadsDirectory();
    private async void SavePreferences(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(Username.Text)) { App.Window.Model.Report(new InvalidDataException("windows_name_required")); return; }
        await SaveAsync(App.Window.Model.Preferences with { Username = Username.Text.Trim(), ReceiveDirectory = ReceiveFolder.Text });
    }
    private async void ThemeChanged(object sender, RoutedEventArgs e)
    {
        if (initializing || ((RadioButton)sender).IsChecked != true) return;
        await SaveAsync(App.Window.Model.Preferences with { Theme = (string)((RadioButton)sender).Tag });
    }
    private async void NotificationsChanged(object sender, RoutedEventArgs e)
    {
        if (initializing) return;
        await SaveAsync(App.Window.Model.Preferences with { Notifications = Notifications.IsOn });
    }
    private void RelayChanged(object sender, SelectionChangedEventArgs e) { if (RelayUrls is not null) UpdateRelayUrls(); }
    private void UpdateRelayUrls()
    {
        var mode = Relay.SelectedItem is ComboBoxItem item ? (CoreRelayMode)item.Tag : CoreRelayMode.Automatic;
        RelayUrls.Visibility = mode is CoreRelayMode.StrictCustom or CoreRelayMode.CustomWithDirectFallback ? Visibility.Visible : Visibility.Collapsed;
    }
    private async void SaveNetwork(object sender, RoutedEventArgs e)
    {
        if (Relay.SelectedItem is not ComboBoxItem item) return;
        var mode = (CoreRelayMode)item.Tag;
        var urls = Urls.Text.Split('\n', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        if (mode is CoreRelayMode.StrictCustom or CoreRelayMode.CustomWithDirectFallback &&
            (urls.Length is 0 or > 8 || urls.Distinct(StringComparer.OrdinalIgnoreCase).Count() != urls.Length ||
             urls.Any(u => !Uri.TryCreate(u, UriKind.Absolute, out var uri) || uri.Scheme != "https" || !string.IsNullOrEmpty(uri.UserInfo) || !string.IsNullOrEmpty(uri.Fragment))))
        { App.Window.Model.Report(new InvalidDataException("windows_relay_invalid")); return; }
        await SaveAsync(App.Window.Model.Preferences with { RelayMode = mode, RelayUrls = mode is CoreRelayMode.Automatic or CoreRelayMode.LocalOnly ? [] : urls });
    }
    private async Task SaveAsync(AppPreferences preferences)
    {
        if (saving) return;
        saving = true; IsEnabled = false;
        try { await App.Window.Model.SavePreferencesAsync(preferences); App.Window.ApplyAppearance(); }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally { saving = false; IsEnabled = true; }
    }
    private async Task LoadStorageAsync()
    {
        if (!App.Window.Model.Ready) return;
        try
        {
            var core = await App.Window.Model.Session.RunAsync(c => c.StorageUsage());
            var artifacts = await App.Window.Model.Session.RunAsync(c => c.ListReceivedArtifacts());
            storage = await Task.Run(() => WindowsStorage.Inspect(App.Window.Model.Session.ProfileDirectory, App.Window.Model.Preferences.ReceiveDirectory, core, artifacts));
            ReceivedUsage.Text = Strings.Size(storage.Received); TransferUsage.Text = Strings.Size(storage.Transfer); AppUsage.Text = Strings.Size(storage.AppData);
            TemporaryUsage.Text = Strings.Size(storage.Temporary); TotalUsage.Text = Strings.Size(storage.Total);
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
    }
    private async void RefreshStorage(object sender, RoutedEventArgs e) => await LoadStorageAsync();
    private async void FreeSpace(object sender, RoutedEventArgs e)
    {
        try
        {
            var facts = await App.Window.Model.Session.RunAsync(c => c.RuntimeObligationFacts());
            if (facts.activeInvitationTransfers + facts.activeTargetedTransfers + facts.invitationProviderAvailability + facts.targetedProviderAvailability + facts.targetedPreparations > 0)
                throw new InvalidOperationException("windows_network_busy");
            await Task.Run(() => WindowsStorage.ReclaimTemporary(App.Window.Model.Session.ProfileDirectory, App.Window.Model.Preferences.ReceiveDirectory));
            await LoadStorageAsync();
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
    }
    private async void ClearCache(object sender, RoutedEventArgs e)
    {
        if (await App.Window.DialogAsync(Strings.Get("storage_clear_transfer_cache"), Strings.Get("storage_clear_transfer_cache_description"), Strings.Get("storage_clear_transfer_cache")) != ContentDialogResult.Primary) return;
        ClearCacheButton.IsEnabled = false;
        try { await App.Window.Model.ClearCacheAsync(); await LoadStorageAsync(); }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally { ClearCacheButton.IsEnabled = true; }
    }
    private async void DeleteAll(object sender, RoutedEventArgs e)
    {
        if (await App.Window.DialogAsync(Strings.Get("storage_delete_transfers"), Strings.Get("storage_delete_transfers_description"), Strings.Get("storage_delete_transfers")) != ContentDialogResult.Primary) return;
        DeleteAllButton.IsEnabled = false;
        try
        {
            var ids = (App.Window.Model.Snapshot?.Transfers ?? []).Select(t => t.transferId).Distinct().ToArray();
            foreach (var id in ids) await App.Window.Model.Session.RunAsync(c => c.DeleteTransfer(id));
            await App.Window.Model.RefreshAsync(true); await App.Window.Model.ClearCacheAsync(); await LoadStorageAsync();
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally { DeleteAllButton.IsEnabled = true; }
    }
    private void OpenBugReport(object sender, RoutedEventArgs e) => Show("BugReport");
    private void BackToAbout(object sender, RoutedEventArgs e) { Show("About"); Sections.SelectedIndex = 5; }
    private async void SubmitBugReport(object sender, RoutedEventArgs e)
    {
        await App.Window.ShowDialogAsync(new ContentDialog
        {
            Title = Strings.Get("about_bug_report"),
            Content = Strings.Get("value_unavailable"),
            CloseButtonText = Strings.Get("button_close"),
        });
    }
}
