using System.Runtime.ExceptionServices;

namespace VniDrop.Core;

[Flags]
public enum PreferenceWriteScope
{
    None = 0,
    Username = 1 << 0,
    ReceiveDirectory = 1 << 1,
    Theme = 1 << 2,
    Notifications = 1 << 3,
    RelayMode = 1 << 4,
    RelayUrls = 1 << 5,
    DiagnosticsInstallId = 1 << 6,
}

public sealed class PreferenceWriteTracker
{
    private readonly object sync = new();
    private readonly Dictionary<Task, PendingWrite> pending = [];
    private readonly Dictionary<PreferenceWriteScope, long> latestSuccessfulVersions = [];
    private readonly Dictionary<PreferenceWriteScope, RetainedFailure> retainedFailures = [];
    private long nextVersion;

    public Task Track(
        Task operation,
        PreferenceWriteScope scope,
        CancellationToken abandonmentToken = default)
    {
        ArgumentNullException.ThrowIfNull(operation);
        if (scope == PreferenceWriteScope.None)
        {
            throw new ArgumentOutOfRangeException(nameof(scope));
        }
        long version;
        lock (sync)
        {
            version = ++nextVersion;
            pending.Add(operation, new(version, scope, abandonmentToken));
        }

        _ = ObserveAsync(operation, version, scope, abandonmentToken);
        return operation;
    }

    public void Resolve(PreferenceWriteScope scope)
    {
        if (scope == PreferenceWriteScope.None)
        {
            return;
        }

        lock (sync)
        {
            var version = ++nextVersion;
            foreach (var field in Fields(scope))
            {
                latestSuccessfulVersions[field] = version;
                retainedFailures.Remove(field);
            }
        }
    }

    public bool HasPending(PreferenceWriteScope scope)
    {
        if (scope == PreferenceWriteScope.None)
        {
            return false;
        }

        lock (sync)
        {
            return pending.Values.Any(write => (write.Scope & scope) != PreferenceWriteScope.None);
        }
    }

    public async Task FlushAsync()
    {
        while (true)
        {
            KeyValuePair<Task, PendingWrite>[] operations;
            Exception? failure;
            lock (sync)
            {
                operations = pending.ToArray();
                failure = retainedFailures.Count == 0
                    ? null
                    : retainedFailures.Values.MaxBy(item => item.Version).Error;
            }

            if (operations.Length == 0)
            {
                if (failure is not null)
                {
                    ExceptionDispatchInfo.Capture(failure).Throw();
                }
                return;
            }

            try
            {
                await Task.WhenAll(operations.Select(entry => entry.Key));
            }
            catch
            {
                // Complete below records the newest failure for this write generation.
            }

            foreach (var operation in operations)
            {
                Complete(
                    operation.Key,
                    operation.Value.Version,
                    operation.Value.Scope,
                    operation.Value.AbandonmentToken);
            }
        }
    }

    private async Task ObserveAsync(
        Task operation,
        long version,
        PreferenceWriteScope scope,
        CancellationToken abandonmentToken)
    {
        try
        {
            await operation;
        }
        catch
        {
            // FlushAsync reports failures and keeps them until a newer write succeeds.
        }
        finally
        {
            Complete(operation, version, scope, abandonmentToken);
        }
    }

    private void Complete(
        Task operation,
        long version,
        PreferenceWriteScope scope,
        CancellationToken abandonmentToken)
    {
        if (!operation.IsCompleted)
        {
            return;
        }

        lock (sync)
        {
            pending.Remove(operation);
            if (abandonmentToken.IsCancellationRequested)
            {
                return;
            }
            if (operation.IsCompletedSuccessfully)
            {
                foreach (var field in Fields(scope))
                {
                    if (!latestSuccessfulVersions.TryGetValue(field, out var successfulVersion)
                        || version > successfulVersion)
                    {
                        latestSuccessfulVersions[field] = version;
                    }
                    if (retainedFailures.TryGetValue(field, out var failure)
                        && version >= failure.Version)
                    {
                        retainedFailures.Remove(field);
                    }
                }
                return;
            }

            var error = operation.Exception?.InnerException
                ?? (Exception?)operation.Exception
                ?? new TaskCanceledException(operation);
            foreach (var field in Fields(scope))
            {
                if (latestSuccessfulVersions.TryGetValue(field, out var successfulVersion)
                    && version <= successfulVersion)
                {
                    continue;
                }
                if (!retainedFailures.TryGetValue(field, out var failure)
                    || version >= failure.Version)
                {
                    retainedFailures[field] = new(version, error);
                }
            }
        }
    }

    private static IEnumerable<PreferenceWriteScope> Fields(PreferenceWriteScope scope)
    {
        foreach (var field in Enum.GetValues<PreferenceWriteScope>())
        {
            if (field != PreferenceWriteScope.None && (scope & field) == field)
            {
                yield return field;
            }
        }
    }

    private readonly record struct PendingWrite(
        long Version,
        PreferenceWriteScope Scope,
        CancellationToken AbandonmentToken);
    private readonly record struct RetainedFailure(long Version, Exception Error);
}
