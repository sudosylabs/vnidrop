using VniDrop.Native;

namespace VniDrop.Core;

public sealed record CoreSnapshot(
    RuntimeStatus Status,
    RuntimeObligationFacts Obligations,
    InvitationTransfer[] Transfers,
    SavedDevice[] Devices,
    DeviceRelationship[] Relationships,
    PairingEligibilitySummary[] EligibleDevices,
    PendingTargetedOffer[] Offers,
    TargetedTransfer[] TargetedTransfers,
    InvitationReceiver[] Requests,
    string[] BlockedDevices);

public sealed class CoreSession : IAsyncDisposable, CoreEventSink
{
    private readonly object gate = new();
    private readonly object eventGate = new();
    private readonly object snapshotGate = new();
    private readonly TransferEventHistory events = new();
    private readonly ReceiverRequestCache receiverRequests = new();
    private readonly HashSet<ulong> receiverChanges = [];
    private VnidropCore? core;
    private bool stopping;
    private int activeCalls;
    private long revision;
    private long snapshotRevision;
    private long lastSnapshotRevision = -1;
    private CoreSnapshot? lastSnapshot;
    private TaskCompletionSource drained = CompletedSource();
    private Task? closing;

    public long Revision => Interlocked.Read(ref revision);
    public string ProfileDirectory { get; }
    public bool IsAvailable { get { lock (gate) return core is not null && !stopping; } }

    public CoreSession(string profileDirectory) => ProfileDirectory = Path.GetFullPath(profileDirectory);

    public async Task InitializeAsync(CoreNetworkConfig configuration, bool resetIdentity = false)
    {
        lock (gate)
        {
            if (core is not null || activeCalls != 0 || stopping) throw new InvalidOperationException("Core is already open.");
            activeCalls++;
            drained = new(TaskCreationOptions.RunContinuationsAsynchronously);
        }
        try
        {
            var initialized = await Task.Run(() => resetIdentity
                ? VnidropCore.ResetUnrecoverableIdentityWithLimitsAndNetworkConfig(ProfileDirectory, this, VnidropMethods.DefaultCoreLimits(), configuration)
                : VnidropCore.InitializeWithLimitsAndNetworkConfig(ProfileDirectory, this, VnidropMethods.DefaultCoreLimits(), configuration));
            lock (gate) core = initialized;
            Interlocked.Increment(ref revision);
        }
        finally { EndCall(); }
    }

    public Task<T> RunAsync<T>(Func<VnidropCore, T> operation)
    {
        VnidropCore instance;
        lock (gate)
        {
            if (stopping || core is null) throw new InvalidOperationException("Core is not available.");
            instance = core;
            if (activeCalls++ == 0) drained = new(TaskCreationOptions.RunContinuationsAsynchronously);
        }
        // Blocking receives and cancellation must run concurrently. A serial queue can deadlock approval and cancellation.
        return Task.Run(() =>
        {
            try { return operation(instance); }
            finally { EndCall(); }
        });
    }

    public Task RunAsync(Action<VnidropCore> operation) => RunAsync(c => { operation(c); return true; });

    public Task<CoreSnapshot> SnapshotAsync(bool force = true) => RunAsync(c =>
    {
        lock (snapshotGate)
        {
            long version;
            HashSet<ulong> changed;
            lock (eventGate)
            {
                version = snapshotRevision;
                if (!force && lastSnapshot is { } cached && lastSnapshotRevision == version) return cached;
                changed = [.. receiverChanges];
            }
            var transfers = c.ListTransfers().Select(transfer => new InvitationTransfer(transfer)).ToArray();
            var updated = new CoreSnapshot(c.Status(), c.RuntimeObligationFacts(), transfers,
                c.ListSavedDevices(), c.ListDeviceRelationships(), c.ListPairingEligibilities(),
                c.ListPendingTargetedOffers(), c.ListTargetedTransfers(),
                receiverRequests.Read(transfers, changed, c.ListReceiverRequests, force),
                c.ListBlockedDevices());
            lock (eventGate)
            {
                if (snapshotRevision == version) receiverChanges.ExceptWith(changed);
            }
            lastSnapshotRevision = version;
            return lastSnapshot = updated;
        }
    });

    public void OnEvent(CoreEvent value)
    {
        lock (eventGate)
        {
            events.Add(value);
            if (!ReceiverRequestCache.IsInvitationProgress(value))
            {
                snapshotRevision++;
                if (value.transferId is { } id && value.phase is "approval" or "access" or "delivery")
                    receiverChanges.Add(id);
            }
        }
        Interlocked.Increment(ref revision);
    }

    public CoreEvent[] DrainEvents()
    {
        lock (eventGate) return events.Drain();
    }

    private void EndCall()
    {
        lock (gate)
        {
            if (--activeCalls == 0) drained.TrySetResult();
        }
    }

    public ValueTask DisposeAsync()
    {
        lock (gate)
        {
            stopping = true;
            closing ??= CloseAsync();
            return new ValueTask(closing);
        }
    }

    private async Task CloseAsync()
    {
        VnidropCore? instance;
        Task pending;
        lock (gate) { instance = core; pending = drained.Task; }
        if (instance is not null) await Task.Run(instance.Shutdown);
        await pending;
        // Initialization may have completed after shutdown was requested.
        lock (gate) { instance = core; core = null; }
        if (instance is not null) await Task.Run(() => { instance.Shutdown(); instance.Dispose(); });
    }

    private static TaskCompletionSource CompletedSource()
    {
        var source = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        source.SetResult();
        return source;
    }
}
