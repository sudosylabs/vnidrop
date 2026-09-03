using System.Text;
using VniDrop.Core;
using VniDrop.Native;
using Xunit;

namespace VniDrop.Tests;

public class PresentationTests
{
    [Fact]
    public void InvitationRejectsMalformedUtf8AndOversizedFiles()
    {
        Assert.Throws<DecoderFallbackException>(() => InvitationDocument.Decode([0xff, 0xfe]));
        Assert.Throws<InvalidDataException>(() => InvitationDocument.Decode(new byte[InvitationDocument.MaximumBytes + 1]));
        Assert.Throws<InvalidDataException>(() => InvitationDocument.Decode(" \n"u8));
        Assert.Equal("invitation-é", InvitationDocument.Decode(Encoding.UTF8.GetBytes("invitation-é")));
    }

    [Fact]
    public void PickerCancellationAndInvalidSelectionsPreserveDraft()
    {
        var draft = new TransferDraft();
        var files = new[] { new DraftSource("a", "a", false, 1), new DraftSource("b", "b", false, 2) };
        draft.Select(files, count => $"{count} files");
        draft.Rename("My selection");
        draft.Select([], _ => "unused");
        Assert.Throws<InvalidDataException>(() => draft.Select([files[0], new("folder", "folder", true, null)], _ => "unused"));
        Assert.Equal(files, draft.Sources);
        Assert.Equal("My selection", draft.Name);
        draft.Remove(files[0], count => $"{count} files");
        Assert.Equal("My selection", draft.Name);
    }

    [Fact]
    public void AutomaticNamesFollowSelectionUntilEdited()
    {
        var draft = new TransferDraft();
        var files = new[] { new DraftSource("a", "a", false, 1), new DraftSource("b", "b", false, 2) };
        draft.Select(files, count => $"{count} files");
        Assert.Equal("2 files", draft.Name);
        draft.Remove(files[0], _ => "unused");
        Assert.Equal("b", draft.Name);
        draft.Remove(files[1], _ => "unused");
        Assert.Empty(draft.Name);
    }

    [Fact]
    public void ClearingDraftRestoresAutomaticNaming()
    {
        var draft = new TransferDraft();
        draft.Select([new("a", "a", false, 1)], _ => "unused");
        draft.Rename("Custom"); draft.Clear();
        draft.Select([new("b", "b", false, 1)], _ => "unused");
        Assert.Equal("b", draft.Name);
    }

    [Fact]
    public void ProgressAcceptsRustByteAliasesAndRejectsInvalidData()
    {
        var valid = Event("download", "progress", "{\"downloaded\":25,\"total_size\":100,\"file_name\":\"résumé.txt\"}");
        var result = TransferPresentation.Progress([valid], 7, "receive", 100);
        Assert.NotNull(result); Assert.Equal(.25, result.Fraction); Assert.Equal("progress_downloading", result.LabelKey); Assert.Equal("résumé.txt", result.FileName);
        var malformed = Event("download", "progress", "not json");
        Assert.Null(TransferPresentation.Progress([malformed, Event("lifecycle", "done", "{}")], 7, "receive", 100));
    }

    [Fact]
    public void TemporaryCleanupOnlyRecognizesVniDropPartFiles()
    {
        Assert.True(WindowsStorage.IsReceivePart(Path.Combine("x", ".photo.vnidrop-123.part")));
        Assert.False(WindowsStorage.IsReceivePart(Path.Combine("x", "photo.vnidrop-123.part")));
        Assert.False(WindowsStorage.IsReceivePart(Path.Combine("x", ".photo.vnidrop-123.txt")));
    }

