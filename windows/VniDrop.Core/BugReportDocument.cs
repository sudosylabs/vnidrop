using System.Text;
using System.Text.RegularExpressions;

namespace VniDrop.Core;

public sealed record BugReportDraft(
    string WhatHappened,
    string Expected,
    string Steps,
    string Contact,
    bool IncludeLogs);

public sealed record BugReportEnvironment(
    string AppVersion,
    string OperatingSystem,
    string DeviceName,
    string DeviceModel,
    string Architecture,
    DateTimeOffset CreatedAt);

public sealed record BugReportValidation(bool MissingWhat, bool MissingExpected)
{
    public bool IsValid => !MissingWhat && !MissingExpected;
}

public sealed record BugReportDocument(string Text, int IncludedLogBytes);

public static class BugReportComposer
{
    public const int MaxLogBytes = 192 * 1024;
    private const int MaxRawLogBytes = MaxLogBytes * 4;
    private static readonly UTF8Encoding Utf8 = new(false, false);
    private static readonly Regex CoreLogName = new(
        @"^vnidrop(?:\.\d+)?\.log$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant | RegexOptions.Compiled);
    private static readonly (Regex Pattern, string Replacement)[] RedactionRules =
    [
        (Pattern("""\bvnd1:[^\s"'<>]+""", RegexOptions.IgnoreCase), "[redacted-ticket]"),
        (Pattern("""\bvndaddr1:[^\s"'<>]+""", RegexOptions.IgnoreCase), "[redacted-endpoint]"),
        (Pattern("""(?<prefix>\b(?:blob[_-]?ticket|ticket|invitation)\b["']?\s*[:=]\s*["']?)(?<value>[^\s,"'<>}\]]{24,})""", RegexOptions.IgnoreCase), "${prefix}[redacted-ticket]"),
        (Pattern("""(?<prefix>\b(?:(?:peer|sender|receiver|from|remote|local)[_-]?)?(?:endpoint|device|node)[_-]?id\b["']?\s*[:=]\s*["']?)(?<value>[A-Za-z0-9+/=_-]{16,})""", RegexOptions.IgnoreCase), "${prefix}[redacted-endpoint]"),
        (Pattern(@"\b[0-9a-f]{32,}\b", RegexOptions.IgnoreCase), "[redacted-hex]"),
        (Pattern(
            """(?<prefix>\b(?:file[_-]?(?:name|path)|relative[_-]?path|path)\b["']?\s*[:=]\s*)(?:(?<quote>["'])(?:[^"'\r\n]*)\k<quote>|.*?(?=(?:\s+\b[A-Za-z_][A-Za-z0-9_-]*\b\s*=)|[,}\]\r\n]|$))""",
            RegexOptions.IgnoreCase),
            "${prefix}${quote}[redacted-path]${quote}"),
        (Pattern("""\b[a-z][a-z0-9+.-]{1,31}://[^\s<>"']+""", RegexOptions.IgnoreCase), "[redacted-uri]"),
        (Pattern("""(?<![A-Za-z0-9_])(?:[A-Za-z]:\\|\\\\)[^\r\n\t"'<>]+"""), "[redacted-path]"),
        (Pattern("""(?<![A-Za-z0-9_])/(?!/)(?:[^\s/:"'<>]+/)*[^\s:"'<>]+"""), "[redacted-path]"),
    ];

    public static BugReportValidation Validate(BugReportDraft draft) => new(
        string.IsNullOrWhiteSpace(draft.WhatHappened),
        string.IsNullOrWhiteSpace(draft.Expected));

    public static BugReportDocument Compose(
        BugReportDraft draft,
        BugReportEnvironment environment,
        string profileDirectory)
    {
        ArgumentNullException.ThrowIfNull(draft);
        ArgumentNullException.ThrowIfNull(environment);
        ArgumentException.ThrowIfNullOrWhiteSpace(profileDirectory);

        var logs = draft.IncludeLogs ? ReadRecentCoreLogs(profileDirectory) : "";
        var report = new StringBuilder();
        report.AppendLine("VniDrop bug report");
        report.Append("Created (UTC): ").AppendLine(environment.CreatedAt.ToUniversalTime().ToString("O"));
        AppendSection(report, "What happened?", draft.WhatHappened);
        AppendSection(report, "What did you expect?", draft.Expected);
        AppendSection(report, "Steps to reproduce (optional)", draft.Steps);
        AppendSection(report, "Contact email (optional)", draft.Contact);

        report.AppendLine("Device information");
        AppendValue(report, "App version", environment.AppVersion);
        AppendValue(report, "Operating system", environment.OperatingSystem);
        AppendValue(report, "Device name", environment.DeviceName);
        AppendValue(report, "Device model", environment.DeviceModel);
        AppendValue(report, "Process architecture", environment.Architecture);
        AppendValue(report, "Recent logs requested", draft.IncludeLogs ? "Yes" : "No");

        if (draft.IncludeLogs)
        {
            report.AppendLine().AppendLine("Recent core logs");
            report.AppendLine(logs.Length == 0 ? "(No recent core logs were available.)" : logs);
        }

        return new(report.ToString().TrimEnd(), Utf8.GetByteCount(logs));
    }

    public static string RedactLogs(string input)
    {
        if (string.IsNullOrEmpty(input)) return input;
        var result = input;
        foreach (var (pattern, replacement) in RedactionRules)
            result = pattern.Replace(result, replacement);
        return result;
    }

    private static string ReadRecentCoreLogs(string profileDirectory)
    {
        DirectoryInfo logDirectory;
        try
        {
            logDirectory = new(Path.GetFullPath(Path.Combine(profileDirectory, "logs")));
            if (!logDirectory.Exists || logDirectory.Attributes.HasFlag(FileAttributes.ReparsePoint)) return "";
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException)
        {
            return "";
        }

        FileInfo[] files;
        try
        {
            files = logDirectory.EnumerateFiles("*", SearchOption.TopDirectoryOnly)
                .Where(file => CoreLogName.IsMatch(file.Name) && !file.Attributes.HasFlag(FileAttributes.ReparsePoint))
                .OrderByDescending(file => file.LastWriteTimeUtc)
                .ThenBy(file => file.Name, StringComparer.OrdinalIgnoreCase)
                .ToArray();
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return "";
        }

        var remaining = MaxRawLogBytes;
        var chunks = new List<string>();
        foreach (var file in files)
        {
            if (remaining == 0) break;
            try
            {
                using var stream = new FileStream(file.FullName, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
                var requested = (int)Math.Min(stream.Length, remaining);
                if (requested == 0) continue;
                stream.Seek(-requested, SeekOrigin.End);
                var bytes = new byte[requested];
                var read = 0;
                while (read < bytes.Length)
                {
                    var count = stream.Read(bytes, read, bytes.Length - read);
                    if (count == 0) break;
                    read += count;
                }
                if (read == 0) continue;
                var start = 0;
                while (start < read && (bytes[start] & 0xC0) == 0x80) start++;
                var text = Utf8.GetString(bytes, start, read - start);
                chunks.Add($"--- {file.Name} ---{Environment.NewLine}{text.TrimEnd()}");
                remaining -= read;
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                // A rotating log can disappear between enumeration and reading.
            }
        }

        chunks.Reverse();
        return TakeUtf8Tail(RedactLogs(string.Join(Environment.NewLine, chunks)), MaxLogBytes);
    }

    private static string TakeUtf8Tail(string value, int maximumBytes)
    {
        var bytes = Utf8.GetBytes(value);
        if (bytes.Length <= maximumBytes) return value;
        var start = bytes.Length - maximumBytes;
        while (start < bytes.Length && (bytes[start] & 0xC0) == 0x80) start++;
        return Utf8.GetString(bytes, start, bytes.Length - start);
    }

    private static Regex Pattern(string expression, RegexOptions options = RegexOptions.None) =>
        new(expression, options | RegexOptions.CultureInvariant | RegexOptions.Compiled);

    private static void AppendSection(StringBuilder report, string heading, string value)
    {
        report.AppendLine().AppendLine(heading).AppendLine(value.Trim());
    }

    private static void AppendValue(StringBuilder report, string label, string value) =>
        report.Append(label).Append(": ").AppendLine(value.Trim());
}
