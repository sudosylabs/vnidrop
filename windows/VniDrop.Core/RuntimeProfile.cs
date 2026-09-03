using VniDrop.Native;

namespace VniDrop.Core;

public sealed class RuntimeProfile(string directory) : IAsyncDisposable
{
    public CoreSession Session { get; private set; } = new(directory);
    public AppPreferences Preferences { get; private set; } = new();
    private readonly SemaphoreSlim maintenance = new(1);

    public async Task InitializeAsync(AppPreferences preferences, bool resetIdentity = false)
    {
        await Session.InitializeAsync(preferences.NetworkConfiguration, resetIdentity);
        Preferences = preferences;
    }

    public async Task SavePreferencesAsync(AppPreferences preferences)
    {
        if (!await maintenance.WaitAsync(0)) throw new InvalidOperationException("windows_network_busy");
        try
        {
            var previous = Preferences;
            var changedNetwork = previous.RelayMode != preferences.RelayMode || !previous.RelayUrls.SequenceEqual(preferences.RelayUrls);
            if (changedNetwork)
            {
                await EnsureIdleAsync();
                await Session.DisposeAsync();
                try
                {
                    Session = new(Session.ProfileDirectory);
                    await Session.InitializeAsync(preferences.NetworkConfiguration);
                    await Task.Run(() => preferences.Save(Session.ProfileDirectory));
                }
                catch
                {
                    // Disk persistence and network startup form one change; either failure restores the previous policy.
                    await Session.DisposeAsync();
                    Session = new(Session.ProfileDirectory);
                    await Session.InitializeAsync(previous.NetworkConfiguration);
                    throw;
                }
            }
            else await Task.Run(() => preferences.Save(Session.ProfileDirectory));
            Preferences = preferences;
        }
        finally { maintenance.Release(); }
    }

    public async Task ClearCacheAsync()
    {
        if (!await maintenance.WaitAsync(0)) throw new InvalidOperationException("windows_network_busy");
        try
        {
            await EnsureIdleAsync();
            var profile = Session.ProfileDirectory;
            await Session.DisposeAsync();
            try { await Task.Run(() => VnidropMethods.ClearInactiveTransferCache(profile)); }
            finally
            {
                Session = new(profile);
                await Session.InitializeAsync(Preferences.NetworkConfiguration);
            }
        }
        finally { maintenance.Release(); }
    }

    private async Task EnsureIdleAsync()
    {
        var facts = await Session.RunAsync(c => c.RuntimeObligationFacts());
        if (facts.activeInvitationTransfers + facts.activeTargetedTransfers + facts.invitationProviderAvailability + facts.targetedProviderAvailability + facts.targetedPreparations > 0)
            throw new InvalidOperationException("windows_network_busy");
    }

    public ValueTask DisposeAsync() => Session.DisposeAsync();
}
