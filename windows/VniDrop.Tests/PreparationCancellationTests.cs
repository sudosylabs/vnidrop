using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class PreparationCancellationTests
{
    [Fact]
    public async Task CancellationWaitsForNativeWorkBeforeReleasingItsOwner()
    {
        using var cancellation = new CancellationTokenSource();
        var work = new TaskCompletionSource<int>(TaskCreationOptions.RunContinuationsAsynchronously);
        var stopped = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var completion = PreparationCancellation.CompleteAsync(work.Task, () =>
        {
            stopped.SetResult();
            return Task.CompletedTask;
        }, cancellation.Token);
        cancellation.Cancel();
        await stopped.Task.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.False(completion.IsCompleted);
        work.SetException(new IOException("Native import stopped"));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => completion.WaitAsync(TimeSpan.FromSeconds(5)));
    }

    [Fact]
    public async Task CancellationStillStopsAResultThatWonTheCompletionRace()
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var stops = 0;
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => PreparationCancellation.CompleteAsync(
            Task.FromResult("registered transfer"), () => { stops++; return Task.CompletedTask; }, cancellation.Token));
        Assert.Equal(1, stops);
    }

    [Fact]
    public async Task StopFailureIsReportedInsteadOfPretendingTheShareWasCancelled()
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var error = new IOException("Could not stop the registered share");
        Assert.Same(error, await Assert.ThrowsAsync<IOException>(() => PreparationCancellation.CompleteAsync(
            Task.FromResult(1), () => Task.FromException(error), cancellation.Token)));
    }

    [Fact]
    public async Task NormalCompletionAndFailureDoNotRequestCancellation()
    {
        Task Stop() => throw new InvalidOperationException("Unexpected cancellation");
        Assert.Equal(7, await PreparationCancellation.CompleteAsync(Task.FromResult(7), Stop, CancellationToken.None));
        var error = new IOException("Unreadable source");
        Assert.Same(error, await Assert.ThrowsAsync<IOException>(() => PreparationCancellation.CompleteAsync(
            Task.FromException<int>(error), Stop, CancellationToken.None)));
    }
}
