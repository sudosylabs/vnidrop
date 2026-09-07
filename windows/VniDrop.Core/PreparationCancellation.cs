namespace VniDrop.Core;

public static class PreparationCancellation
{
    public static async Task<T> CompleteAsync<T>(Task<T> work, Func<Task> stop, CancellationToken cancellation)
    {
        try
        {
            var result = await work.WaitAsync(cancellation);
            if (!cancellation.IsCancellationRequested) return result;
        }
        catch (Exception) when (cancellation.IsCancellationRequested) { }

        // Keep the native handle alive until both cancellation and the blocking call finish.
        try { await stop(); }
        finally
        {
            try { await work; }
            catch (Exception) { }
        }
        throw new OperationCanceledException(cancellation);
    }
}
