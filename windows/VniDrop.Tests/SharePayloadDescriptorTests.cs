using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class SharePayloadDescriptorTests
{
    [Fact]
    public void ReportPayloadContainsTextOnly()
    {
        var payload = SharePayloadDescriptor.ForText("Bug report", "Choose where to share", "Report body");

        Assert.Equal(SharePayloadContentKind.Text, payload.Kind);
        Assert.Equal("Report body", payload.Text);
        Assert.Null(payload.FilePath);
    }

    [Fact]
    public void InvitationPayloadContainsFileOnly()
    {
        var payload = SharePayloadDescriptor.ForFile("Transfer", "Share invitation", @"C:\Temp\transfer.vnd");

        Assert.Equal(SharePayloadContentKind.File, payload.Kind);
        Assert.Null(payload.Text);
        Assert.Equal(@"C:\Temp\transfer.vnd", payload.FilePath);
    }

    [Fact]
    public void OnlyOneNativeShareCanOwnThePayloadSlot()
    {
        var gate = new ShareRequestGate();
        var first = gate.Enter();

        var error = Assert.Throws<InvalidOperationException>(() => gate.Enter());
        Assert.Equal("windows_share_busy", error.Message);
        Assert.True(gate.IsActive);

        first.Dispose();
        first.Dispose();
        Assert.False(gate.IsActive);
        using var next = gate.Enter();
        Assert.True(gate.IsActive);
    }
}
