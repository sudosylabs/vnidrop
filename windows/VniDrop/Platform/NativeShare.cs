using System.Runtime.InteropServices;
using VniDrop.Core;
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
    private static readonly TimeSpan HandoffTimeout = TimeSpan.FromSeconds(5);
    private static readonly Guid DataTransferManagerId = new(0xa5caee9b, 0x8708, 0x49d1, 0x8d, 0x36, 0x67, 0xd2, 0x5a, 0x8d, 0xa0, 0x0c);
    private readonly IntPtr windowHandle;
    private readonly IDataTransferManagerInterop interop;
    private readonly DataTransferManager manager;
    private readonly ShareRequestGate requestGate = new();
    private readonly ShareStagingStore staging;
    private SharePayload? pending;
    private bool disposed;

    public NativeShare(Microsoft.UI.Xaml.Window window)
    {
        staging = new(Path.Combine(Path.GetTempPath(), "VniDrop", "Share"), Environment.ProcessId);
        try
        {
            windowHandle = WinRT.Interop.WindowNative.GetWindowHandle(window);
            interop = DataTransferManager.As<IDataTransferManagerInterop>();
            var dataTransferManagerId = DataTransferManagerId;
            manager = WinRT.MarshalInterface<DataTransferManager>.FromAbi(interop.GetForWindow(windowHandle, ref dataTransferManagerId));
            manager.DataRequested += OnDataRequested;
        }
        catch
        {
            staging.Dispose();
            throw;
        }
    }

    public async Task ShowInvitationAsync(string ticket, string transferName, CancellationToken cancellationToken = default)
    {
        using var request = requestGate.Enter();
        var path = "";
        try
        {
            path = staging.CreatePayloadPath(Core.InvitationDocument.FileName(transferName));
            await File.WriteAllTextAsync(path, ticket, cancellationToken);
            var file = await StorageFile.GetFileFromPathAsync(path);
            var descriptor = SharePayloadDescriptor.ForFile(transferName, Strings.Get("transfer_share_title"), path);
            await ShowPayloadAsync(new(descriptor, file, request), cancellationToken);
        }
        finally
        {
            // A target can still be opening the attachment after Windows requests it.
            if (!request.WasHandedOff) staging.DeletePayload(path);
        }
    }

    public async Task ShowTextAsync(string title, string description, string text, CancellationToken cancellationToken = default)
    {
        using var request = requestGate.Enter();
        await ShowPayloadAsync(new(SharePayloadDescriptor.ForText(title, description, text), null, request), cancellationToken);
    }

    private async Task ShowPayloadAsync(SharePayload payload, CancellationToken cancellationToken)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        cancellationToken.ThrowIfCancellationRequested();
        pending = payload;
        try
        {
            interop.ShowShareUIForWindow(windowHandle);
            // Some share targets never report a completion or cancellation event.
            await payload.Request.WaitForHandoffAsync(HandoffTimeout, cancellationToken);
        }
        finally
        {
            pending = null;
        }
    }

    private void OnDataRequested(DataTransferManager sender, DataRequestedEventArgs args)
    {
        if (pending is not { } payload || payload.Request.WasHandedOff) return;
        try
        {
            var package = args.Request.Data;
            package.Properties.Title = payload.Descriptor.Title;
            package.Properties.Description = payload.Descriptor.Description;
            switch (payload.Descriptor.Kind)
            {
                case SharePayloadContentKind.Text:
                    package.SetText(payload.Descriptor.Text!);
                    break;
                case SharePayloadContentKind.File:
                    package.SetStorageItems([payload.File!]);
                    break;
            }
            payload.Request.HandedOff();
        }
        catch (Exception error)
        {
            payload.Request.Fail(error);
        }
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        manager.DataRequested -= OnDataRequested;
        pending?.Request.Cancel();
        staging.Dispose();
    }

    private sealed record SharePayload(
        SharePayloadDescriptor Descriptor,
        StorageFile? File,
        ShareRequestGate.Request Request);
}
