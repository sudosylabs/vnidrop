using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop.Views.Settings;

public sealed partial class NotificationsPage : Page
{
    private bool initializing = true;
    private bool saving;

    public NotificationsPage()
    {
        InitializeComponent();
        Render();
        initializing = false;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        initializing = true;
        Render();
        initializing = false;
    }

    private void Render()
    {
        Notifications.IsOn = App.Window.Model.Preferences.Notifications;
        Notifications.IsEnabled = NativeNotifications.Available && !saving;
        NotificationUnavailable.IsOpen = !NativeNotifications.Available;
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private async void NotificationsChanged(object sender, RoutedEventArgs e)
    {
        if (initializing || saving) return;
        saving = true;
        Notifications.IsEnabled = false;
        try { await App.Window.Model.SavePreferencesAsync(App.Window.Model.Preferences with { Notifications = Notifications.IsOn }); }
        catch (Exception ex)
        {
            App.Window.Model.Report(ex);
            initializing = true;
            Render();
            initializing = false;
            App.Window.Model.DiscardPreferenceWriteFailure(PreferenceWriteScope.Notifications);
        }
        finally
        {
            saving = false;
            Notifications.IsEnabled = NativeNotifications.Available;
        }
    }
}
