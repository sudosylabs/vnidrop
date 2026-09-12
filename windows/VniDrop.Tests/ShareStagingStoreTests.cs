using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class ShareStagingStoreTests
{
    [Fact]
    public void ActiveProcessPayloadsSurviveAnotherStoreCleanup()
    {
        var root = TemporaryRoot();
        try
        {
            using var active = new ShareStagingStore(root, 100001);
            var activePayload = active.CreatePayloadPath("active.vnd");
            File.WriteAllText(activePayload, "capability");

            using var other = new ShareStagingStore(root, 100002);

            Assert.True(File.Exists(activePayload));
        }
        finally { DeleteTree(root); }
    }

    [Fact]
    public void UnlockedCrashedProcessPayloadsAreReclaimed()
    {
        var root = TemporaryRoot();
        var staleRoot = Path.Combine(root, "100003");
        var payloadRoot = Path.Combine(staleRoot, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(payloadRoot);
        File.WriteAllText(Path.Combine(staleRoot, ".owner"), "");
        var stalePayload = Path.Combine(payloadRoot, "stale.vnd");
        File.WriteAllText(stalePayload, "capability");
        File.SetLastWriteTimeUtc(stalePayload, DateTime.UtcNow.AddDays(-2));
        try
        {
            using var current = new ShareStagingStore(root, 100004);

            Assert.False(Directory.Exists(staleRoot));
        }
        finally { DeleteTree(root); }
    }

    [Fact]
    public void HandedOffInvitationSurvivesShutdownAndAnotherProcessUntilRetentionExpires()
    {
        var root = TemporaryRoot();
        try
        {
            string payload;
            var ticket = "vnidrop-test-invitation-é";
            using (var source = new ShareStagingStore(root, 100005))
            {
                payload = source.CreatePayloadPath(InvitationDocument.FileName("Photos été"));
                File.WriteAllText(payload, ticket);
            }
            using (var next = new ShareStagingStore(root, 100006))
            {
                Assert.Equal(".vnd", Path.GetExtension(payload));
                Assert.Equal(System.Text.Encoding.UTF8.GetBytes(ticket), File.ReadAllBytes(payload));
            }
            File.SetLastWriteTimeUtc(payload, DateTime.UtcNow.AddDays(-2));
            using var cleanup = new ShareStagingStore(root, 100007);
            Assert.False(File.Exists(payload));
        }
        finally { DeleteTree(root); }
    }

    [Fact]
    public void CancelledInvitationIsRemovedImmediately()
    {
        var root = TemporaryRoot();
        try
        {
            using var source = new ShareStagingStore(root, 100008);
            var payload = source.CreatePayloadPath("cancelled.vnd");
            File.WriteAllText(payload, "ticket");
            source.DeletePayload(payload);
            Assert.False(File.Exists(payload));
        }
        finally { DeleteTree(root); }
    }

    [Fact]
    public void ActiveLegacyProcessDirectoryIsNotReclaimed()
    {
        var root = TemporaryRoot();
        var legacyRoot = Path.Combine(root, Environment.ProcessId.ToString());
        var payloadRoot = Path.Combine(legacyRoot, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(payloadRoot);
        var payload = Path.Combine(payloadRoot, "active.vnd");
        File.WriteAllText(payload, "capability");
        try
        {
            using var current = new ShareStagingStore(root, int.MaxValue);

            Assert.True(File.Exists(payload));
        }
        finally { DeleteTree(root); }
    }

    private static string TemporaryRoot() => Path.Combine(
        Path.GetTempPath(),
        "vnidrop-share-staging-tests",
        Guid.NewGuid().ToString("N"));

    private static void DeleteTree(string path)
    {
        try { Directory.Delete(path, true); }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }
}
