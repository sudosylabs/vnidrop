using System.Runtime.InteropServices;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;

namespace VniDrop.Platform;

[ComImport]
[Guid("3A3DCD6C-3EAB-43DC-BCDE-45671CE800C8")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IDataTransferManagerInterop
{
    IntPtr GetForWindow([In] IntPtr appWindow, [In] ref Guid riid);
    void ShowShareUIForWindow(IntPtr appWindow);
}

public sealed class NativeShare : IDisposable
{
    private static readonly Guid DataTransferManagerId = new(0xa5caee9b, 0x8708, 0x49d1, 0x8d, 0x36, 0x67, 0xd2, 0x5a, 0x8d, 0xa0, 0x0c);
    private static readonly string ShareDirectory = Path.Combine(Path.GetTempPath(), "VniDrop", "Share");
    private readonly IntPtr windowHandle;
    private readonly IDataTransferManagerInterop interop;
    private readonly DataTransferManager manager;
    private StorageFile? invitationFile;
    private string? invitationTitle;
    private string? invitationPath;

    public NativeShare(Microsoft.UI.Xaml.Window window)
    {
        DeleteStaleFiles();
        windowHandle = WinRT.Interop.WindowNative.GetWindowHandle(window);
        interop = DataTransferManager.As<IDataTransferManagerInterop>();
        var dataTransferManagerId = DataTransferManagerId;
        manager = WinRT.MarshalInterface<DataTransferManager>.FromAbi(interop.GetForWindow(windowHandle, ref dataTransferManagerId));
        manager.DataRequested += OnDataRequested;
    }

    public async Task ShowInvitationAsync(string ticket, string transferName)
    {
        DeletePendingFile();
        var directory = Path.Combine(ShareDirectory, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        invitationPath = Path.Combine(directory, Core.InvitationDocument.FileName(transferName));
        await File.WriteAllTextAsync(invitationPath, ticket);
        invitationFile = await StorageFile.GetFileFromPathAsync(invitationPath);
        invitationTitle = transferName;
        interop.ShowShareUIForWindow(windowHandle);
    }

    private void OnDataRequested(DataTransferManager sender, DataRequestedEventArgs args)
    {
        if (invitationFile is null || invitationPath is null || invitationTitle is null) return;
        var path = invitationPath;
        var package = args.Request.Data;
        package.Properties.Title = invitationTitle;
        package.Properties.Description = Strings.Get("transfer_share_title");
        package.SetStorageItems([invitationFile]);
        package.ShareCompleted += (_, _) => DeleteFile(path);
        package.ShareCanceled += (_, _) => DeleteFile(path);
    }

    public void Dispose()
    {
        manager.DataRequested -= OnDataRequested;
        DeletePendingFile();
    }

    private void DeletePendingFile()
    {
        if (invitationPath is { } path) DeleteFile(path);
        invitationFile = null;
        invitationTitle = null;
        invitationPath = null;
    }

    private static void DeleteFile(string path)
    {
        try
        {
            File.Delete(path);
            var directory = Path.GetDirectoryName(path);
            if (directory is not null) Directory.Delete(directory, false);
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static void DeleteStaleFiles()
    {
        if (!Directory.Exists(ShareDirectory)) return;
        foreach (var directory in Directory.EnumerateDirectories(ShareDirectory))
        {
            if (!Guid.TryParseExact(Path.GetFileName(directory), "N", out _)) continue;
            try
            {
                foreach (var file in Directory.EnumerateFiles(directory, "*.vnd", SearchOption.TopDirectoryOnly)) File.Delete(file);
                Directory.Delete(directory, false);
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }
}
