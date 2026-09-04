using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class DialogPriorityPolicyTests
{
    [Fact]
    public void AttentionRunsBeforeAnEarlierStandardRequest()
    {
        Assert.True(DialogPriorityPolicy.ShouldRunBefore(
            DialogPriority.Attention,
            candidateSequence: 2,
            DialogPriority.Standard,
            currentSequence: 1));
    }

    [Fact]
    public void EqualPriorityRequestsRemainFirstInFirstOut()
    {
        Assert.True(DialogPriorityPolicy.ShouldRunBefore(
            DialogPriority.Standard,
            candidateSequence: 1,
            DialogPriority.Standard,
            currentSequence: 2));
        Assert.False(DialogPriorityPolicy.ShouldRunBefore(
            DialogPriority.Standard,
            candidateSequence: 2,
            DialogPriority.Standard,
            currentSequence: 1));
    }

    [Fact]
    public void AttentionRequestsYieldFromYieldableStandardDialog()
    {
        Assert.True(DialogPriorityPolicy.ShouldRequestYield(
            DialogPriority.Standard,
            DialogPriority.Attention,
            activeCanYield: true));
    }

    [Theory]
    [InlineData(DialogPriority.Standard, DialogPriority.Standard, true)]
    [InlineData(DialogPriority.Attention, DialogPriority.Standard, true)]
    [InlineData(DialogPriority.Attention, DialogPriority.Attention, true)]
    [InlineData(DialogPriority.Standard, DialogPriority.Attention, false)]
    public void LowerEqualOrNonYieldableRequestsDoNotYield(
        DialogPriority active,
        DialogPriority incoming,
        bool activeCanYield)
    {
        Assert.False(DialogPriorityPolicy.ShouldRequestYield(active, incoming, activeCanYield));
    }
}
