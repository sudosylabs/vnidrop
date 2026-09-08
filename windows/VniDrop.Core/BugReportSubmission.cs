using System.Text;
using System.Text.Json;

namespace VniDrop.Core;

public sealed record PreparedBugReport(string Id, string InstallId, ReadOnlyMemory<byte> Body);

public sealed class BugReportSubmission
{
    public const int MaxRequestBytes = 256 * 1024;
    private BugReportDraft? preparedDraft;
    private PreparedBugReport? prepared;

    public PreparedBugReport Prepare(BugReportDraft draft, BugReportEnvironment environment, string profile, string installId)
    {
        if (!BugReportComposer.Validate(draft).IsValid) throw new InvalidDataException("bug_report_submit_failed");
        if (new[] { draft.WhatHappened, draft.Expected, draft.Steps }.Any(value => Encoding.UTF8.GetByteCount(value.Trim()) > 4000) ||
            Encoding.UTF8.GetByteCount(draft.Contact.Trim()) > 320)
            throw new InvalidDataException("windows_report_text_too_long");
        if (!Guid.TryParseExact(installId, "D", out var installation))
            throw new InvalidDataException("bug_report_submit_failed");
        installId = installation.ToString("D");
        // A timeout can follow successful storage. Retry the same document and ID
        // until the user edits the draft, so the service can acknowledge it once.
        if (draft == preparedDraft && prepared?.InstallId == installId) return prepared;

        var id = Guid.NewGuid().ToString("D");
        var logs = draft.IncludeLogs ? BugReportComposer.ReadRecentCoreLogs(profile) : "";
        byte[] Serialize(string includedLogs) => JsonSerializer.SerializeToUtf8Bytes(new
        {
            schemaVersion = 1,
            id,
            timestampMillis = environment.CreatedAt.ToUnixTimeMilliseconds(),
            installId,
            appVersion = Limit(environment.AppVersion, 40),
            platform = "windows-native",
            whatHappened = draft.WhatHappened.Trim(),
            expected = draft.Expected.Trim(),
            steps = draft.Steps.Trim(),
            contact = draft.Contact.Trim(),
            includeLogs = draft.IncludeLogs,
            logs = includedLogs,
            device = new
            {
                deviceName = Limit(environment.DeviceName, 128),
                deviceModel = Limit(environment.DeviceModel, 128),
                operatingSystem = Limit(environment.OperatingSystem, 192),
                network = "",
                batteryLevel = "",
            },
        });

        var body = Serialize(logs);
        if (body.Length > MaxRequestBytes)
        {
            body = Serialize("");
            var minimum = 0;
            var maximum = Encoding.UTF8.GetByteCount(logs);
            while (minimum <= maximum)
            {
                var count = minimum + (maximum - minimum) / 2;
                var candidate = Serialize(BugReportComposer.TakeUtf8Tail(logs, count));
                if (candidate.Length <= MaxRequestBytes) { body = candidate; minimum = count + 1; }
                else maximum = count - 1;
            }
        }
        if (body.Length > MaxRequestBytes) throw new InvalidDataException("windows_report_text_too_long");
        preparedDraft = draft;
        return prepared = new(id, installId, body);
    }

    public void Clear() { prepared = null; preparedDraft = null; }

    private static string Limit(string text, int maximumBytes)
    {
        var result = new StringBuilder();
        var count = 0;
        foreach (var rune in text.Trim().EnumerateRunes())
        {
            if (count + rune.Utf8SequenceLength > maximumBytes) break;
            result.Append(rune.ToString());
            count += rune.Utf8SequenceLength;
        }
        return result.ToString();
    }
}
