using System.Text;
using VniDrop.Core;
using VniDrop.Native;
using Xunit;

namespace VniDrop.Tests;

public class PresentationTests
{
    [Fact]
    public void InvitationRejectsMalformedUtf8AndOversizedFiles()
    {
        Assert.Throws<DecoderFallbackException>(() => InvitationDocument.Decode([0xff, 0xfe]));
        Assert.Throws<InvalidDataException>(() => InvitationDocument.Decode(new byte[InvitationDocument.MaximumBytes + 1]));
        Assert.Throws<InvalidDataException>(() => InvitationDocument.Decode(" \n"u8));
        Assert.Equal("invitation-é", InvitationDocument.Decode(Encoding.UTF8.GetBytes("invitation-é")));
    }

    [Fact]
    public void PickerCancellationAndInvalidSelectionsPreserveDraft()
    {
        var draft = new TransferDraft();
        var files = new[] { new DraftSource("a", "a", false, 1), new DraftSource("b", "b", false, 2) };
        draft.Select(files, count => $"{count} files");
        draft.Rename("My selection");
        draft.Select([], _ => "unused");
        Assert.Throws<InvalidDataException>(() => draft.Select([files[0], new("folder", "folder", true, null)], _ => "unused"));
        Assert.Equal(files, draft.Sources);
        Assert.Equal("My selection", draft.Name);
        draft.Remove(files[0], count => $"{count} files");
        Assert.Equal("My selection", draft.Name);
    }

    [Fact]
    public void AutomaticNamesFollowSelectionUntilEdited()
    {
        var draft = new TransferDraft();
        var files = new[] { new DraftSource("a", "a", false, 1), new DraftSource("b", "b", false, 2) };
        draft.Select(files, count => $"{count} files");
        Assert.Equal("2 files", draft.Name);
        draft.Remove(files[0], _ => "unused");
        Assert.Equal("b", draft.Name);
        draft.Remove(files[1], _ => "unused");
        Assert.Empty(draft.Name);
    }

    [Fact]
    public void ClearingDraftRestoresAutomaticNaming()
    {
        var draft = new TransferDraft();
        draft.Select([new("a", "a", false, 1)], _ => "unused");
        draft.Rename("Custom"); draft.Clear();
        draft.Select([new("b", "b", false, 1)], _ => "unused");
        Assert.Equal("b", draft.Name);
    }

    [Fact]
    public void ProgressAcceptsRustByteAliasesAndRejectsInvalidData()
    {
        var valid = Event("download", "progress", "{\"downloaded\":25,\"total_size\":100,\"file_name\":\"résumé.txt\"}");
        var result = TransferPresentation.Progress([valid], 7, "receive", 100);
        Assert.NotNull(result); Assert.Equal(.25, result.Fraction); Assert.Equal("progress_downloading", result.LabelKey); Assert.Equal("résumé.txt", result.FileName);
        var malformed = Event("download", "progress", "not json");
        Assert.Null(TransferPresentation.Progress([malformed, Event("lifecycle", "done", "{}")], 7, "receive", 100));
    }

    [Fact]
    public void TemporaryCleanupOnlyRecognizesVniDropPartFiles()
    {
        Assert.True(WindowsStorage.IsReceivePart(Path.Combine("x", ".photo.vnidrop-123.part")));
        Assert.False(WindowsStorage.IsReceivePart(Path.Combine("x", "photo.vnidrop-123.part")));
        Assert.False(WindowsStorage.IsReceivePart(Path.Combine("x", ".photo.vnidrop-123.txt")));
    }

    [Fact]
    public void ImportsKotlinPreferencesIncludingStrictRelayPolicy()
    {
        var bytes = Preference("username", "Élodie").Concat(Preference("receive_folder_value", "D:\\Reçus"))
            .Concat(Preference("relay_mode", "StrictCustom")).Concat(Preference("relay_urls", "https://relay.example.com"))
            .Concat(Preference("theme_mode", "Dark")).ToArray();
        var result = AppPreferences.ImportLegacy(bytes);
        Assert.Equal("Élodie", result.Username);
        Assert.Equal("D:\\Reçus", result.ReceiveDirectory);
        Assert.Equal("Dark", result.Theme);
        Assert.Equal(CoreRelayMode.StrictCustom, result.NetworkConfiguration.mode);
        Assert.Equal(["https://relay.example.com"], result.NetworkConfiguration.relayUrls);
    }

