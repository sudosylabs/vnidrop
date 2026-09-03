using System.Globalization;
using Microsoft.Windows.ApplicationModel.Resources;
using VniDrop.Native;

namespace VniDrop.Platform;

public static class Strings
{
    private static readonly ResourceLoader Resources = new(Path.Combine(AppContext.BaseDirectory, "VniDrop.pri"), "Resources");
    public static string Get(string key)
    {
        try { var value = Resources.GetString(key); return string.IsNullOrEmpty(value) ? key : value; }
        catch (ArgumentException) { return key; }
    }
    public static string Format(string key, params (string Name, object? Value)[] arguments)
    {
        var text = Get(key);
        foreach (var (name, value) in arguments) text = text.Replace("{" + name + "}", Convert.ToString(value, CultureInfo.CurrentCulture));
        return text;
    }
    public static string Size(ulong bytes)
    {
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        double size = bytes; var unit = 0;
        while (size >= 1024 && unit < units.Length - 1) { size /= 1024; unit++; }
        return $"{size.ToString(unit == 0 ? "N0" : "N1", CultureInfo.CurrentCulture)} {units[unit]}";
    }
    public static string FileSummary(ulong count, ulong bytes)
    {
        var category = Core.FileCountPlural.Category(count, CultureInfo.CurrentUICulture.Name);
        return Format("transfer_file_count_" + category, ("count", count)) + " · " + Size(bytes);
    }
    public static string Error(Exception error) => Get(error switch
    {
        VnidropException.DestinationExists => "error_destination_exists",
        VnidropException.StorageFull => "error_storage_full",
        VnidropException.FilesystemPermission or UnauthorizedAccessException => "error_filesystem",
        VnidropException.Filesystem or IOException => "error_selection_failed",
        VnidropException.Ticket or System.Text.DecoderFallbackException => "error_invalid_ticket",
        VnidropException.Network or VnidropException.DeviceUnavailable or VnidropException.OfferTimeout => "error_network",
        VnidropException.Permission => "error_permission",
        VnidropException.Repository => "error_repository",
        VnidropException.SecureStorageMissing or VnidropException.SecureStorageCorrupted => "app_identity_reset_message",
        VnidropException.SecureStorageLocked or VnidropException.SecureStorageUnavailable => "app_secure_storage_unavailable_message",
        DllNotFoundException or BadImageFormatException => "error_missing_native_library",
        InvalidDataException or InvalidOperationException when error.Message.StartsWith("windows_") || error.Message == "error_invalid_ticket" => error.Message,
        _ => "windows_operation_failed",
    });
}
