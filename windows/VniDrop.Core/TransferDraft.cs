using System.Security.Cryptography;
using VniDrop.Native;

namespace VniDrop.Core;

public sealed record DraftSource(string Path, string Name, bool IsDirectory, long? Size);

public sealed class TransferDraft
{
    public IReadOnlyList<DraftSource> Sources { get; private set; } = [];
    public string Name { get; private set; } = "";
    public SavedDevice? Receiver { get; }
    public bool IsSubmitting { get; private set; }
    private bool automaticName = true;
    private CancellationTokenSource? cancellation;

    public void CancelPreparation() => cancellation?.Cancel();

    public TransferDraft(SavedDevice? receiver = null) => Receiver = receiver;
    public void Clear()
    {
        if (IsSubmitting) return;
        Sources = []; Name = ""; automaticName = true;
    }
    public void Rename(string name) { if (!IsSubmitting) { Name = name; automaticName = false; } }

    public void Select(IReadOnlyList<DraftSource> sources, Func<int, string> multipleName)
    {
        if (IsSubmitting || sources.Count == 0) return;
        if (sources.Any(s => s.IsDirectory) && (sources.Count != 1 || !sources[0].IsDirectory))
            throw new InvalidDataException("windows_selection_invalid");
        Sources = sources.ToArray();
        if (automaticName) Name = sources.Count == 1 ? sources[0].Name : multipleName(sources.Count);
    }

    public void Remove(DraftSource source, Func<int, string> multipleName)
    {
        if (IsSubmitting) return;
        Sources = Sources.Where(s => s != source).ToArray();
        if (automaticName) Name = Sources.Count switch { 0 => "", 1 => Sources[0].Name, _ => multipleName(Sources.Count) };
    }

    public async Task<object> SubmitAsync(CoreSession session, string sender, bool requireApproval)
    {
        if (IsSubmitting || Sources.Count == 0 || string.IsNullOrWhiteSpace(Name)) throw new InvalidOperationException("windows_selection_required");
        IsSubmitting = true;
        using var cancel = new CancellationTokenSource();
        cancellation = cancel;
        try
        {
            var sources = Sources.Select(s => new ShareSource(SourceKind.Path, s.Path, s.Name, s.IsDirectory)).ToArray();
            if (Receiver is null)
            {
                var id = BitConverter.ToUInt64(RandomNumberGenerator.GetBytes(8)) & long.MaxValue;
                id = Math.Max(id, 1);
                var work = session.RunAsync(c => c.ShareFiles(sources, new ShareMetadataInput(id, Name.Trim(), sender,
                    requireApproval ? TransferAccessMode.ApprovalRequired : TransferAccessMode.Public)));
                return await PreparationCancellation.CompleteAsync(work, () => StopInvitationAsync(session, work, id), cancel.Token);
            }
            using var preparation = await session.RunAsync(c => c.NewTargetedTransferPreparation(Receiver.endpointId));
            var sending = session.RunAsync(_ => preparation.Send(sources, Name.Trim()));
            return await PreparationCancellation.CompleteAsync(sending, () => session.RunAsync(_ => preparation.Stop()), cancel.Token);
        }
        finally
        {
            cancellation = null;
            IsSubmitting = false;
        }
    }

    private static async Task StopInvitationAsync(CoreSession session, Task work, ulong id)
    {
        // ShareFiles registers its cancellation handle after entering the native call.
        // Retain an early cancellation request until registration or completion wins.
        while (!work.IsCompleted)
        {
            try { await session.RunAsync(c => c.CancelTransfer(id)); }
            catch (VnidropException.Transfer) { }
            await Task.WhenAny(work, Task.Delay(50));
        }
        var transfer = await session.RunAsync(c => c.ListTransfers().FirstOrDefault(t => t.transferId == id));
        if (transfer?.status is "sharing" or "importing")
            await session.RunAsync(c => c.CancelTransfer(id));
    }
}
