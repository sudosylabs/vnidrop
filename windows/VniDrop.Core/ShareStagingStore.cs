using System.Diagnostics;
using System.Globalization;

namespace VniDrop.Core;

public sealed class ShareStagingStore : IDisposable
{
    private const string OwnerFileName = ".owner";
    private readonly string rootDirectory;
    private readonly string ownerPath;
    private FileStream? ownerLease;
    private int disposed;

    public ShareStagingStore(string rootDirectory, int processId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(rootDirectory);
        if (processId <= 0) throw new ArgumentOutOfRangeException(nameof(processId));

        this.rootDirectory = Path.GetFullPath(rootDirectory);
        Directory.CreateDirectory(this.rootDirectory);
        EnsureDirectoryIsNotReparsePoint(this.rootDirectory);
        DirectoryPath = Path.Combine(this.rootDirectory, processId.ToString(CultureInfo.InvariantCulture));
        ownerPath = Path.Combine(DirectoryPath, OwnerFileName);

        using var cleanupLease = AcquireCleanupLease(this.rootDirectory);
        Directory.CreateDirectory(DirectoryPath);
        EnsureDirectoryIsNotReparsePoint(DirectoryPath);
        if (File.Exists(ownerPath) && File.GetAttributes(ownerPath).HasFlag(FileAttributes.ReparsePoint))
            throw new UnauthorizedAccessException("Share staging ownership cannot use a reparse point.");
        ownerLease = new FileStream(ownerPath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read);
        DeletePayloadDirectories(DirectoryPath);
        DeleteStaleProcessDirectories(this.rootDirectory, DirectoryPath);
    }

    public string DirectoryPath { get; }

    public string CreatePayloadPath(string fileName)
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref disposed) != 0, this);
        ArgumentException.ThrowIfNullOrWhiteSpace(fileName);
        if (!string.Equals(fileName, Path.GetFileName(fileName), StringComparison.Ordinal)
            || fileName.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
            throw new ArgumentException("The share payload name must be a file name.", nameof(fileName));

        var directory = Path.Combine(DirectoryPath, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        return Path.Combine(directory, fileName);
    }

    public void DeletePayload(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return;
        try
        {
            var fullPath = Path.GetFullPath(path);
            var payloadDirectory = Path.GetDirectoryName(fullPath);
            if (payloadDirectory is null
                || !Guid.TryParseExact(Path.GetFileName(payloadDirectory), "N", out _)
                || !string.Equals(Path.GetDirectoryName(payloadDirectory), DirectoryPath, StringComparison.OrdinalIgnoreCase))
                return;
            File.Delete(fullPath);
            Directory.Delete(payloadDirectory, false);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException)
        {
        }
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref disposed, 1) != 0) return;
        ownerLease?.Dispose();
        ownerLease = null;
        DeletePayloadDirectories(DirectoryPath);
        TryDeleteFile(ownerPath);
        TryDeleteDirectory(DirectoryPath);
    }

    private static FileStream AcquireCleanupLease(string rootDirectory)
    {
        var path = Path.Combine(rootDirectory, ".cleanup.lock");
        if (File.Exists(path) && File.GetAttributes(path).HasFlag(FileAttributes.ReparsePoint))
            throw new UnauthorizedAccessException("Share staging cleanup cannot use a reparse point.");
        var deadline = Stopwatch.StartNew();
        while (true)
        {
            try { return new FileStream(path, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None); }
            catch (IOException) when (deadline.Elapsed < TimeSpan.FromSeconds(5)) { Thread.Sleep(25); }
        }
    }

    private static void DeleteStaleProcessDirectories(string rootDirectory, string currentDirectory)
    {
        foreach (var directory in Directory.EnumerateDirectories(rootDirectory))
        {
            if (string.Equals(directory, currentDirectory, StringComparison.OrdinalIgnoreCase)) continue;
            try
            {
                EnsureDirectoryIsNotReparsePoint(directory);
                var ownerPath = Path.Combine(directory, OwnerFileName);
                if (File.Exists(ownerPath))
                {
                    if (File.GetAttributes(ownerPath).HasFlag(FileAttributes.ReparsePoint)) continue;
                    try
                    {
                        using var probe = new FileStream(ownerPath, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                    }
                    catch (Exception error) when (error is IOException or UnauthorizedAccessException)
                    {
                        continue;
                    }
                }
                else
                {
                    if (!int.TryParse(Path.GetFileName(directory), NumberStyles.None, CultureInfo.InvariantCulture, out var processId)
                        || IsProcessRunning(processId))
                        continue;
                }

                DeletePayloadDirectories(directory);
                TryDeleteFile(ownerPath);
                TryDeleteDirectory(directory);
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
            }
        }
    }

    private static bool IsProcessRunning(int processId)
    {
        try
        {
            using var process = Process.GetProcessById(processId);
            return !process.HasExited;
        }
        catch (ArgumentException) { return false; }
        catch (InvalidOperationException) { return false; }
        catch { return true; }
    }

    private static void DeletePayloadDirectories(string processDirectory)
    {
        if (!Directory.Exists(processDirectory)) return;
        string[] directories;
        try { directories = Directory.GetDirectories(processDirectory); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { return; }
        foreach (var directory in directories)
        {
            if (!Guid.TryParseExact(Path.GetFileName(directory), "N", out _)) continue;
            try
            {
                EnsureDirectoryIsNotReparsePoint(directory);
                foreach (var file in Directory.EnumerateFiles(directory, "*.vnd", SearchOption.TopDirectoryOnly))
                    File.Delete(file);
                Directory.Delete(directory, false);
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
            }
        }
    }

    private static void EnsureDirectoryIsNotReparsePoint(string path)
    {
        if (File.GetAttributes(path).HasFlag(FileAttributes.ReparsePoint))
            throw new UnauthorizedAccessException("Share staging cannot use a reparse point.");
    }

    private static void TryDeleteFile(string path)
    {
        try { File.Delete(path); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
    }

    private static void TryDeleteDirectory(string path)
    {
        try { Directory.Delete(path, false); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
    }
}
