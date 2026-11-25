using VniDrop.Native;

namespace VniDrop.Core;

public sealed class ReceiverRequestCache
{
    private readonly Dictionary<ulong, (InvitationStatus Status, InvitationReceiver[] Requests)> entries = [];

    public InvitationReceiver[] Read(
        InvitationTransfer[] transfers,
        IReadOnlySet<ulong> changed,
        Func<ulong, ReceiverRequest[]> read,
        bool force)
    {
        var sends = transfers.Where(transfer => transfer.Direction == InvitationDirection.Send).ToArray();
        var remaining = sends.Select(transfer => transfer.Value.transferId).ToHashSet();
        foreach (var id in entries.Keys.Where(id => !remaining.Contains(id)).ToArray())
            entries.Remove(id);
        foreach (var transfer in sends)
        {
            var id = transfer.Value.transferId;
            if (force || changed.Contains(id) || !entries.TryGetValue(id, out var previous) || previous.Status != transfer.Status)
                entries[id] = (transfer.Status, read(id).Select(request => new InvitationReceiver(request)).ToArray());
        }
        return sends.SelectMany(transfer => entries[transfer.Value.transferId].Requests).ToArray();
    }

    internal static bool IsInvitationProgress(CoreEvent value) =>
        value.transferId is not null &&
        value.phase is "import" or "transfer" or "download" or "export" &&
        value.kind is "copy-progress" or "outboard-progress" or "progress";
}
