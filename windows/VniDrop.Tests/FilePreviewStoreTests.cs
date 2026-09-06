using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class FilePreviewStoreTests : IDisposable
{
    private readonly string profile = Path.Combine(Path.GetTempPath(), "vnidrop-preview-tests", Guid.NewGuid().ToString("N"));
    private static readonly byte[] png = Convert.FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/lX8AAAAASUVORK5CYII=");

    [Fact]
    public void PreviewSurvivesRestartAndReadsExistingKmpCache()
    {
        var store = new FilePreviewStore(profile);
        store.Save(7, png);
        Assert.Equal(png, new FilePreviewStore(profile).Read(7));
        File.WriteAllBytes(Path.Combine(profile, "ui", "previews", "8.preview"), png);
        Assert.Equal(png, store.Read(8));
        Assert.Null(store.Read(9));
    }

    [Fact]
    public void InvalidReplacementPreservesPreviewAndInvalidDiskEntriesFallBack()
    {
        var store = new FilePreviewStore(profile);
        store.Save(7, png);
        store.Save(7, "not a preview"u8.ToArray());
        store.Save(7, new byte[FilePreviewStore.MaximumEntryBytes + 1]);
        Assert.Equal(png, store.Read(7));
        File.WriteAllText(Path.Combine(profile, "ui", "previews", "8.preview"), "corrupt");
        Assert.Null(store.Read(8));
        File.WriteAllBytes(Path.Combine(profile, "ui", "previews", "9.preview"), new byte[FilePreviewStore.MaximumEntryBytes + 1]);
        Assert.Null(store.Read(9));
    }

    [Fact]
    public void RemovingTransfersPrunesTheirPreviewsAndPreservesUnrelatedFiles()
    {
        var store = new FilePreviewStore(profile);
        store.Save(7, png);
        store.Save(8, png);
        var unrelated = Path.Combine(profile, "ui", "previews", "notes.txt");
        File.WriteAllText(unrelated, "keep");
        store.Retain(new HashSet<ulong> { 8 });
        Assert.Null(store.Read(7));
        Assert.Equal(png, store.Read(8));
        Assert.Equal("keep", File.ReadAllText(unrelated));
    }

    [Fact]
    public void CacheQuotaEvictsOldestPreviewAndProtectsLatestSave()
    {
        var store = new FilePreviewStore(profile);
        var large = new byte[FilePreviewStore.MaximumEntryBytes];
        png.CopyTo(large, 0);
        for (ulong id = 1; id <= 40; id++) store.Save(id, large);
        File.SetLastWriteTimeUtc(Path.Combine(profile, "ui", "previews", "1.preview"), DateTime.UnixEpoch);
        store.Save(41, large);
        Assert.Null(store.Read(1));
        Assert.Equal(large, store.Read(41));
        Assert.Equal(40, Directory.GetFiles(Path.Combine(profile, "ui", "previews"), "*.preview").Length);
    }

    [Theory]
    [InlineData("IMG_0438.PNG", "holiday.jpeg")]
    [InlineData("video.MOV", "movie.mp4")]
    [InlineData("archive.ZIP", "files.7z")]
    public void FileArtworkRecognizesExtensionsIndependentlyOfCase(string name, string equivalentType)
    {
        Assert.Equal(FileArtworkPresentation.Glyph(equivalentType), FileArtworkPresentation.Glyph(name));
        Assert.NotEqual(FileArtworkPresentation.Glyph("unknown"), FileArtworkPresentation.Glyph(name));
    }

    [Fact]
    public void DirectoryAndCollectionArtworkDoNotMisrepresentNamesAsSingleImages()
    {
        Assert.Equal(FileArtworkPresentation.Glyph("folder", true), FileArtworkPresentation.Glyph("Photos.png", true));
        Assert.NotEqual(FileArtworkPresentation.Glyph("Photos.png"), FileArtworkPresentation.Glyph("Photos.png", true));
        Assert.Equal(FileArtworkPresentation.Glyph("selection", fileCount: 2), FileArtworkPresentation.Glyph("photo.jpg", fileCount: 2));
    }

    public void Dispose()
    {
        if (Directory.Exists(profile)) Directory.Delete(profile, true);
    }
}
