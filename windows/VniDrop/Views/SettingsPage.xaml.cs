using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;
using SettingsViews = VniDrop.Views.Settings;

namespace VniDrop.Views;

public sealed partial class SettingsPage : Page
{
    public SettingsPage()
    {
        InitializeComponent();
        Loaded += (_, _) => UpdateContentWidth();
        SizeChanged += (_, _) => UpdateContentWidth();
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        var preferences = App.Window.Model.Preferences;
        PreferencesRow.Value = preferences.Username;
        AppearanceRow.Value = Strings.Get(preferences.Theme switch
        {
            "Light" => "appearance_light_mode",
            "Dark" => "appearance_dark_mode",
            _ => "appearance_system_mode",
        });
        NotificationsRow.Value = Strings.Get(preferences.Notifications ? "windows_on" : "windows_off");
        NetworkRow.Value = Strings.Get(preferences.RelayMode switch
        {
            CoreRelayMode.Automatic => "relay_mode_automatic",
            CoreRelayMode.StrictCustom => "relay_mode_custom",
            CoreRelayMode.CustomWithDirectFallback => "relay_mode_custom_direct_fallback",
            CoreRelayMode.LocalOnly => "relay_mode_local_only",
            _ => throw new ArgumentOutOfRangeException(),
        });
        AboutRow.Value = typeof(SettingsPage).Assembly.GetName().Version?.ToString(3) ?? Strings.Get("value_unavailable");
    }

    private void OpenPreferences(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(SettingsViews.PreferencesPage));
    private void OpenAppearance(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(SettingsViews.AppearancePage));
    private void OpenNotifications(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(SettingsViews.NotificationsPage));
    private void OpenStorage(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(SettingsViews.StoragePage));
    private void OpenNetwork(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(SettingsViews.NetworkPage));
    private void OpenAbout(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(SettingsViews.AboutPage));

    private void UpdateContentWidth()
    {
        var available = Math.Max(0, Root.ActualWidth - Root.Padding.Left - Root.Padding.Right);
        ContentColumn.Width = Math.Min(880, available);
    }
}
