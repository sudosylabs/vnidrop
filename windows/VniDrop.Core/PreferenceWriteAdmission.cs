namespace VniDrop.Core;

public sealed class PreferenceWriteAdmission
{
    private readonly object sync = new();
    private bool paused;

    public Task Start(Func<Task> operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        lock (sync)
        {
            return paused
                ? Task.FromException(new InvalidOperationException("windows_operation_failed"))
                : operation();
        }
    }

    public void Pause()
    {
        lock (sync) paused = true;
    }

    public void Resume()
    {
        lock (sync) paused = false;
    }
}