    [Fact]
    public void DamagedPreferencesNeverFallBackToPublicRelays()
    {
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy([10, 127, 8]));
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy(Preference("relay_mode", "UnknownMode")));
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy(Preference("relay_mode", "99")));
        var name = Encoding.UTF8.GetBytes("relay_mode");
        byte[] wrongType = [10, (byte)(name.Length + 6), 10, (byte)name.Length, .. name, 18, 2, 8, 1];
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy(wrongType));
    }

    [Fact]
    public void RelaySettingsRetainAndNormalizeCustomServers()
    {
        string[] retained = ["https://saved.example.com"];
        Assert.True(RelaySettingsPolicy.TryNormalize(CoreRelayMode.Automatic, [], retained, out var automatic));
        Assert.Equal(retained, automatic);

        Assert.True(RelaySettingsPolicy.TryNormalize(
            CoreRelayMode.StrictCustom,
            [" HTTPS://Relay.Example.COM:443/ ", "https://[2001:DB8::1]:8443"],
            retained,
            out var custom));
        Assert.Equal(["https://relay.example.com", "https://[2001:db8::1]:8443"], custom);

        Assert.False(RelaySettingsPolicy.TryNormalize(
            CoreRelayMode.CustomWithDirectFallback,
            ["https://relay.example.com", "https://RELAY.example.com:443/"],
            retained,
            out _));
        Assert.False(RelaySettingsPolicy.TryNormalize(CoreRelayMode.StrictCustom, ["https://relay.example.com/path"], retained, out _));
        Assert.False(RelaySettingsPolicy.TryNormalize(CoreRelayMode.StrictCustom, ["https://user@relay.example.com"], retained, out _));
        Assert.False(RelaySettingsPolicy.TryNormalize(CoreRelayMode.StrictCustom, ["http://relay.example.com"], retained, out _));
    }

    [Fact]
    public void BulkHistoryDeletionKeepsOngoingTransfers()
    {
        var ordinary = new[]
        {
            Stored(1, "importing"), Stored(2, "sharing"), Stored(3, "receiving"),
            Stored(4, "done"), Stored(5, "cancelled"), Stored(6, "stopped"), Stored(7, "failed"),
        };
        Assert.Equal([4UL, 5UL, 6UL, 7UL], TransferPresentation.DeletableHistoryIds(ordinary));

        var targeted = new[]
        {
            Targeted("preparing", TargetedTransferState.Preparing),
            Targeted("offering", TargetedTransferState.Offering),
            Targeted("awaiting", TargetedTransferState.AwaitingApproval),
            Targeted("approved", TargetedTransferState.Approved),
            Targeted("connecting", TargetedTransferState.Connecting),
            Targeted("transferring", TargetedTransferState.Transferring),
            Targeted("interrupted", TargetedTransferState.Interrupted),
            Targeted("completed", TargetedTransferState.Completed),
            Targeted("declined", TargetedTransferState.Declined),
            Targeted("cancelled", TargetedTransferState.Cancelled),
            Targeted("failed", TargetedTransferState.Failed),
            Targeted("deleted", TargetedTransferState.Deleted),
        };
        Assert.Equal(
            ["completed", "declined", "cancelled", "failed"],
            TransferPresentation.DeletableTargetedHistoryIds(targeted));
    }

    [Fact]
    public async Task ForcedRefreshWaitsForAnOverlappingRefresh()
    {
        var gate = new RefreshGate();
        var firstStarted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseFirst = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var first = gate.RunAsync(false, async () =>
        {
            firstStarted.SetResult();
            await releaseFirst.Task;
        });
        await firstStarted.Task;

        var opportunisticRan = false;
        Assert.False(await gate.RunAsync(false, () =>
        {
            opportunisticRan = true;
            return Task.CompletedTask;
        }));

        var forcedRan = false;
        var forced = gate.RunAsync(true, () =>
        {
            forcedRan = true;
            return Task.CompletedTask;
        });
        await Task.Yield();
        Assert.False(forcedRan);

        releaseFirst.SetResult();
        Assert.True(await first);
        Assert.True(await forced);
        Assert.True(forcedRan);
        Assert.False(opportunisticRan);
    }

    [Fact]
    public async Task ClosingCanWaitForMaintenanceToFinish()
    {
        var gate = new MaintenanceGate();
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var maintenance = gate.RunAsync(async () =>
        {
            started.SetResult();
            await release.Task;
        });
        await started.Task;

        var closeWait = gate.WaitAsync();
        Assert.False(closeWait.IsCompleted);
        await Assert.ThrowsAsync<InvalidOperationException>(() => gate.RunAsync(() => Task.CompletedTask));

        release.SetResult();
        await maintenance;
        await closeWait;
    }

    [Fact]
    public async Task FailedPreferenceWriteRemainsVisibleUntilTheSameFieldSucceeds()
    {
        var tracker = new PreferenceWriteTracker();
        _ = tracker.Track(
            Task.FromException(new IOException("save failed")),
            PreferenceWriteScope.Username);

        var firstFailure = await Assert.ThrowsAsync<IOException>(tracker.FlushAsync);
        Assert.Equal("save failed", firstFailure.Message);
        await Assert.ThrowsAsync<IOException>(tracker.FlushAsync);

        await tracker.Track(Task.CompletedTask, PreferenceWriteScope.Theme);
        await Assert.ThrowsAsync<IOException>(tracker.FlushAsync);

        await tracker.Track(Task.CompletedTask, PreferenceWriteScope.Username);
        await tracker.FlushAsync();
    }

    [Fact]
    public async Task OlderPreferenceFailureCannotOverrideANewerSuccessfulWrite()
    {
        var tracker = new PreferenceWriteTracker();
        var older = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _ = tracker.Track(older.Task, PreferenceWriteScope.Username);
        await tracker.Track(Task.CompletedTask, PreferenceWriteScope.Username);

        older.SetException(new IOException("obsolete failure"));
        await tracker.FlushAsync();
    }

    [Fact]
    public async Task AbandonedPreferenceLifetimeSuppressesAWriteTrackedAfterResolve()
    {
        var tracker = new PreferenceWriteTracker();
        var pageScope = PreferenceWriteScope.Username | PreferenceWriteScope.ReceiveDirectory;
        using var lifetime = new CancellationTokenSource();

        lifetime.Cancel();
        tracker.Resolve(pageScope);
        _ = tracker.Track(
            Task.FromException(new IOException("discarded failure")),
            pageScope,
            lifetime.Token);

        await tracker.FlushAsync();
    }

    [Fact]
    public async Task NavigationQueuesLatestPreferenceWriteBeforeAbandonment()
    {
        var tracker = new PreferenceWriteTracker();
        var active = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var latest = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        using var lifetime = new CancellationTokenSource();
        _ = tracker.Track(active.Task, PreferenceWriteScope.Username, lifetime.Token);
        _ = tracker.Track(latest.Task, PreferenceWriteScope.Username, lifetime.Token);

        lifetime.Cancel();
        tracker.Resolve(PreferenceWriteScope.Username);
        var flush = tracker.FlushAsync();

        Assert.False(flush.IsCompleted);
        active.SetResult();
        await Task.Yield();
        Assert.False(flush.IsCompleted);
        latest.SetException(new IOException("discarded failure"));
        await flush;
    }

    [Fact]
    public async Task AbandonedPreferenceLifetimeCannotHideANewerActiveFailure()
    {
        var tracker = new PreferenceWriteTracker();
        using var lifetime = new CancellationTokenSource();
        lifetime.Cancel();
        tracker.Resolve(PreferenceWriteScope.Username);
        _ = tracker.Track(Task.CompletedTask, PreferenceWriteScope.Username, lifetime.Token);
        _ = tracker.Track(
            Task.FromException(new IOException("active failure")),
            PreferenceWriteScope.Username);

        var failure = await Assert.ThrowsAsync<IOException>(tracker.FlushAsync);
        Assert.Equal("active failure", failure.Message);
    }

    [Fact]
    public async Task PendingPreferenceScopeRequiresACompensatingNoOpWrite()
    {
        var tracker = new PreferenceWriteTracker();
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _ = tracker.Track(pending.Task, PreferenceWriteScope.Username);

        Assert.True(tracker.HasPending(PreferenceWriteScope.Username));
        Assert.True(tracker.HasPending(PreferenceWriteScope.Username | PreferenceWriteScope.ReceiveDirectory));
        Assert.False(tracker.HasPending(PreferenceWriteScope.Theme));

        pending.SetResult();
        await tracker.FlushAsync();
        Assert.False(tracker.HasPending(PreferenceWriteScope.Username));
    }

    [Fact]
    public async Task PausedPreferenceAdmissionRejectsNewWritesUntilResumed()
    {
        var admission = new PreferenceWriteAdmission();
        var starts = 0;
        Task Start() => admission.Start(() =>
        {
            starts++;
            return Task.CompletedTask;
        });

        await Start();
        admission.Pause();
        await Assert.ThrowsAsync<InvalidOperationException>(Start);
        Assert.Equal(1, starts);

        admission.Resume();
        await Start();
        Assert.Equal(2, starts);
    }

    [Theory]
    [InlineData(true, false, false, true)]
    [InlineData(false, false, false, false)]
    [InlineData(true, true, false, false)]
    [InlineData(true, false, true, false)]
    public void WindowNavigationRequiresAnInteractiveRuntime(
        bool ready,
        bool maintaining,
        bool closing,
        bool expected)
    {
        Assert.Equal(expected, WindowInteractionPolicy.AllowsNavigation(ready, maintaining, closing));
    }

    [Fact]
    public void InvalidNativePreferencesRemainUntouched()
    {
        var directory = Path.Combine(Path.GetTempPath(), "vnidrop-preferences-test", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "windows-preferences.json");
        try
        {
            const string contents = "{\"RelayMode\":1,\"RelayUrls\":null}";
            File.WriteAllText(path, contents);
            Assert.Throws<InvalidDataException>(() => AppPreferences.Load(directory));
            Assert.Equal(contents, File.ReadAllText(path));
        }
        finally { File.Delete(path); Directory.Delete(directory); }
    }

    [Fact]
    public void ProfileArgumentsCannotBecomeInvitations()
    {
        var result = LaunchOptions.Parse(["--profile", "profile.vnd", "invitation.vnd", "invitation.vnd"]);
        Assert.Equal(Path.GetFullPath("profile.vnd"), result.Profile);
        Assert.Equal([Path.GetFullPath("invitation.vnd")], result.Invitations);
        Assert.Throws<ArgumentException>(() => LaunchOptions.Parse(["--profile"]));
    }

    [Theory]
    [InlineData(0, "fr-FR", "one")]
    [InlineData(1, "en-US", "one")]
    [InlineData(2, "en-US", "other")]
    [InlineData(1, "pl-PL", "one")]
    [InlineData(12, "pl-PL", "many")]
    [InlineData(22, "pl-PL", "few")]
    [InlineData(21, "ru-RU", "one")]
    [InlineData(11, "ru-RU", "many")]
    public void FileCountsUseLanguagePluralRules(int count, string language, string category) => Assert.Equal(category, FileCountPlural.Category((ulong)count, language));

    [Fact]
    public async Task FailedSubmissionRetainsEditableSources()
    {
        await using var session = new CoreSession(Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N")));
        var draft = new TransferDraft();
        draft.Select([new("missing", "missing", false, 1)], _ => "unused");
        draft.Rename("Keep me");
        await Assert.ThrowsAsync<InvalidOperationException>(() => draft.SubmitAsync(session, "Sender", true));
        Assert.False(draft.IsSubmitting);
        Assert.Equal("Keep me", draft.Name);
        Assert.Single(draft.Sources);
        draft.Rename("Retry");
        Assert.Equal("Retry", draft.Name);
    }

    private static byte[] Preference(string key, string value)
    {
        var name = Encoding.UTF8.GetBytes(key); var text = Encoding.UTF8.GetBytes(value);
        byte[] entry = [10, (byte)name.Length, .. name, 18, (byte)(text.Length + 2), 42, (byte)text.Length, .. text];
        return [10, (byte)entry.Length, .. entry];
    }

    private static CoreEvent Event(string phase, string kind, string data) => new("id-" + kind, 1, 1, "transfer", 7, "receive", phase, kind, data);

    private static StoredTransfer Stored(ulong id, string status) => new(
        id.ToString(), id, null, "send", status, "Transfer", null, null, 1, 1,
        TransferAccessMode.ApprovalRequired, 0, 0);

    private static TargetedTransfer Targeted(string id, TargetedTransferState state) => new(
        id, TargetedTransferRole.Sender, "sender", "receiver", "manifest", "Transfer", 1, 1, 0, state, 0, 0);
}
