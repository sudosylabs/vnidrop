using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Microsoft.Win32;
using VniDrop.Platform;

namespace VniDrop.Views.Settings;

public sealed partial class AboutPage : Page
{
    public AboutPage() => InitializeComponent();

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        VersionValue.Value = typeof(AboutPage).Assembly.GetName().Version?.ToString(3) ?? Strings.Get("value_unavailable");
        OsValue.Value = Environment.OSVersion.VersionString;
        DeviceModelValue.Value = DeviceModel();
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();
    private void OpenBugReport(object sender, RoutedEventArgs e) => App.Window.NavigateTo(typeof(BugReportPage));

    private static string DeviceModel()
    {
        try
        {
            const string bios = @"HKEY_LOCAL_MACHINE\HARDWARE\DESCRIPTION\System\BIOS";
            var manufacturer = Registry.GetValue(bios, "SystemManufacturer", null) as string;
            var product = Registry.GetValue(bios, "SystemProductName", null) as string;
            var value = string.Join(" ", new[] { manufacturer, product }
                .Where(part => !string.IsNullOrWhiteSpace(part))
                .Distinct(StringComparer.OrdinalIgnoreCase));
            return string.IsNullOrWhiteSpace(value) ? Environment.MachineName : value;
        }
        catch
        {
            return Environment.MachineName;
        }
    }
}
