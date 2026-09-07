using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Microsoft.Win32;
using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop.Views.Settings;

public sealed partial class BugReportPage : Page
{
    private bool submitting;
    private readonly BugReportSubmission submission = new();
    private BugReportConfiguration? configuration;
    private CancellationTokenSource? cancellation;

    public BugReportPage()
    {
        InitializeComponent();
        try { configuration = BugReporting.Configuration(); }
        catch { configuration = null; }
        if (configuration is null)
        {
            SubmitError.Message = Strings.Get("windows_report_unconfigured");
            SubmitError.IsOpen = true;
            SubmitButton.IsEnabled = false;
        }
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        cancellation?.Cancel();
        base.OnNavigatedFrom(e);
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private async void SubmitBugReport(object sender, RoutedEventArgs e)
    {
        if (submitting || configuration is null) return;

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
        SubmitSuccess.IsOpen = false;
        if (!validation.IsValid)
        {
            (validation.MissingWhat ? BugWhat : BugExpected).Focus(FocusState.Programmatic);
            return;
        }

        submitting = true;
        using var attempt = new CancellationTokenSource();
        cancellation = attempt;
        SetBusy(true);
        try
        {
            var context = CurrentEnvironment();
            var model = App.Window.Model;
            var installId = model.Preferences.DiagnosticsInstallId;
            if (!Guid.TryParseExact(installId, "D", out _))
            {
                installId = Guid.NewGuid().ToString("D");
                await model.SavePreferencesAsync(model.Preferences with { DiagnosticsInstallId = installId },
                    PreferenceWriteScope.DiagnosticsInstallId, attempt.Token);
            }
            var report = await Task.Run(() => submission.Prepare(draft, context, model.Session.ProfileDirectory, installId), attempt.Token);
            await new BugReportTransport(configuration).SendAsync(report, attempt.Token);
            submission.Clear();
            BugWhat.Text = BugExpected.Text = BugSteps.Text = BugContact.Text = "";
            SubmitSuccess.IsOpen = true;
            Announce(SubmitSuccess);
        }
        catch (OperationCanceledException) when (attempt.IsCancellationRequested) { }
        catch (Exception error)
        {
            SubmitError.Message = Strings.Get(error is InvalidDataException && error.Message is
                "windows_report_unconfigured" or "windows_report_text_too_long" or
                "windows_report_service_configuration" or "windows_report_rate_limited" or
                "windows_report_unconfirmed" or "windows_report_timeout" or "windows_report_connection_failed"
                    ? error.Message : "bug_report_submit_failed");
            SubmitError.IsOpen = true;
            Announce(SubmitError);
        }
        finally
        {
            cancellation = null;
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
        BugLogs.IsEnabled = !busy;
        SubmitButton.IsEnabled = !busy && configuration is not null;
        SubmitButton.Content = Strings.Get(busy ? "bug_report_submitting" : "bug_report_submit");
        Submitting.Visibility = busy ? Visibility.Visible : Visibility.Collapsed;
        Submitting.IsActive = busy;
        Submitting.IsTabStop = busy;
        if (busy)
        {
            Submitting.Focus(FocusState.Programmatic);
            Announce(Submitting);
        }
        else if (IsLoaded)
        {
            Control feedback = SubmitSuccess.IsOpen ? SubmitSuccess : SubmitError.IsOpen ? SubmitError : SubmitButton;
            feedback.Focus(FocusState.Programmatic);
            feedback.StartBringIntoView();
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
