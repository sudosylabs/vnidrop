using VniDrop.Core;
using VniDrop.Native;
using Xunit;

namespace VniDrop.Tests;

public sealed class TransferEventHistoryTests
{
    [Fact]
    public void BusyTransferCannotEvictAnotherTransferBeforeRefresh()
    {
        var session = new CoreSession(Path.GetTempPath());
        var connecting = Event(1, 7, "network", "connecting");
        var progress = Event(2, 7, "download", "progress", """{"downloaded":40,"total_size":100}""");
        session.OnEvent(connecting);
        session.OnEvent(progress);
        for (ulong revision = 3; revision <= 610; revision++)
            session.OnEvent(Event(revision, 8, "download", "progress"));

        var retained = session.DrainEvents().Where(entry => entry.transferId == 7).ToArray();
        Assert.Equal([connecting, progress], retained);
        Assert.Equal(.4, TransferPresentation.Progress(retained, 7, "receive", 100)?.Fraction);
    }

    [Fact]
    public void RecentProgressCannotEvictARecordedActivityEntry()
    {
        var history = new TransferEventHistory();
        var connecting = Event(1, 7, "network", "connecting");
        history.Add(connecting);
        history.Add(connecting);
        for (ulong revision = 2; revision <= 1200; revision++)
            history.Add(Event(revision, 7, "download", "progress", """{"downloaded":40,"total_size":100}"""));

        var retained = history.ForTransfer(7);
        Assert.Single(retained, entry => entry.id == connecting.id);
        Assert.Equal((ulong)1200, retained[^1].revision);
        Assert.Equal(.4, TransferPresentation.Progress(retained, 7, "receive", 100)?.Fraction);
    }

    private static CoreEvent Event(ulong revision, ulong transferId, string phase, string kind, string data = "{}") =>
        new($"event-{revision}", revision, (long)revision, "transfer", transferId, "receive", phase, kind, data);
}
