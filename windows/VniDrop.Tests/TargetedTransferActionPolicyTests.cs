using VniDrop.Core;
using VniDrop.Native;
using Xunit;

namespace VniDrop.Tests;

public class TargetedTransferActionPolicyTests
{
    [Theory]
    [InlineData(TargetedTransferState.Approved)]
    [InlineData(TargetedTransferState.Interrupted)]
    public void RunningReceiveDisablesDuplicateStartButKeepsCancelReachable(TargetedTransferState state)
    {
        var available = TargetedTransferActionPolicy.Evaluate(
            TargetedTransferRole.Receiver,
            state,
            receiveRunning: true,
            mutationBusy: false);

        Assert.False(available.CanStart);
        Assert.True(available.CanCancel);
        Assert.True(available.CanOpenActions);
        Assert.False(available.CanDelete);
        Assert.True(available.ShowBusy);
    }

    [Fact]
    public void ShortMutationBlocksEveryActionUntilItFinishes()
    {
        var available = TargetedTransferActionPolicy.Evaluate(
            TargetedTransferRole.Receiver,
            TargetedTransferState.Approved,
            receiveRunning: true,
            mutationBusy: true);

        Assert.False(available.CanStart);
        Assert.False(available.CanCancel);
        Assert.False(available.CanDelete);
        Assert.False(available.CanOpenActions);
        Assert.True(available.ShowBusy);
    }

    [Fact]
    public void TerminalTransferWaitsForReceiveToUnwindBeforeDelete()
    {
        var unwinding = TargetedTransferActionPolicy.Evaluate(
            TargetedTransferRole.Receiver,
            TargetedTransferState.Completed,
            receiveRunning: true,
            mutationBusy: false);
        var finished = TargetedTransferActionPolicy.Evaluate(
            TargetedTransferRole.Receiver,
            TargetedTransferState.Completed,
            receiveRunning: false,
            mutationBusy: false);

        Assert.False(unwinding.CanDelete);
        Assert.False(unwinding.CanOpenActions);
        Assert.True(unwinding.ShowBusy);
        Assert.True(finished.CanDelete);
        Assert.True(finished.CanOpenActions);
        Assert.False(finished.ShowBusy);
    }
}
