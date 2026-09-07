using System.Text;
using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class BugReportDocumentTests : IDisposable
{
    private readonly string profileDirectory = Path.Combine(
        Path.GetTempPath(),
        "vnidrop-bug-report-tests",
        Guid.NewGuid().ToString("N"));

    public BugReportDocumentTests() => Directory.CreateDirectory(profileDirectory);

    [Theory]
    [InlineData("", "Expected result", true, false)]
    [InlineData(" \t\r\n", "Expected result", true, false)]
    [InlineData("Observed result", "", false, true)]
    [InlineData("Observed result", " \t\r\n", false, true)]
    [InlineData("Observed result", "Expected result", false, false)]
    public void ValidationRequiresObservedAndExpectedResults(
        string whatHappened,
        string expected,
        bool missingWhat,
        bool missingExpected)
    {
        var validation = BugReportComposer.Validate(new(
            whatHappened,
            expected,
            Steps: "",
            Contact: "",
            IncludeLogs: false));

        Assert.Equal(missingWhat, validation.MissingWhat);
        Assert.Equal(missingExpected, validation.MissingExpected);
        Assert.Equal(!missingWhat && !missingExpected, validation.IsValid);
    }

    [Fact]
    public void ComposeIncludesEveryDraftAndEnvironmentField()
    {
        var createdAt = new DateTimeOffset(2026, 9, 4, 20, 10, 30, TimeSpan.FromHours(2));
        var draft = new BugReportDraft(
            WhatHappened: "  The transfer stopped at 63%.  ",
            Expected: "  The transfer should complete.  ",
            Steps: "  1. Start a transfer.\n2. Disconnect Wi-Fi.  ",
            Contact: "  reporter@example.test  ",
            IncludeLogs: false);
        var environment = new BugReportEnvironment(
            AppVersion: "  0.4.2  ",
            OperatingSystem: "  Windows 11 Pro  ",
            DeviceName: "  TEST-DESKTOP  ",
            DeviceModel: "  Example Model 9000  ",
            Architecture: "  X64  ",
            CreatedAt: createdAt);

        var document = BugReportComposer.Compose(draft, environment, profileDirectory);

        Assert.Contains("Created (UTC): " + createdAt.ToUniversalTime().ToString("O"), document.Text);
        Assert.Contains("What happened?" + Environment.NewLine + "The transfer stopped at 63%.", document.Text);
        Assert.Contains("What did you expect?" + Environment.NewLine + "The transfer should complete.", document.Text);
        Assert.Contains("Steps to reproduce (optional)" + Environment.NewLine + "1. Start a transfer.\n2. Disconnect Wi-Fi.", document.Text);
        Assert.Contains("Contact email (optional)" + Environment.NewLine + "reporter@example.test", document.Text);
        Assert.Contains("App version: 0.4.2", document.Text);
        Assert.Contains("Operating system: Windows 11 Pro", document.Text);
        Assert.Contains("Device name: TEST-DESKTOP", document.Text);
        Assert.Contains("Device model: Example Model 9000", document.Text);
        Assert.Contains("Process architecture: X64", document.Text);
        Assert.Contains("Recent logs requested: No", document.Text);
        Assert.Equal(0, document.IncludedLogBytes);
    }

    [Fact]
    public void DisabledLogCollectionNeverIncludesProfileLogContents()
    {
        var logsDirectory = Directory.CreateDirectory(Path.Combine(profileDirectory, "logs")).FullName;
        const string privateLogMarker = "private-log-marker-that-must-stay-out";
        File.WriteAllText(Path.Combine(logsDirectory, "vnidrop.log"), privateLogMarker);

        var document = BugReportComposer.Compose(Draft(includeLogs: false), EnvironmentDetails(), profileDirectory);

        Assert.DoesNotContain(privateLogMarker, document.Text);
        Assert.DoesNotContain("Recent core logs", document.Text);
        Assert.Contains("Recent logs requested: No", document.Text);
        Assert.Equal(0, document.IncludedLogBytes);
    }

    [Fact]
    public void LogCollectionReadsOnlyStandardCoreLogsDirectlyUnderLogsDirectory()
    {
        var logsDirectory = Directory.CreateDirectory(Path.Combine(profileDirectory, "logs")).FullName;
        WriteLog(logsDirectory, "vnidrop.log", "accepted-current-log");
        WriteLog(logsDirectory, "vnidrop.1.log", "accepted-rotated-log");
        WriteLog(logsDirectory, "VNIDROP.42.LOG", "accepted-case-insensitive-log");

        WriteLog(logsDirectory, "vnidrop-debug.log", "rejected-nonnumeric-suffix");
        WriteLog(logsDirectory, "vnidrop.latest.log", "rejected-word-suffix");
        WriteLog(logsDirectory, "vnidrop.log.bak", "rejected-backup-extension");
        WriteLog(logsDirectory, "another.log", "rejected-other-log");
        WriteLog(profileDirectory, "vnidrop.log", "rejected-profile-root-log");
        var nestedDirectory = Directory.CreateDirectory(Path.Combine(logsDirectory, "nested")).FullName;
        WriteLog(nestedDirectory, "vnidrop.2.log", "rejected-nested-log");

        var document = BugReportComposer.Compose(Draft(includeLogs: true), EnvironmentDetails(), profileDirectory);

        Assert.Contains("accepted-current-log", document.Text);
        Assert.Contains("accepted-rotated-log", document.Text);
        Assert.Contains("accepted-case-insensitive-log", document.Text);
        Assert.DoesNotContain("rejected-nonnumeric-suffix", document.Text);
        Assert.DoesNotContain("rejected-word-suffix", document.Text);
        Assert.DoesNotContain("rejected-backup-extension", document.Text);
        Assert.DoesNotContain("rejected-other-log", document.Text);
        Assert.DoesNotContain("rejected-profile-root-log", document.Text);
        Assert.DoesNotContain("rejected-nested-log", document.Text);
    }

    [Fact]
    public void IncludedLogsRedactTicketsIdentifiersPathsAndUris()
    {
        var logsDirectory = Directory.CreateDirectory(Path.Combine(profileDirectory, "logs")).FullName;
        const string vndTicket = "vnd1:private-ticket-material-1234567890";
        const string namedTicket = "abcdefghijklmnopqrstuvwxyz0123456789";
        const string endpointId = "AbCdEfGhIjKlMnOpQrStUvWxYz012345";
        const string persistedEndpoint = "vndaddr1:private-address-material-1234567890";
        const string longHex = "0123456789abcdef0123456789abcdef0123456789abcdef";
        const string windowsPath = @"C:\Users\Alice\Documents\private-file.txt";
        const string uncPath = @"\\fileserver\private-share\private-file.txt";
        const string unixPath = "/home/alice/private/private-file.txt";
        const string relativePath = "Private folder/medical record.pdf";
        const string fileName = "tax return.pdf";
        const string privateUri = "https://relay.example.test/private?token=secret";
        var contents = string.Join(Environment.NewLine,
        [
            "invitation " + vndTicket,
            "ticket=" + namedTicket,
            "endpoint_id=" + endpointId,
            "persisted_endpoint " + persistedEndpoint,
            "digest " + longHex,
            "windows_path " + windowsPath,
            "unc_path " + uncPath,
            "unix_path " + unixPath,
            "relativegardener=public relative_path=" + relativePath + " status=failed",
            "file_name=\"" + fileName + "\" status=failed",
            "relay " + privateUri,
        ]);
        File.WriteAllText(Path.Combine(logsDirectory, "vnidrop.log"), contents);

        var document = BugReportComposer.Compose(Draft(includeLogs: true), EnvironmentDetails(), profileDirectory);

        Assert.Contains("[redacted-ticket]", document.Text);
        Assert.Contains("[redacted-endpoint]", document.Text);
        Assert.Contains("[redacted-hex]", document.Text);
        Assert.Contains("[redacted-path]", document.Text);
        Assert.Contains("[redacted-uri]", document.Text);
        Assert.DoesNotContain(vndTicket, document.Text);
        Assert.DoesNotContain(namedTicket, document.Text);
        Assert.DoesNotContain(endpointId, document.Text);
        Assert.DoesNotContain(persistedEndpoint, document.Text);
        Assert.DoesNotContain(longHex, document.Text);
        Assert.DoesNotContain(windowsPath, document.Text);
        Assert.DoesNotContain(uncPath, document.Text);
        Assert.DoesNotContain(unixPath, document.Text);
        Assert.DoesNotContain(relativePath, document.Text);
        Assert.DoesNotContain(fileName, document.Text);
        Assert.DoesNotContain(privateUri, document.Text);
        Assert.Contains("relativegardener=public", document.Text);
    }

    [Fact]
    public void MultibyteLogsStayWithinByteLimitAndRetainRecentTail()
    {
        var logsDirectory = Directory.CreateDirectory(Path.Combine(profileDirectory, "logs")).FullName;
        var contents = "very-old-prefix" + Environment.NewLine
            + string.Concat(Enumerable.Repeat("🙂é", 50_000)) + Environment.NewLine
            + "recent-tail-marker";
        File.WriteAllText(Path.Combine(logsDirectory, "vnidrop.log"), contents, new UTF8Encoding(false));

        var document = BugReportComposer.Compose(Draft(includeLogs: true), EnvironmentDetails(), profileDirectory);
        var logHeading = "Recent core logs" + Environment.NewLine;
        var logStart = document.Text.IndexOf(logHeading, StringComparison.Ordinal);

        Assert.True(logStart >= 0);
        var includedLogs = document.Text[(logStart + logHeading.Length)..];
        Assert.Equal(Encoding.UTF8.GetByteCount(includedLogs), document.IncludedLogBytes);
        Assert.InRange(document.IncludedLogBytes, BugReportComposer.MaxLogBytes - 4, BugReportComposer.MaxLogBytes);
        Assert.DoesNotContain("very-old-prefix", includedLogs);
        Assert.DoesNotContain('\uFFFD', includedLogs);
        Assert.EndsWith("recent-tail-marker", includedLogs);
    }

    private static BugReportDraft Draft(bool includeLogs) => new(
        WhatHappened: "A transfer failed.",
        Expected: "The transfer should complete.",
        Steps: "Start the transfer.",
        Contact: "",
        IncludeLogs: includeLogs);

    private static BugReportEnvironment EnvironmentDetails() => new(
        AppVersion: "0.4.2",
        OperatingSystem: "Windows",
        DeviceName: "TEST-DESKTOP",
        DeviceModel: "Test model",
        Architecture: "X64",
        CreatedAt: new DateTimeOffset(2026, 9, 4, 18, 0, 0, TimeSpan.Zero));

    private static void WriteLog(string directory, string name, string contents) =>
        File.WriteAllText(Path.Combine(directory, name), contents);

    public void Dispose()
    {
        if (Directory.Exists(profileDirectory)) Directory.Delete(profileDirectory, recursive: true);
    }
}
