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

    private void PreventCloseWhileBusy(ContentDialog sender, ContentDialogClosingEventArgs args) =>
        args.Cancel = actionRunning;

    private void DialogClosed(ContentDialog sender, ContentDialogClosedEventArgs args) => closed = true;

    public void RequestYieldForAttention()
    {
        if (closed || yieldRequested) return;
        yieldRequested = true;
        if (!actionRunning) Hide();
    }

    private async void ShareInvitation(ContentDialog sender, ContentDialogButtonClickEventArgs args) =>
        await RunActionAsync(args, () => App.Window.NativeShare.ShowInvitationAsync(ticket, transferName));

    private async void SaveInvitation(ContentDialog sender, ContentDialogButtonClickEventArgs args) =>
        await RunActionAsync(args, () => WindowsFiles.SaveInvitationAsync(ticket, transferName));

    private async Task RunActionAsync(ContentDialogButtonClickEventArgs args, Func<Task> action)
    {
        args.Cancel = true;
        if (actionRunning) return;

        actionRunning = true;
        var deferral = args.GetDeferral();
        IsPrimaryButtonEnabled = false;
        IsSecondaryButtonEnabled = false;
        CloseButtonText = "";
        ActionError.IsOpen = false;
        ActionStatus.Visibility = Visibility.Visible;
        Announce(ActionStatusText);

        try
        {
            await action();
        }
        catch (Exception ex)
        {
            ActionError.Message = Strings.Error(ex);
            ActionError.IsOpen = true;
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
            deferral.Complete();
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
