using VniDrop.Native;

namespace VniDrop.Core;

public sealed record WindowsStorageUsage(ulong Received, ulong Transfer, ulong AppData, ulong Temporary)
{
    public ulong Total => Received + Transfer + AppData + Temporary;
}

public static class WindowsStorage
{
    public static WindowsStorageUsage Inspect(string profile, string receiveDirectory, CoreStorageUsage core, IReadOnlyList<ReceivedArtifact> artifacts)
    {
        ulong received = 0;
        foreach (var artifact in artifacts)
            try { var file = new FileInfo(artifact.locator); if (file.Exists) received += (ulong)file.Length; } catch (Exception) { }
        return new(received, core.blobStoreBytes, core.databaseBytes + core.logsBytes + core.previewsBytes + core.otherCoreBytes, TemporaryBytes(receiveDirectory));
    }

    public static ulong ReclaimTemporary(string profile, string receiveDirectory)
    {
        ulong reclaimed = 0;
        if (Directory.Exists(receiveDirectory))
            foreach (var file in Directory.EnumerateFiles(receiveDirectory, ".*.part", SearchOption.AllDirectories).Where(IsReceivePart))
                try { var size = (ulong)new FileInfo(file).Length; File.Delete(file); reclaimed += size; } catch (IOException) { } catch (UnauthorizedAccessException) { }
        if (Directory.Exists(profile))
            foreach (var directory in Directory.EnumerateDirectories(profile, ".Trash", SearchOption.AllDirectories).ToArray())
                try { var size = Directory.EnumerateFiles(directory, "*", SearchOption.AllDirectories).Aggregate(0UL, (total, file) => total + (ulong)new FileInfo(file).Length); Directory.Delete(directory, true); reclaimed += size; } catch (IOException) { } catch (UnauthorizedAccessException) { }
        return reclaimed;
    }

    public static ulong TemporaryBytes(string receiveDirectory)
    {
        if (!Directory.Exists(receiveDirectory)) return 0;
        ulong total = 0;
        try { foreach (var file in Directory.EnumerateFiles(receiveDirectory, ".*.part", SearchOption.AllDirectories).Where(IsReceivePart)) total += (ulong)new FileInfo(file).Length; } catch (IOException) { } catch (UnauthorizedAccessException) { }
        return total;
    }

    public static bool IsReceivePart(string path)
    {
        var name = Path.GetFileName(path);
        return name.StartsWith('.') && name.Contains(".vnidrop-", StringComparison.Ordinal) && name.EndsWith(".part", StringComparison.Ordinal);
    }
}
