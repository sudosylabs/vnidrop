namespace VniDrop.Core;

public static class FileArtworkPresentation
{
    public static string Glyph(string? name, bool isDirectory = false, ulong fileCount = 1) => isDirectory ? "\uE8B7"
        : fileCount > 1 ? "\uE8FD"
        : Path.GetExtension(name ?? "").ToLowerInvariant() switch
        {
            ".png" or ".jpg" or ".jpeg" or ".gif" or ".webp" or ".bmp" or ".heic" or ".heif" or ".tif" or ".tiff" or ".svg" => "\uEB9F",
            ".mp4" or ".mov" or ".mkv" or ".avi" or ".webm" => "\uE714",
            ".mp3" or ".wav" or ".flac" or ".m4a" or ".ogg" or ".aac" => "\uE8D6",
            ".zip" or ".7z" or ".rar" or ".tar" or ".gz" => "\uF012",
            ".pdf" => "\uEA90",
            ".txt" or ".md" or ".doc" or ".docx" or ".rtf" => "\uE8A5",
            ".xls" or ".xlsx" or ".csv" => "\uE9F9",
            ".ppt" or ".pptx" => "\uE8F9",
            _ => "\uE7C3",
        };
}
