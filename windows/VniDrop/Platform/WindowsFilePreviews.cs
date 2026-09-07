using VniDrop.Core;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.FileProperties;
using Windows.Storage.Streams;

namespace VniDrop.Platform;

public static class WindowsFilePreviews
{
    private static readonly SemaphoreSlim readers = new(4);

    public static async Task<byte[]?> ReadAsync(string path)
    {
        await readers.WaitAsync();
        try
        {
            using var thumbnail = Directory.Exists(path)
                ? await (await StorageFolder.GetFolderFromPathAsync(path)).GetThumbnailAsync(ThumbnailMode.SingleItem, 128)
                : await (await StorageFile.GetFileFromPathAsync(path)).GetThumbnailAsync(ThumbnailMode.SingleItem, 128);
            if (thumbnail is null || thumbnail.Size == 0) return null;
            var decoder = await BitmapDecoder.CreateAsync(thumbnail);
            using var bitmap = await decoder.GetSoftwareBitmapAsync();
            using var output = new InMemoryRandomAccessStream();
            var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, output);
            encoder.SetSoftwareBitmap(bitmap);
            await encoder.FlushAsync();
            if (output.Size is 0 or > FilePreviewStore.MaximumEntryBytes) return null;
            output.Seek(0);
            using var reader = new DataReader(output.GetInputStreamAt(0));
            await reader.LoadAsync((uint)output.Size);
            var bytes = new byte[(int)output.Size];
            reader.ReadBytes(bytes);
            return bytes;
        }
        catch (Exception) { return null; }
        finally { readers.Release(); }
    }
}
