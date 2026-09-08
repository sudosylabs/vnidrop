using System.Text.Json;
using VniDrop.Core;

namespace VniDrop.Platform;

internal static class BugReporting
{
    public static BugReportConfiguration? Configuration()
    {
        var endpoint = Environment.GetEnvironmentVariable("VNIDROP_DIAGNOSTICS_ENDPOINT");
        var key = Environment.GetEnvironmentVariable("VNIDROP_DIAGNOSTICS_INGEST_KEY");
        if (endpoint is not null || key is not null)
            return BugReportConfiguration.Create(endpoint ?? "", key ?? "");
        using var stream = typeof(BugReporting).Assembly.GetManifestResourceStream("VniDrop.Diagnostics.json")
            ?? throw new InvalidDataException("windows_report_unconfigured");
        using var configuration = JsonDocument.Parse(stream);
        return BugReportConfiguration.Create(
            configuration.RootElement.GetProperty("endpoint").GetString() ?? "",
            configuration.RootElement.GetProperty("ingestKey").GetString() ?? "");
    }
}
