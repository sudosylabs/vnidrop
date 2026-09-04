using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;

namespace VniDrop.Views.Settings;

public sealed partial class AppearancePage : Page
{
    private bool initializing = true;
    private bool saving;

    public AppearancePage()
    {
        InitializeComponent();
        SelectCurrentTheme();
        initializing = false;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        initializing = true;
        SelectCurrentTheme();
        initializing = false;
    }

    private void SelectCurrentTheme() =>
        (App.Window.Model.Preferences.Theme switch
        {
            "Light" => LightTheme,
            "Dark" => DarkTheme,
            _ => SystemTheme,
        }).IsChecked = true;

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private async void ThemeChanged(object sender, RoutedEventArgs e)
    {
        if (initializing || saving || sender is not RadioButton { IsChecked: true } selected) return;
        saving = true;
        SetChoicesEnabled(false);
        try
        {
            await App.Window.Model.SavePreferencesAsync(App.Window.Model.Preferences with { Theme = (string)selected.Tag });
            App.Window.ApplyAppearance();
        }
        catch (Exception ex)
        {
            App.Window.Model.Report(ex);
            initializing = true;
            SelectCurrentTheme();
            initializing = false;
            App.Window.Model.DiscardPreferenceWriteFailure(PreferenceWriteScope.Theme);
        }
        finally
        {
            saving = false;
            SetChoicesEnabled(true);
        }
    }

    private void SetChoicesEnabled(bool enabled)
    {
        SystemTheme.IsEnabled = LightTheme.IsEnabled = DarkTheme.IsEnabled = enabled;
    }
}
