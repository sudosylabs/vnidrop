using VniDrop.Native;

namespace VniDrop.Core;

public sealed record WindowsStorageUsage(ulong Received, ulong Transfer, ulong AppData, ulong Temporary)
{
    public ulong Total => Received + Transfer + AppData + Temporary;
}

public static class WindowsStorage
{
    public static WindowsStorageUsage Inspect(string profile, CoreStorageUsage core, IReadOnlyList<ReceivedArtifact> artifacts)
    {
        ulong received = 0;
        foreach (var artifact in artifacts)
            try { var file = new FileInfo(artifact.locator); if (file.Exists) received += (ulong)file.Length; } catch (Exception) { }
        return new(received, core.blobStoreBytes, core.databaseBytes + core.logsBytes + core.previewsBytes + core.otherCoreBytes, VnidropMethods.ReceiveStagingBytes(profile, false));
    }

    public static ulong ReclaimTemporary(string profile)
    {
        var reclaimed = VnidropMethods.ReceiveStagingBytes(profile, true);
        if (Directory.Exists(profile))
            foreach (var directory in Directory.EnumerateDirectories(profile, ".Trash", SearchOption.AllDirectories).ToArray())
                try { var size = Directory.EnumerateFiles(directory, "*", SearchOption.AllDirectories).Aggregate(0UL, (total, file) => total + (ulong)new FileInfo(file).Length); Directory.Delete(directory, true); reclaimed += size; } catch (IOException) { } catch (UnauthorizedAccessException) { }
        return reclaimed;
    }

}