    [Fact]
    public void ImportsKotlinPreferencesIncludingStrictRelayPolicy()
    {
        var bytes = Preference("username", "Élodie").Concat(Preference("receive_folder_value", "D:\\Reçus"))
            .Concat(Preference("relay_mode", "StrictCustom")).Concat(Preference("relay_urls", "https://relay.example.com"))
            .Concat(Preference("theme_mode", "Dark")).ToArray();
        var result = AppPreferences.ImportLegacy(bytes);
        Assert.Equal("Élodie", result.Username);
        Assert.Equal("D:\\Reçus", result.ReceiveDirectory);
        Assert.Equal("Dark", result.Theme);
        Assert.Equal(CoreRelayMode.StrictCustom, result.NetworkConfiguration.mode);
        Assert.Equal(["https://relay.example.com"], result.NetworkConfiguration.relayUrls);
    }

    [Fact]
    public void DamagedPreferencesNeverFallBackToPublicRelays()
    {
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy([10, 127, 8]));
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy(Preference("relay_mode", "UnknownMode")));
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy(Preference("relay_mode", "99")));
        var name = Encoding.UTF8.GetBytes("relay_mode");
        byte[] wrongType = [10, (byte)(name.Length + 6), 10, (byte)name.Length, .. name, 18, 2, 8, 1];
        Assert.Throws<InvalidDataException>(() => AppPreferences.ImportLegacy(wrongType));
    }

    [Fact]
    public void InvalidNativePreferencesRemainUntouched()
    {
        var directory = Path.Combine(Path.GetTempPath(), "vnidrop-preferences-test", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "windows-preferences.json");
        try
        {
            const string contents = "{\"RelayMode\":1,\"RelayUrls\":null}";
            File.WriteAllText(path, contents);
            Assert.Throws<InvalidDataException>(() => AppPreferences.Load(directory));
            Assert.Equal(contents, File.ReadAllText(path));
        }
        finally { File.Delete(path); Directory.Delete(directory); }
    }

    [Fact]
    public void ProfileArgumentsCannotBecomeInvitations()
    {
        var result = LaunchOptions.Parse(["--profile", "profile.vnd", "invitation.vnd", "invitation.vnd"]);
        Assert.Equal(Path.GetFullPath("profile.vnd"), result.Profile);
        Assert.Equal([Path.GetFullPath("invitation.vnd")], result.Invitations);
        Assert.Throws<ArgumentException>(() => LaunchOptions.Parse(["--profile"]));
    }

    [Theory]
    [InlineData(0, "fr-FR", "one")]
    [InlineData(1, "en-US", "one")]
    [InlineData(2, "en-US", "other")]
    [InlineData(1, "pl-PL", "one")]
    [InlineData(12, "pl-PL", "many")]
    [InlineData(22, "pl-PL", "few")]
    [InlineData(21, "ru-RU", "one")]
    [InlineData(11, "ru-RU", "many")]
    public void FileCountsUseLanguagePluralRules(int count, string language, string category) => Assert.Equal(category, FileCountPlural.Category((ulong)count, language));

    [Fact]
    public async Task FailedSubmissionRetainsEditableSources()
    {
        await using var session = new CoreSession(Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N")));
        var draft = new TransferDraft();
        draft.Select([new("missing", "missing", false, 1)], _ => "unused");
        draft.Rename("Keep me");
        await Assert.ThrowsAsync<InvalidOperationException>(() => draft.SubmitAsync(session, "Sender", true));
        Assert.False(draft.IsSubmitting);
        Assert.Equal("Keep me", draft.Name);
        Assert.Single(draft.Sources);
        draft.Rename("Retry");
        Assert.Equal("Retry", draft.Name);
    }

    private static byte[] Preference(string key, string value)
    {
        var name = Encoding.UTF8.GetBytes(key); var text = Encoding.UTF8.GetBytes(value);
        byte[] entry = [10, (byte)name.Length, .. name, 18, (byte)(text.Length + 2), 42, (byte)text.Length, .. text];
        return [10, (byte)entry.Length, .. entry];
    }

    private static CoreEvent Event(string phase, string kind, string data) => new("id-" + kind, 1, 1, "transfer", 7, "receive", phase, kind, data);
}
