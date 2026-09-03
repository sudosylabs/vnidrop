using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop.ViewModels;

public sealed record DraftSourceItem(DraftSource Source)
{
    public string Name => Source.Name;
    public string Glyph => Source.IsDirectory ? "\uE8B7" : "\uE8A5";
    public string Detail => Source.IsDirectory ? Strings.Get("send_folder_label") : Source.Size is { } size ? Strings.Size((ulong)size) : Strings.Get("send_file_size_unknown");
}
