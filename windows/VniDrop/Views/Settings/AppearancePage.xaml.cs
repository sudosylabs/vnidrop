using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Platform;

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

    private void SelectCurrentTheme()
    {
        ThemeChoices.SelectedIndex = App.Window.Model.Preferences.Theme switch
        {
            "Light" => 1,
            "Dark" => 2,
            _ => 0,
        };
        RenderDescription();
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private async void ThemeChanged(object sender, SelectionChangedEventArgs e)
    {
        RenderDescription();
        if (initializing || saving || ThemeChoices.SelectedIndex < 0) return;
        var theme = ThemeChoices.SelectedIndex switch
        {
            1 => "Light",
            2 => "Dark",
            _ => "System",
        };
        saving = true;
        SetChoicesEnabled(false);
        try
        {
            await App.Window.Model.SavePreferencesAsync(App.Window.Model.Preferences with { Theme = theme });
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

    private void SetChoicesEnabled(bool enabled) => ThemeChoices.IsEnabled = enabled;

    private void RenderDescription()
    {
        if (ThemeDescription is not null)
            ThemeDescription.Visibility = ThemeChoices.SelectedIndex == 0 ? Visibility.Visible : Visibility.Collapsed;
    }
}
