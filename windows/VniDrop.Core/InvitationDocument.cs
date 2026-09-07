using System.Text;

namespace VniDrop.Core;

public static class InvitationDocument
{
    public const int MaximumBytes = 64 * 1024;
    public static string Decode(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length is 0 or > MaximumBytes) throw new InvalidDataException("error_invalid_ticket");
        var text = new UTF8Encoding(false, true).GetString(bytes);
        if (string.IsNullOrWhiteSpace(text)) throw new InvalidDataException("error_invalid_ticket");
        return text;
    }

    public static async Task<string> ReadAsync(string path)
    {
        if (!string.Equals(Path.GetExtension(path), ".vnd", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("error_invalid_ticket");
        await using var stream = File.OpenRead(path);
        var buffer = new byte[MaximumBytes + 1];
        var length = 0;
        while (length < buffer.Length)
        {
            var read = await stream.ReadAsync(buffer.AsMemory(length));
            if (read == 0) break;
            length += read;
        }
        return Decode(buffer.AsSpan(0, length));
    }

    public static string FileName(string name)
    {
        var safe = string.Concat(name.Select(c => Path.GetInvalidFileNameChars().Contains(c) ? '_' : c)).Trim().TrimEnd('.');
        if (safe.Length > 100) safe = safe[..100];
        return (string.IsNullOrWhiteSpace(safe) ? "VniDrop" : safe) + ".vnd";
    }
}
