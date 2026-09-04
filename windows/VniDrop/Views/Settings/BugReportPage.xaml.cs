using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Win32;
using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop.Views.Settings;

public sealed partial class BugReportPage : Page
{
    private bool submitting;

    public BugReportPage() => InitializeComponent();

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private async void SubmitBugReport(object sender, RoutedEventArgs e)
    {
        if (submitting) return;

        var draft = new BugReportDraft(
            BugWhat.Text,
            BugExpected.Text,
            BugSteps.Text,
            BugContact.Text,
            BugLogs.IsOn);
        var validation = BugReportComposer.Validate(draft);
        SetValidationError(BugWhat, BugWhatError, validation.MissingWhat);
        SetValidationError(BugExpected, BugExpectedError, validation.MissingExpected);
        SubmitError.IsOpen = false;
        if (!validation.IsValid)
        {
            (validation.MissingWhat ? BugWhat : BugExpected).Focus(FocusState.Programmatic);
            return;
        }

        submitting = true;
        SetBusy(true);
        try
        {
            var context = CurrentEnvironment();
            var profile = App.Window.Model.Session.ProfileDirectory;
            var report = await Task.Run(() => BugReportComposer.Compose(draft, context, profile));
            await App.Window.NativeShare.ShowTextAsync(
                Strings.Get("about_bug_report"),
                Strings.Get("windows_bug_report_description"),
                report.Text);
        }
        catch
        {
            SubmitError.IsOpen = true;
        }
        finally
        {
            submitting = false;
            SetBusy(false);
        }
    }

    private void RequiredTextChanged(object sender, TextChangedEventArgs e)
    {
        if (ReferenceEquals(sender, BugWhat) && !string.IsNullOrWhiteSpace(BugWhat.Text))
            SetValidationError(BugWhat, BugWhatError, visible: false);
        if (ReferenceEquals(sender, BugExpected) && !string.IsNullOrWhiteSpace(BugExpected.Text))
            SetValidationError(BugExpected, BugExpectedError, visible: false);
    }

    private static void SetValidationError(TextBox input, TextBlock error, bool visible)
    {
        error.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        AutomationProperties.SetHelpText(input, visible ? error.Text : string.Empty);
        if (visible) Announce(error);
    }

    private void SetBusy(bool busy)
    {
        BugWhat.IsEnabled = BugExpected.IsEnabled = BugSteps.IsEnabled = BugContact.IsEnabled = !busy;
        BugLogs.IsEnabled = SubmitButton.IsEnabled = !busy;
        Submitting.Visibility = busy ? Visibility.Visible : Visibility.Collapsed;
        Submitting.IsActive = busy;
        Submitting.IsTabStop = busy;
        if (busy)
        {
            Submitting.Focus(FocusState.Programmatic);
            Announce(Submitting);
        }
        else
        {
            SubmitButton.Focus(FocusState.Programmatic);
        }
    }

    private static void Announce(FrameworkElement element)
    {
        var peer = FrameworkElementAutomationPeer.FromElement(element)
            ?? FrameworkElementAutomationPeer.CreatePeerForElement(element);
        peer?.RaiseAutomationEvent(AutomationEvents.LiveRegionChanged);
    }

    private static BugReportEnvironment CurrentEnvironment()
    {
        var version = typeof(BugReportPage).Assembly.GetName().Version?.ToString(3) ?? "Unknown";
        return new(
            version,
            RuntimeInformation.OSDescription,
            Environment.MachineName,
            DeviceModel(),
            $"OS {RuntimeInformation.OSArchitecture}; process {RuntimeInformation.ProcessArchitecture}",
            DateTimeOffset.UtcNow);
    }

    private static string DeviceModel()
    {
        try
        {
            const string bios = @"HKEY_LOCAL_MACHINE\HARDWARE\DESCRIPTION\System\BIOS";
            var manufacturer = Registry.GetValue(bios, "SystemManufacturer", null) as string;
            var product = Registry.GetValue(bios, "SystemProductName", null) as string;
            var model = string.Join(" ", new[] { manufacturer, product }
                .Where(value => !string.IsNullOrWhiteSpace(value))
                .Distinct(StringComparer.OrdinalIgnoreCase));
            return string.IsNullOrWhiteSpace(model) ? Environment.MachineName : model;
        }
        catch
        {
            return Environment.MachineName;
        }
    }
}
