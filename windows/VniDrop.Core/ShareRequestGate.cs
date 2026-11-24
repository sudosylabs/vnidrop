namespace VniDrop.Core;

public sealed class ShareRequestGate
{
    private int active;

    public bool IsActive => Volatile.Read(ref active) != 0;

    public Request Enter()
    {
        if (Interlocked.CompareExchange(ref active, 1, 0) != 0)
            throw new InvalidOperationException("windows_share_busy");
        return new Request(this);
    }

    public sealed class Request : IDisposable
    {
        private readonly TaskCompletionSource handoff = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private ShareRequestGate? current;

        internal Request(ShareRequestGate owner) => current = owner;

        public bool WasHandedOff => handoff.Task.IsCompletedSuccessfully;

        public Task WaitForHandoffAsync(TimeSpan timeout, CancellationToken cancellationToken) =>
            handoff.Task.WaitAsync(timeout, cancellationToken);

        public void HandedOff() => handoff.TrySetResult();

        public void Fail(Exception error) => handoff.TrySetException(error);

        public void Cancel() => handoff.TrySetCanceled();

        public void Dispose()
        {
            var released = Interlocked.Exchange(ref current, null);
            if (released is null) return;
            Cancel();
            Volatile.Write(ref released.active, 0);
        }
    }
}
