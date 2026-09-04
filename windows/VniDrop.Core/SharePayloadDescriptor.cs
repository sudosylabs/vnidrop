namespace VniDrop.Core;

public enum SharePayloadContentKind
{
    Text,
    File,
}

public sealed record SharePayloadDescriptor
{
    private SharePayloadDescriptor(
        SharePayloadContentKind kind,
        string title,
        string description,
        string? text,
        string? filePath)
    {
        Kind = kind;
        Title = title;
        Description = description;
        Text = text;
        FilePath = filePath;
    }

    public SharePayloadContentKind Kind { get; }
    public string Title { get; }
    public string Description { get; }
    public string? Text { get; }
    public string? FilePath { get; }

    public static SharePayloadDescriptor ForText(string title, string description, string text)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(title);
        ArgumentException.ThrowIfNullOrWhiteSpace(text);
        return new(SharePayloadContentKind.Text, title, description, text, null);
    }

    public static SharePayloadDescriptor ForFile(string title, string description, string filePath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(title);
        ArgumentException.ThrowIfNullOrWhiteSpace(filePath);
        return new(SharePayloadContentKind.File, title, description, null, filePath);
    }
}
