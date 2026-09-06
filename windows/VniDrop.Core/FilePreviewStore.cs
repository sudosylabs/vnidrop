using System.Globalization;

namespace VniDrop.Core;

public sealed class FilePreviewStore(string profileDirectory)
{
    public const int MaximumEntryBytes = 512 * 1024;
    private const long MaximumTotalBytes = 20L * 1024 * 1024;
    private readonly string directory = Path.Combine(profileDirectory, "ui", "previews");
    private readonly object gate = new();

    public byte[]? Read(ulong transferId)
    {
        lock (gate)
        {
            var path = PreviewPath(transferId);
            if (!File.Exists(path)) return null;
            using var stream = File.OpenRead(path);
            if (stream.Length is <= 0 or > MaximumEntryBytes) return null;
            var bytes = new byte[(int)stream.Length];
            stream.ReadExactly(bytes);
            return IsSupported(bytes) ? bytes : null;
        }
    }

    public void Save(ulong transferId, byte[] bytes)
    {
        if (bytes.Length is <= 0 or > MaximumEntryBytes || !IsSupported(bytes)) return;
        lock (gate)
        {
            Directory.CreateDirectory(directory);
            var path = PreviewPath(transferId);
            var temporary = path + ".tmp";
            try
            {
                File.WriteAllBytes(temporary, bytes);
                File.Move(temporary, path, true);
                Prune(null, transferId);
            }
            finally { if (File.Exists(temporary)) File.Delete(temporary); }
        }
    }

    public void Retain(IReadOnlySet<ulong> transferIds)
    {
        lock (gate) Prune(transferIds, null);
    }

    private void Prune(IReadOnlySet<ulong>? transferIds, ulong? protectedId)
    {
        if (!Directory.Exists(directory)) return;
        var files = new DirectoryInfo(directory).GetFiles("*.preview")
            .Where(file => ulong.TryParse(Path.GetFileNameWithoutExtension(file.Name), out _))
            .OrderBy(file => file.LastWriteTimeUtc).ToArray();
        long total = 0;
        var retained = new List<FileInfo>();
        foreach (var file in files)
        {
            var id = ulong.Parse(Path.GetFileNameWithoutExtension(file.Name), CultureInfo.InvariantCulture);
            if (file.Length is <= 0 or > MaximumEntryBytes || transferIds is not null && !transferIds.Contains(id))
                file.Delete();
            else { retained.Add(file); total += file.Length; }
        }
        foreach (var file in retained)
        {
            if (total <= MaximumTotalBytes) break;
            if (file.FullName == (protectedId is { } id ? Path.GetFullPath(PreviewPath(id)) : null)) continue;
            total -= file.Length;
            file.Delete();
        }
    }

    private string PreviewPath(ulong id) => Path.Combine(directory, id.ToString(CultureInfo.InvariantCulture) + ".preview");

    private static bool IsSupported(ReadOnlySpan<byte> bytes) =>
        bytes.StartsWith(new byte[] { 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a })
        || bytes.StartsWith(new byte[] { 0xff, 0xd8, 0xff })
        || bytes.Length >= 12 && bytes[..4].SequenceEqual("RIFF"u8) && bytes.Slice(8, 4).SequenceEqual("WEBP"u8);
}
