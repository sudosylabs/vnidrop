namespace VniDrop.Core;

public sealed class MaintenanceGate
{
    private readonly object sync = new();
    private Task current = Task.CompletedTask;

    public Task RunAsync(Func<Task> maintenance)
    {
        TaskCompletionSource activation;
        Task operation;
        lock (sync)
        {
            if (!current.IsCompleted) throw new InvalidOperationException("windows_network_busy");
            activation = new(TaskCreationOptions.RunContinuationsAsynchronously);
            operation = RunCoreAsync(maintenance, activation.Task);
            current = operation;
        }

        activation.SetResult();
        return operation;
    }

    public Task WaitAsync()
    {
        lock (sync) return current;
    }

    private static async Task RunCoreAsync(Func<Task> maintenance, Task activation)
    {
        await activation;
        await maintenance();
    }
}
