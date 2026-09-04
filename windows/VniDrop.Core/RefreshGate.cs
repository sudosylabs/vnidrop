namespace VniDrop.Core;

public sealed class RefreshGate
{
    private readonly SemaphoreSlim gate = new(1, 1);

    public async Task<bool> RunAsync(bool waitForTurn, Func<Task> refresh)
    {
        var entered = waitForTurn
            ? await WaitAsync()
            : await gate.WaitAsync(0);
        if (!entered) return false;
        try
        {
            await refresh();
            return true;
        }
        finally { gate.Release(); }
    }

    private async Task<bool> WaitAsync()
    {
        await gate.WaitAsync();
        return true;
    }
}
