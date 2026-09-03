using System.Collections.ObjectModel;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;

namespace VniDrop.ViewModels;

public sealed class AppViewModel : ObservableModel
{
    private readonly RuntimeProfile runtime;
    public CoreSession Session => runtime.Session;
    public AppPreferences Preferences => runtime.Preferences;
    public bool Maintaining { get; private set; }
    public CoreSnapshot? Snapshot { get; private set; }
    public ObservableCollection<TransferItem> Outgoing { get; } = [];
    public ObservableCollection<TransferItem> Incoming { get; } = [];
    public List<CoreEvent> Events { get; } = [];
    public event Action? Updated;
    private bool ready;
    public bool Ready { get => ready; private set => Set(ref ready, value); }
    private bool starting;
    public bool Starting { get => starting; private set => Set(ref starting, value); }
    private string error = "";
    public string Error { get => error; private set { Set(ref error, value); Changed(nameof(HasError)); } }
    public bool HasError => Error.Length > 0;
    public bool CanResetIdentity { get; private set; }
    public bool HasRequests => Snapshot is { } s && (s.Requests.Any(r => r.status == "requested") || s.Offers.Length > 0 || s.Relationships.Any(r => r.state == DeviceRelationshipState.PendingIncoming));
    private long lastRevision = -1;
    private bool refreshing;

    public AppViewModel(string profile) => runtime = new(profile);
    public async Task StartAsync(bool resetIdentity = false)
    {
        if (Starting || Ready) return;
        Starting = true; ClearError();
        try
        {
            var preferences = AppPreferences.Load(Session.ProfileDirectory);
            if (!File.Exists(Path.Combine(Session.ProfileDirectory, "windows-preferences.json")) && !File.Exists(Path.Combine(Session.ProfileDirectory, "app_preferences.preferences_pb")))
                preferences = preferences with { ReceiveDirectory = WindowsFiles.DownloadsDirectory() };
            await runtime.InitializeAsync(preferences, resetIdentity);
            CanResetIdentity = false;
            Ready = true;
            await RefreshAsync(true);
        }
        catch (Exception ex)
        {
            CanResetIdentity = ex is VnidropException.SecureStorageMissing or VnidropException.SecureStorageCorrupted;
            Report(ex); Changed(nameof(CanResetIdentity));
        }
        finally { Starting = false; }
    }

    public async Task RefreshAsync(bool force = false)
    {
        if (!Ready || refreshing || (!force && lastRevision == Session.Revision)) return;
        refreshing = true;
        var session = Session;
        var revision = session.Revision;
        try
        {
            var snapshot = await session.SnapshotAsync();
            if (!Ready || !ReferenceEquals(session, Session)) return;
            if (lastRevision < 0) Events.AddRange(await session.RunAsync(c => c.ListEvents(null)));
            Events.AddRange(session.DrainEvents());
            var uniqueEvents = Events.DistinctBy(e => e.id).OrderBy(e => e.timestamp).ThenBy(e => e.revision).ToArray();
            Events.Clear(); Events.AddRange(uniqueEvents);
            if (Events.Count > 1024) Events.RemoveRange(0, Events.Count - 1024);
            Snapshot = snapshot;
            Sync(Outgoing, snapshot.Transfers.Where(t => t.direction == "send"));
            Sync(Incoming, snapshot.Transfers.Where(t => t.direction == "receive"));
            lastRevision = revision;
            Changed(nameof(Snapshot)); Changed(nameof(HasRequests)); Updated?.Invoke();
        }
        catch (Exception ex) { Report(ex); }
        finally { refreshing = false; }
    }

    private void Sync(ObservableCollection<TransferItem> destination, IEnumerable<StoredTransfer> transfers)
    {
        var rows = transfers.OrderByDescending(t => t.createdAt).ToArray();
        var ids = rows.Select(t => t.localId).ToHashSet();
        foreach (var obsolete in destination.Where(t => !ids.Contains(t.Transfer.localId)).ToArray()) destination.Remove(obsolete);
        for (var i = 0; i < rows.Length; i++)
        {
            var row = destination.FirstOrDefault(t => t.Transfer.localId == rows[i].localId);
            if (row is null) { row = new(rows[i]); destination.Insert(i, row); }
            else if (destination.IndexOf(row) != i) destination.Move(destination.IndexOf(row), i);
            row.Update(rows[i], Events, Snapshot?.Requests ?? []);
        }
    }

    public async Task<bool> PerformAsync(Func<Task> action)
    {
        try { await action(); await RefreshAsync(true); return true; }
        catch (Exception ex) { Report(ex); return false; }
    }
    public void Report(Exception ex) => Error = Strings.Error(ex);
    public void ClearError() => Error = "";

    public async Task ClearCacheAsync()
    {
        await MaintainAsync(runtime.ClearCacheAsync);
    }

    public async Task SavePreferencesAsync(AppPreferences preferences)
    {
        await MaintainAsync(() => runtime.SavePreferencesAsync(preferences));
        Changed(nameof(Preferences));
    }

    private async Task MaintainAsync(Func<Task> operation)
    {
        if (Maintaining) throw new InvalidOperationException("windows_network_busy");
        Maintaining = true; Ready = false;
        var previous = Session;
        try { await operation(); }
        finally
        {
            Maintaining = false; Ready = Session.IsAvailable; lastRevision = -1;
            if (!ReferenceEquals(previous, Session)) Events.Clear();
            await RefreshAsync(true);
        }
    }
}
