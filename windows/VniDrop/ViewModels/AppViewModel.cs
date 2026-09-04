using System.Collections.ObjectModel;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;

namespace VniDrop.ViewModels;

public sealed class AppViewModel : ObservableModel
{
    private readonly RuntimeProfile runtime;
    private readonly SemaphoreSlim preferenceSave = new(1, 1);
    private readonly PreferenceWriteAdmission preferenceWriteAdmission = new();
    private readonly PreferenceWriteTracker preferenceWrites = new();
    private readonly MaintenanceGate maintenance = new();
    private readonly object targetedReceiveGate = new();
    private readonly Dictionary<string, Task> targetedReceives = [];
    public CoreSession Session => runtime.Session;
    public AppPreferences Preferences => runtime.Preferences;
    private bool maintaining;
    public bool Maintaining { get => maintaining; private set => Set(ref maintaining, value); }
    public CoreSnapshot? Snapshot { get; private set; }
    public ObservableCollection<TransferItem> Outgoing { get; } = [];
    public ObservableCollection<TransferItem> Incoming { get; } = [];
    public List<CoreEvent> Events { get; } = [];
    public event Action? Updated;
    public event Action? TargetedReceiveChanged;
    private bool ready;
    public bool Ready { get => ready; private set => Set(ref ready, value); }
    private bool starting;
    public bool Starting { get => starting; private set => Set(ref starting, value); }
    private string error = "";
    public string Error { get => error; private set { Set(ref error, value); Changed(nameof(HasError)); } }
    public bool HasError => Error.Length > 0;
    public bool CanResetIdentity { get; private set; }
    public bool HasRequests => Snapshot is { } snapshot && PairingPromptPolicy.HasPending(
        snapshot.Requests,
        snapshot.Offers,
        snapshot.Relationships,
        snapshot.EligibleDevices);
    private long lastRevision = -1;
    private readonly RefreshGate refreshGate = new();

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
        if (!Ready || (!force && lastRevision == Session.Revision)) return;
        await refreshGate.RunAsync(force, async () =>
        {
            if (!Ready || (!force && lastRevision == Session.Revision)) return;
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
        });
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

    public bool IsTargetedReceiveRunning(string transferId)
    {
        lock (targetedReceiveGate) return targetedReceives.ContainsKey(transferId);
    }

    public bool StartTargetedReceive(string transferId, bool resume)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(transferId);
        var activation = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        var operation = RunTargetedReceiveAsync(
            transferId,
            resume,
            Preferences.ReceiveDirectory,
            activation.Task);
        lock (targetedReceiveGate)
        {
            if (targetedReceives.ContainsKey(transferId))
            {
                activation.SetResult(false);
                return false;
            }
            targetedReceives.Add(transferId, operation);
        }

