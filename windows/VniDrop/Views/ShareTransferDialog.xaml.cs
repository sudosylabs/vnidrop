using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using QRCoder;
using VniDrop.Platform;
using VniDrop.Services;
using Windows.Storage.Streams;

namespace VniDrop.Views;

public sealed partial class ShareTransferDialog : ContentDialog, IAttentionYieldingDialog
{
    private readonly string ticket;
    private readonly string transferName;
    private bool qrLoadingStarted;
    private bool actionRunning;
    private bool closed;
    private bool yieldRequested;
    private CancellationTokenSource? nativeShareCancellation;

    public ShareTransferDialog(string ticket, string transferName)
    {
        this.ticket = ticket;
        this.transferName = transferName;
        InitializeComponent();
    }

    private async void LoadQr(ContentDialog sender, ContentDialogOpenedEventArgs args)
    {
        if (qrLoadingStarted) return;
        qrLoadingStarted = true;

        try
        {
            var bytes = await Task.Run(() => PngByteQRCodeHelper.GetQRCode(ticket, QRCodeGenerator.ECCLevel.L, 6));
            if (closed) return;
            using var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
            {
                writer.WriteBytes(bytes);
                await writer.StoreAsync();
            }
            if (closed) return;

            var bitmap = new BitmapImage();
            await bitmap.SetSourceAsync(stream);
            if (closed) return;
            QrImage.Source = bitmap;
            QrImage.Visibility = Visibility.Visible;
            SetLiveStatus(QrCaption, Strings.Get("transfer_scan_qr"));
        }
        catch
        {
            if (closed) return;
            QrFrame.Visibility = Visibility.Collapsed;
            QrCaption.Visibility = Visibility.Collapsed;
            QrError.IsOpen = true;
        }
        finally
        {
            if (!closed)
            {
                QrProgress.IsActive = false;
                QrProgress.Visibility = Visibility.Collapsed;
            }
        }
    }

    private void DialogClosing(ContentDialog sender, ContentDialogClosingEventArgs args)
    {
        args.Cancel = actionRunning && nativeShareCancellation is null;
        nativeShareCancellation?.Cancel();
    }

    private void DialogClosed(ContentDialog sender, ContentDialogClosedEventArgs args)
    {
        closed = true;
        nativeShareCancellation?.Cancel();
    }

    public void RequestYieldForAttention()
    {
        if (closed || yieldRequested) return;
        yieldRequested = true;
        if (!actionRunning || nativeShareCancellation is not null) Hide();
    }

    private async void ShareInvitation(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        args.Cancel = true;
        if (actionRunning) return;

        using var cancellation = new CancellationTokenSource();
        nativeShareCancellation = cancellation;
        try
        {
            await RunActionAsync(() => App.Window.NativeShare.ShowInvitationAsync(ticket, transferName, cancellation.Token), cancellation.Token);
        }
        finally
        {
            nativeShareCancellation = null;
        }
    }

    private async void SaveInvitation(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        args.Cancel = true;
        if (actionRunning) return;
        var deferral = args.GetDeferral();
        try { await RunActionAsync(() => WindowsFiles.SaveInvitationAsync(ticket, transferName)); }
        finally { deferral.Complete(); }
    }

    private async Task RunActionAsync(Func<Task> action, CancellationToken cancellationToken = default)
    {
        actionRunning = true;
        IsPrimaryButtonEnabled = false;
        IsSecondaryButtonEnabled = false;
        if (nativeShareCancellation is null) CloseButtonText = "";
        ActionError.IsOpen = false;
        ActionStatus.Visibility = Visibility.Visible;
        Announce(ActionStatusText);

        try
        {
            await action();
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception ex)
        {
            if (!closed && !cancellationToken.IsCancellationRequested)
            {
                ActionError.Message = Strings.Error(ex);
                ActionError.IsOpen = true;
            }
        }
        finally
        {
            if (!closed)
            {
                ActionStatus.Visibility = Visibility.Collapsed;
                IsPrimaryButtonEnabled = true;
                IsSecondaryButtonEnabled = true;
                CloseButtonText = Strings.Get("button_close");
            }
            actionRunning = false;
            if (yieldRequested && !closed)
            {
                DispatcherQueue.TryEnqueue(() =>
                {
                    if (!closed && !actionRunning) Hide();
                });
            }
        }
    }

    private static void SetLiveStatus(TextBlock element, string status)
    {
        element.Text = status;
        Announce(element);
    }

    private static void Announce(TextBlock element)
    {
        var peer = FrameworkElementAutomationPeer.FromElement(element)
            ?? FrameworkElementAutomationPeer.CreatePeerForElement(element);
        peer?.RaiseAutomationEvent(AutomationEvents.LiveRegionChanged);
    }
}
