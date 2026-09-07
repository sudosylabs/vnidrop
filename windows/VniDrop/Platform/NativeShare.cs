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
    private static readonly Guid DataTransferManagerId = new(0xa5caee9b, 0x8708, 0x49d1, 0x8d, 0x36, 0x67, 0xd2, 0x5a, 0x8d, 0xa0, 0x0c);
    private readonly IntPtr windowHandle;
    private readonly IDataTransferManagerInterop interop;
    private readonly DataTransferManager manager;
    private readonly ShareRequestGate requestGate = new();
    private readonly ShareStagingStore staging;
    private SharePayload? pending;

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

    public async Task ShowInvitationAsync(string ticket, string transferName)
    {
        var lease = requestGate.Enter();
        var path = "";
        SharePayload? payload = null;
        try
        {
            path = staging.CreatePayloadPath(Core.InvitationDocument.FileName(transferName));
            await File.WriteAllTextAsync(path, ticket);
            var file = await StorageFile.GetFileFromPathAsync(path);
            var descriptor = SharePayloadDescriptor.ForFile(transferName, Strings.Get("transfer_share_title"), path);
            payload = new(Guid.NewGuid(), descriptor, file, lease, new(TaskCreationOptions.RunContinuationsAsynchronously));
            pending = payload;
            interop.ShowShareUIForWindow(windowHandle);
            await payload.Completion.Task;
        }
        catch
        {
            if (payload is null)
            {
                staging.DeletePayload(path);
                lease.Dispose();
            }
            else Abort(payload);
            throw;
        }
    }

    public async Task ShowTextAsync(string title, string description, string text)
    {
        var lease = requestGate.Enter();
        SharePayload? payload = null;
        try
        {
            payload = new(
                Guid.NewGuid(),
                SharePayloadDescriptor.ForText(title, description, text),
                null,
                lease,
                new(TaskCreationOptions.RunContinuationsAsynchronously));
            pending = payload;
            interop.ShowShareUIForWindow(windowHandle);
            await payload.Completion.Task;
        }
        catch
        {
            if (payload is null) lease.Dispose();
            else Abort(payload);
            throw;
        }
    }

    private void OnDataRequested(DataTransferManager sender, DataRequestedEventArgs args)
    {
        if (pending is not { } payload) return;
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
        package.ShareCompleted += (_, _) => Complete(payload);
        package.ShareCanceled += (_, _) => Complete(payload);
    }

    public void Dispose()
    {
        manager.DataRequested -= OnDataRequested;
        if (pending is { } payload) Abort(payload);
        staging.Dispose();
    }

    private void Complete(SharePayload payload)
    {
        if (payload.Descriptor.FilePath is { } path) staging.DeletePayload(path);
        if (pending?.Id == payload.Id) pending = null;
        payload.Lease.Dispose();
        payload.Completion.TrySetResult();
    }

    private void Abort(SharePayload payload)
    {
        if (payload.Descriptor.FilePath is { } path) staging.DeletePayload(path);
        if (pending?.Id == payload.Id) pending = null;
        payload.Lease.Dispose();
        payload.Completion.TrySetResult();
    }

    private sealed record SharePayload(
        Guid Id,
        SharePayloadDescriptor Descriptor,
        StorageFile? File,
        IDisposable Lease,
        TaskCompletionSource Completion);
}