        TargetedReceiveChanged?.Invoke();
        activation.SetResult(true);
        return true;
    }

    private async Task RunTargetedReceiveAsync(
        string transferId,
        bool resume,
        string destination,
        Task<bool> activation)
    {
        if (!await activation)
        {
            return;
        }

        try
        {
            await PerformAsync(() => Session.RunAsync(core =>
            {
                if (resume)
                {
                    core.ResumeTargetedTransfer(transferId, destination);
                }
                else
                {
                    core.ReceiveTargetedTransfer(transferId, destination);
                }
            }));
        }
        finally
        {
            lock (targetedReceiveGate) targetedReceives.Remove(transferId);
            TargetedReceiveChanged?.Invoke();
        }
    }

    public void Report(Exception ex) => Error = Strings.Error(ex);
    public void ClearError() => Error = "";

    public async Task ClearCacheAsync()
    {
        await MaintainAsync(runtime.ClearCacheAsync);
    }

    public Task SavePreferencesAsync(
        AppPreferences preferences,
        PreferenceWriteScope explicitScope = PreferenceWriteScope.None,
        CancellationToken abandonmentToken = default) =>
        preferenceWriteAdmission.Start(() => SavePreferencesAdmitted(
            preferences,
            explicitScope,
            abandonmentToken));

    private Task SavePreferencesAdmitted(
        AppPreferences preferences,
        PreferenceWriteScope explicitScope,
        CancellationToken abandonmentToken)
    {
        if (abandonmentToken.IsCancellationRequested)
            return Task.CompletedTask;
        var current = Preferences;
        var changedScope = ChangedPreferenceScope(current, preferences);
        var mustFollowPendingWrite = explicitScope != PreferenceWriteScope.None
            && preferenceWrites.HasPending(explicitScope);
        if (changedScope == PreferenceWriteScope.None && !mustFollowPendingWrite)
        {
            if (!abandonmentToken.IsCancellationRequested)
                preferenceWrites.Resolve(explicitScope);
            return Task.CompletedTask;
        }
        var scope = explicitScope | changedScope;
        var operation = SavePreferencesCoreAsync(preferences, scope);
        return preferenceWrites.Track(operation, scope, abandonmentToken);
    }

    public Task FlushPreferenceWritesAsync() => preferenceWrites.FlushAsync();
    public void PausePreferenceWrites() => preferenceWriteAdmission.Pause();
    public void ResumePreferenceWrites() => preferenceWriteAdmission.Resume();
    public void DiscardPreferenceWriteFailure(PreferenceWriteScope scope) => preferenceWrites.Resolve(scope);

    public async Task WaitForMaintenanceAsync()
    {
        try { await maintenance.WaitAsync(); }
        catch { }
    }

    private async Task SavePreferencesCoreAsync(AppPreferences requested, PreferenceWriteScope scope)
    {
        await preferenceSave.WaitAsync();
        try
        {
            var current = Preferences;
            var preferences = current with
            {
                Username = HasScope(scope, PreferenceWriteScope.Username) ? requested.Username : current.Username,
                ReceiveDirectory = HasScope(scope, PreferenceWriteScope.ReceiveDirectory) ? requested.ReceiveDirectory : current.ReceiveDirectory,
                Theme = HasScope(scope, PreferenceWriteScope.Theme) ? requested.Theme : current.Theme,
                Notifications = HasScope(scope, PreferenceWriteScope.Notifications) ? requested.Notifications : current.Notifications,
                RelayMode = HasScope(scope, PreferenceWriteScope.RelayMode) ? requested.RelayMode : current.RelayMode,
                RelayUrls = HasScope(scope, PreferenceWriteScope.RelayUrls) ? requested.RelayUrls : current.RelayUrls,
                DiagnosticsInstallId = HasScope(scope, PreferenceWriteScope.DiagnosticsInstallId)
                    ? requested.DiagnosticsInstallId : current.DiagnosticsInstallId,
            };
            var networkChanged = current.RelayMode != preferences.RelayMode
                || !current.RelayUrls.SequenceEqual(preferences.RelayUrls);
            if (networkChanged) await MaintainAsync(() => runtime.SavePreferencesAsync(preferences));
            else await runtime.SavePreferencesAsync(preferences);
            Changed(nameof(Preferences));
        }
        finally { preferenceSave.Release(); }
    }

    private static PreferenceWriteScope ChangedPreferenceScope(AppPreferences current, AppPreferences requested)
    {
        var scope = PreferenceWriteScope.None;
        if (current.Username != requested.Username) scope |= PreferenceWriteScope.Username;
        if (current.ReceiveDirectory != requested.ReceiveDirectory) scope |= PreferenceWriteScope.ReceiveDirectory;
        if (current.Theme != requested.Theme) scope |= PreferenceWriteScope.Theme;
        if (current.Notifications != requested.Notifications) scope |= PreferenceWriteScope.Notifications;
        if (current.RelayMode != requested.RelayMode) scope |= PreferenceWriteScope.RelayMode;
        if (!current.RelayUrls.SequenceEqual(requested.RelayUrls)) scope |= PreferenceWriteScope.RelayUrls;
        if (current.DiagnosticsInstallId != requested.DiagnosticsInstallId) scope |= PreferenceWriteScope.DiagnosticsInstallId;
        return scope;
    }

    private static bool HasScope(PreferenceWriteScope scope, PreferenceWriteScope field) => (scope & field) == field;

    private Task MaintainAsync(Func<Task> operation) => maintenance.RunAsync(() => MaintainCoreAsync(operation));

    private async Task MaintainCoreAsync(Func<Task> operation)
    {
        Maintaining = true; Ready = false;
        var previous = Session;
        try { await operation(); }
        finally
        {
            Ready = Session.IsAvailable; lastRevision = -1;
            if (!ReferenceEquals(previous, Session)) Events.Clear();
            try { await RefreshAsync(true); }
            finally { Maintaining = false; }
        }
    }
}
