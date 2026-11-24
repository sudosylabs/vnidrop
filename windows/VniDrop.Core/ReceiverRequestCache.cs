using VniDrop.Native;

namespace VniDrop.Core;

public sealed class ReceiverRequestCache
{
    private readonly Dictionary<ulong, (string Status, ReceiverRequest[] Requests)> entries = [];

    public ReceiverRequest[] Read(
        StoredTransfer[] transfers,
        IReadOnlySet<ulong> changed,
        Func<ulong, ReceiverRequest[]> read,
        bool force)
    {
        var sends = transfers.Where(transfer => transfer.direction == "send").ToArray();
        var remaining = sends.Select(transfer => transfer.transferId).ToHashSet();
        foreach (var id in entries.Keys.Where(id => !remaining.Contains(id)).ToArray())
            entries.Remove(id);
        foreach (var transfer in sends)
        {
            var id = transfer.transferId;
            if (force || changed.Contains(id) || !entries.TryGetValue(id, out var previous) || previous.Status != transfer.status)
                entries[id] = (transfer.status, read(id));
        }
        return sends.SelectMany(transfer => entries[transfer.transferId].Requests).ToArray();
    }

    internal static bool IsInvitationProgress(CoreEvent value) =>
        value.transferId is not null &&
        value.phase is "import" or "transfer" or "download" or "export" &&
        value.kind is "copy-progress" or "outboard-progress" or "progress";
}
