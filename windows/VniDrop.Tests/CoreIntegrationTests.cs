using System.Diagnostics;
using VniDrop.Core;
using VniDrop.Native;
using Xunit;

namespace VniDrop.Tests;

public sealed class CoreIntegrationTests : IAsyncLifetime
{
    private readonly string directory = Path.Combine(Path.GetTempPath(), "vnidrop-windows-tests", Guid.NewGuid().ToString("N"));
    private CoreSession sender = null!;
    private CoreSession receiver = null!;

    public async Task InitializeAsync()
    {
        Directory.CreateDirectory(directory);
        sender = new(Path.Combine(directory, "sender")); receiver = new(Path.Combine(directory, "receiver"));
        await sender.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
        await receiver.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
    }

    [Fact]
    public async Task FolderTransferPreservesBytesAndRefusesOverwriteAcrossCSharpBoundary()
    {
        var source = Path.Combine(directory, "Photos été"); Directory.CreateDirectory(source);
        var contents = Enumerable.Range(0, 8192).Select(i => (byte)(i % 251)).ToArray();
        await File.WriteAllBytesAsync(Path.Combine(source, "photo-é.bin"), contents);
        await File.WriteAllTextAsync(Path.Combine(source, "empty.txt"), "");
        var share = await sender.RunAsync(c => c.ShareFiles([new(SourceKind.Path, source, null, true)], new(41, "Photos été", "Windows", TransferAccessMode.Public)));
        GC.Collect(); GC.WaitForPendingFinalizers();
        var destination = Path.Combine(directory, "received");
        await receiver.RunAsync(c => c.Receive(share.ticket, destination, "Receiver")).WaitAsync(TimeSpan.FromSeconds(30));
        var artifacts = await receiver.RunAsync(c => c.ListReceivedArtifacts());
        var file = Assert.Single(artifacts, a => a.relativePath.EndsWith("photo-é.bin"));
        Assert.Equal(contents, await File.ReadAllBytesAsync(file.locator));
        Assert.Equal("done", Assert.Single((await receiver.SnapshotAsync()).Transfers).status);
        Assert.Contains(receiver.DrainEvents(), e => e.transferId == 41);
        await using var another = new CoreSession(Path.Combine(directory, "another"));
        await another.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
        await Assert.ThrowsAnyAsync<VnidropException>(() => another.RunAsync(c => c.Receive(share.ticket, destination, "Another")));
        Assert.Equal(contents, await File.ReadAllBytesAsync(file.locator));
    }

    [Fact]
    public async Task CancelReachesCoreWhileReceiveWaitsForApproval()
    {
        var source = Path.Combine(directory, "approval.txt"); await File.WriteAllTextAsync(source, "approval required");
        var share = await sender.RunAsync(c => c.ShareFiles([new(SourceKind.Path, source, null, false)], new(42, "Approval", null, TransferAccessMode.ApprovalRequired)));
        var receiving = receiver.RunAsync(c => c.Receive(share.ticket, Path.Combine(directory, "received"), "Receiver"));
        await UntilAsync(async () => (await sender.RunAsync(c => c.ListReceiverRequests(42))).Any(r => r.status == "requested"), "Receiver never requested approval");
        await receiver.RunAsync(c => c.CancelTransfer(42)).WaitAsync(TimeSpan.FromSeconds(10));
        await Assert.ThrowsAnyAsync<Exception>(() => receiving.WaitAsync(TimeSpan.FromSeconds(10)));
        Assert.Equal("cancelled", Assert.Single((await receiver.SnapshotAsync()).Transfers).status);
        Assert.Empty(await receiver.RunAsync(c => c.ListReceivedArtifacts()));
    }

