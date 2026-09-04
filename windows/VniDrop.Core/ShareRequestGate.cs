namespace VniDrop.Core;

public sealed class ShareRequestGate
{
    private int active;

    public bool IsActive => Volatile.Read(ref active) != 0;

    public IDisposable Enter()
    {
        if (Interlocked.CompareExchange(ref active, 1, 0) != 0)
            throw new InvalidOperationException("windows_share_busy");
        return new Lease(this);
    }

    private sealed class Lease(ShareRequestGate owner) : IDisposable
    {
        private ShareRequestGate? current = owner;

        public void Dispose()
        {
            var released = Interlocked.Exchange(ref current, null);
            if (released is not null) Volatile.Write(ref released.active, 0);
        }
    }
}
