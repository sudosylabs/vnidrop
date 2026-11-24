using VniDrop.Native;

namespace VniDrop.Core;

public sealed class TransferEventHistory
{
    private const int RecentLimit = 512;
    private const int ActivityLimit = 128;
    private readonly Dictionary<ulong, TransferHistory> transfers = [];
    private readonly EventWindow endpointEvents = new(RecentLimit);

    public void Add(CoreEvent entry)
    {
        if (entry.transferId is not { } id)
        {
            endpointEvents.Add(entry);
            return;
        }
        if (!transfers.TryGetValue(id, out var history))
        {
            history = new TransferHistory();
            transfers.Add(id, history);
        }
        (TransferPresentation.ActivityKey(entry) is null ? history.Recent : history.Activity).Add(entry);
    }

    public void AddRange(IEnumerable<CoreEvent> entries)
    {
        foreach (var entry in entries) Add(entry);
    }

    public CoreEvent[] ForTransfer(ulong id, bool includeEndpointEvents = false)
    {
        if (!transfers.TryGetValue(id, out var history)) return [];
        return InOrder(includeEndpointEvents ? history.Events.Concat(endpointEvents.Events) : history.Events);
    }

    public void Remove(ulong id) => transfers.Remove(id);

    public CoreEvent[] Drain()
    {
        var entries = InOrder(endpointEvents.Events.Concat(transfers.Values.SelectMany(history => history.Events)));
        Clear();
        return entries;
    }

    public void Clear()
    {
        transfers.Clear();
        endpointEvents.Clear();
    }

    public static CoreEvent[] InOrder(IEnumerable<CoreEvent> events) =>
        events.DistinctBy(entry => entry.id).OrderBy(entry => entry.timestamp).ThenBy(entry => entry.revision).ToArray();

    private sealed class TransferHistory
    {
        public EventWindow Activity { get; } = new(ActivityLimit);
        public EventWindow Recent { get; } = new(RecentLimit);
        public IEnumerable<CoreEvent> Events => Activity.Events.Concat(Recent.Events);
    }

    private sealed class EventWindow(int limit)
    {
        private readonly Queue<CoreEvent> events = new();
        private readonly HashSet<string> ids = [];
        public IEnumerable<CoreEvent> Events => events;

        public void Add(CoreEvent entry)
        {
            if (!ids.Add(entry.id)) return;
            events.Enqueue(entry);
            if (events.Count > limit) ids.Remove(events.Dequeue().id);
        }

        public void Clear()
        {
            events.Clear();
            ids.Clear();
        }
    }
}
