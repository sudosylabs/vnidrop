using System.Text.Json;
using VniDrop.Native;

namespace VniDrop.Core;

public sealed record TransferProgress(double? Fraction, string LabelKey, string? FileName = null);

public static class TransferPresentation
{
    public static ulong[] DeletableHistoryIds(IEnumerable<StoredTransfer> transfers) => transfers
        .Where(transfer => transfer.status is "done" or "cancelled" or "stopped" or "failed")
        .Select(transfer => transfer.transferId)
        .Distinct()
        .ToArray();

    public static string[] DeletableTargetedHistoryIds(IEnumerable<TargetedTransfer> transfers) => transfers
        .Where(transfer => transfer.state is TargetedTransferState.Completed
            or TargetedTransferState.Declined
            or TargetedTransferState.Cancelled
            or TargetedTransferState.Failed)
        .Select(transfer => transfer.id)
        .Distinct(StringComparer.Ordinal)
        .ToArray();

    public static TransferProgress? Progress(IReadOnlyList<CoreEvent> events, ulong id, string direction, ulong size)
    {
        var latest = events.LastOrDefault(e => e.transferId == id && e.direction == direction && e.phase is "import" or "network" or "handshake" or "download" or "export" or "lifecycle");
        if (latest is null || latest.kind is "done" or "failed" or "cancelled" or "share-stopped") return null;
        var data = Data(latest);
        var done = Number(data, "exported", "downloaded", "offset", "end_offset", "transferred", "written");
        var total = Number(data, "file_size", "total_size", "size", "total") ?? size;
        var label = latest.phase switch
        {
            "import" => "progress_preparing", "handshake" => "progress_requesting_access", "network" => "progress_connecting",
            "download" => latest.kind == "found-collection" ? "progress_getting_ready" : "progress_downloading",
            "export" => "progress_saving", _ => "progress_working"
        };
        return new(done is not null && total > 0 ? Math.Clamp(done.Value / total, 0, 1) : null, label, Text(data, "file_name"));
    }

    public static TransferProgress? ReceiverProgress(IReadOnlyList<CoreEvent> events, ulong id, string endpoint, ulong totalSize)
    {
        var parsed = events.Select(e => (Event: e, Data: Data(e))).ToArray();
        var connections = parsed.Where(p => Text(p.Data, "endpoint_id") == endpoint).Select(p => Text(p.Data, "connection_id")).Where(c => c is not null).ToHashSet();
        var relevant = parsed.Where(p => p.Event.transferId == id && p.Event.direction == "send" && p.Event.phase == "transfer" &&
            p.Event.kind is "started" or "progress" or "completed" or "aborted" &&
            (Text(p.Data, "endpoint_id") is { } peer ? peer == endpoint : Text(p.Data, "connection_id") is { } connection && connections.Contains(connection))).ToArray();
        if (relevant.Length == 0) return null;
        if (relevant[^1].Event.kind == "aborted") return new(null, "progress_interrupted");
        var requests = new Dictionary<string, (double Offset, double? Size, bool Aborted)>();
        double? connectionOffset = null;
        double? connectionSize = null;
        foreach (var (e, data) in relevant)
        {
            var offset = Number(data, "end_offset", "offset", "transferred");
            var size = Number(data, "size");
            if (Text(data, "request_id") is not { } request) { connectionOffset = offset ?? connectionOffset; connectionSize = size ?? connectionSize; continue; }
            var key = Text(data, "connection_id") + ":" + request;
            requests.TryGetValue(key, out var previous);
            size ??= previous.Size;
            requests[key] = (e.kind == "completed" ? size ?? previous.Offset : Math.Max(previous.Offset, offset ?? 0), size, e.kind == "aborted");
        }
        var active = requests.Values.Where(r => !r.Aborted).ToArray();
        var done = requests.Count > 0 ? active.Sum(r => r.Offset) : connectionOffset;
        var total = totalSize > 0 ? totalSize : requests.Count > 0 ? active.Sum(r => r.Size ?? 0) : connectionSize ?? 0;
        double? fraction = done is not null && total > 0 ? Math.Clamp(done.Value / total, 0, 1) : null;
        return new(fraction, relevant[^1].Event.kind == "completed" && fraction is >= .999 ? "progress_completed" : "progress_sending");
    }

    public static string? ActivityKey(CoreEvent e) => (e.phase, e.kind) switch
    {
        ("import", "started") => "transfer_event_preparing", ("ticket", "created") => "transfer_event_ready",
        ("network", "connecting" or "connected") => "transfer_event_connecting", ("download", "found-collection") => "transfer_event_downloading",
        (_, "receiver-requested") => "transfer_event_requested", (_, "receiver-accepted" or "receiver-auto-approved") => "transfer_event_approved",
        (_, "receiver-refused") => "transfer_event_refused", (_, "receiver-completed") or ("lifecycle", "done") => "transfer_event_completed",
        (_, "share-stopped") or ("lifecycle", "cancelled") => "transfer_event_stopped", (_, "failed") => "transfer_event_failed", _ => null
    };

    private static JsonElement Data(CoreEvent e)
    {
        try { using var data = JsonDocument.Parse(e.dataJson); return data.RootElement.Clone(); }
        catch (JsonException) { return default; }
    }
    private static string? Text(JsonElement data, string key) => data.ValueKind == JsonValueKind.Object && data.TryGetProperty(key, out var value) && value.ValueKind is JsonValueKind.String or JsonValueKind.Number ? value.ToString() : null;
    private static double? Number(JsonElement data, params string[] keys)
    {
        foreach (var key in keys)
            if (data.ValueKind == JsonValueKind.Object && data.TryGetProperty(key, out var value) && value.ValueKind == JsonValueKind.Number && value.TryGetDouble(out var number) && double.IsFinite(number) && number >= 0) return number;
        return null;
    }
}
