using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Windows.Storage.Pickers;
using VniDrop.Core;

namespace VniDrop.Platform;

public static class WindowsFiles
{
    public static async Task<DraftSource[]> PickAsync(bool folder)
    {
        if (folder)
        {
            var result = await new FolderPicker(App.Window.AppWindow.Id).PickSingleFolderAsync();
            return result is null ? [] : [Source(result.Path)];
        }
        var files = await new FileOpenPicker(App.Window.AppWindow.Id).PickMultipleFilesAsync();
        return files.Select(f => Source(f.Path)).ToArray();
    }
    public static DraftSource Source(string path) => Directory.Exists(path)
        ? new(path, new DirectoryInfo(path).Name, true, null)
        : new(path, Path.GetFileName(path), false, new FileInfo(path).Length);

    public static async Task<string?> OpenInvitationAsync()
    {
        var picker = new FileOpenPicker(App.Window.AppWindow.Id);
        picker.FileTypeFilter.Add(".vnd");
        var result = await picker.PickSingleFileAsync();
        return result is null ? null : await InvitationDocument.ReadAsync(result.Path);
    }
    public static async Task SaveInvitationAsync(string ticket, string name)
    {
        var picker = new FileSavePicker(App.Window.AppWindow.Id) { SuggestedFileName = InvitationDocument.FileName(name) };
        picker.FileTypeChoices.Add(Strings.Get("receive_method_file"), [".vnd"]);
        var result = await picker.PickSaveFileAsync();
        if (result is not null) await File.WriteAllTextAsync(result.Path, ticket);
    }
    public static void OpenFolder(string path) => Process.Start(new ProcessStartInfo { FileName = path, UseShellExecute = true });

    public static string DownloadsDirectory()
    {
        var id = new Guid("374DE290-123F-4565-9164-39C4925E467B");
        var result = SHGetKnownFolderPath(ref id, 0, IntPtr.Zero, out var pointer);
        if (result != 0) Marshal.ThrowExceptionForHR(result);
        try { return Marshal.PtrToStringUni(pointer)!; }
        finally { Marshal.FreeCoTaskMem(pointer); }
    }
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern int SHGetKnownFolderPath(ref Guid id, uint flags, IntPtr token, out IntPtr path);
}