    [Fact]
    public async Task SavedDeviceAndTargetedTransferUseCoreOwnedApprovalAndIdentity()
    {
        var source = Path.Combine(directory, "hello.txt"); await File.WriteAllTextAsync(source, "Hello from WinUI");
        var share = await sender.RunAsync(c => c.ShareFiles([new(SourceKind.Path, source, null, false)], new(43, "Hello", "Sender", TransferAccessMode.Public)));
        await receiver.RunAsync(c => c.Receive(share.ticket, Path.Combine(directory, "first"), "Receiver"));
        var receiverId = (await receiver.RunAsync(c => c.Status())).endpointId;
        var senderId = (await sender.RunAsync(c => c.Status())).endpointId;
        await UntilAsync(async () => (await sender.RunAsync(c => c.ListPairingEligibilities())).Any(e => e.peerEndpointId == receiverId), "Pairing eligibility was not published");
        Assert.True(await sender.RunAsync(c => c.RequestSavedDevicePairing(receiverId)));
        await UntilAsync(async () => (await receiver.RunAsync(c => c.ListDeviceRelationships())).Any(r => r.state == DeviceRelationshipState.PendingIncoming), "Pairing request did not arrive");
        Assert.True(await receiver.RunAsync(c => c.RespondToDevicePairing(senderId, true)));
        await UntilAsync(async () => (await sender.RunAsync(c => c.ListSavedDevices())).Length == 1, "Pairing was not persisted");
        using var preparation = await sender.RunAsync(c => c.NewTargetedTransferPreparation(receiverId));
        var transfer = await sender.RunAsync(_ => preparation.Send([new(SourceKind.Path, source, null, false)], "Direct"));
        await UntilAsync(async () => (await receiver.RunAsync(c => c.ListPendingTargetedOffers())).Any(o => o.transferId == transfer.id), "Targeted offer did not arrive");
        var outcome = await receiver.RunAsync(c => c.RespondToTargetedOffer(transfer.id, true));
        Assert.IsType<TargetedOfferResponse.Approved>(outcome);
        await receiver.RunAsync(c => c.ReceiveTargetedTransfer(transfer.id, Path.Combine(directory, "targeted"))).WaitAsync(TimeSpan.FromSeconds(30));
        Assert.Equal(TargetedTransferState.Completed, (await receiver.RunAsync(c => c.GetTargetedTransfer(transfer.id)))!.state);
        Assert.Equal("Hello from WinUI", await File.ReadAllTextAsync(Directory.GetFiles(Path.Combine(directory, "targeted"), "hello.txt", SearchOption.AllDirectories).Single()));
    }

    [Fact]
    public async Task ShutdownReleasesProfileAndRetainsProtectedIdentity()
    {
        var identity = (await sender.RunAsync(c => c.Status())).endpointId;
        var path = sender.ProfileDirectory;
        await sender.DisposeAsync();
        await using var reopened = new CoreSession(path);
        await reopened.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
        Assert.Equal(identity, (await reopened.RunAsync(c => c.Status())).endpointId);
    }

    [Fact]
    public async Task FailedPreferenceWriteRestoresPreviousRuntimeAndIdentity()
    {
        var profilePath = Path.Combine(directory, "preferences");
        await using var profile = new RuntimeProfile(profilePath);
        var preferences = new AppPreferences { RelayMode = CoreRelayMode.LocalOnly };
        await profile.InitializeAsync(preferences);
        var original = profile.Session;
        var identity = (await original.RunAsync(c => c.Status())).endpointId;
        Directory.CreateDirectory(Path.Combine(profilePath, "windows-preferences.json"));
        await Assert.ThrowsAsync<UnauthorizedAccessException>(() => profile.SavePreferencesAsync(preferences with { RelayUrls = ["https://unused.example"] }));
        Assert.NotSame(original, profile.Session);
        Assert.Equal(preferences, profile.Preferences);
        Assert.Equal(identity, (await profile.Session.RunAsync(c => c.Status())).endpointId);
    }

    [Fact]
    public async Task MaintenanceCannotStopAnAvailableShare()
    {
        await using var profile = new RuntimeProfile(Path.Combine(directory, "maintenance"));
        var preferences = new AppPreferences { RelayMode = CoreRelayMode.LocalOnly };
        await profile.InitializeAsync(preferences);
        var source = Path.Combine(directory, "keep-sharing.txt"); await File.WriteAllTextAsync(source, "keep sharing");
        await profile.Session.RunAsync(c => c.ShareFiles([new(SourceKind.Path, source, null, false)], new(44, "Keep sharing", null, TransferAccessMode.Public)));
        await Assert.ThrowsAsync<InvalidOperationException>(profile.ClearCacheAsync);
        await Assert.ThrowsAsync<InvalidOperationException>(() => profile.SavePreferencesAsync(preferences with { RelayMode = CoreRelayMode.Automatic }));
        Assert.Equal("sharing", Assert.Single((await profile.Session.SnapshotAsync()).Transfers).status);
        Assert.Equal(preferences, profile.Preferences);
    }

    private static async Task UntilAsync(Func<Task<bool>> predicate, string message)
    {
        var deadline = Stopwatch.StartNew();
        while (deadline.Elapsed < TimeSpan.FromSeconds(20))
        {
            if (await predicate()) return;
            await Task.Delay(30);
        }
        Assert.Fail(message);
    }

    public async Task DisposeAsync()
    {
        if (sender is not null) await sender.DisposeAsync();
        if (receiver is not null) await receiver.DisposeAsync();
        var cleanup = Stopwatch.StartNew();
        while (true)
        {
            try { Directory.Delete(directory, true); break; }
            catch (IOException) when (cleanup.Elapsed < TimeSpan.FromSeconds(5)) { await Task.Delay(50); }
        }
    }
}
