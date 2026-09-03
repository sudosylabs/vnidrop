using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Native;
using VniDrop.Platform;

namespace VniDrop.Views;

public sealed partial class ReceivePage : ContentDialog
{
    private TicketInspection? inspection;
    private bool inspecting;
    private bool picking;
    private bool receiving;
    private bool closeRequested;
    private bool cancelling;
    private bool finished;
    private string invitation = "";
    public ReceivePage(string? invitation = null)
    {
        InitializeComponent();
        this.invitation = invitation ?? "";
        Heading.Text = Strings.Get("receive_choose_method_title");
        ReceiverName.Text = App.Window.Model.Preferences.Username;
        Destination.Text = App.Window.Model.Preferences.ReceiveDirectory;
        Opened += async (_, _) =>
        {
            App.Window.Model.Updated += UpdateProgress;
            if (this.invitation.Length > 0) await InspectAsync();
        };
        Closed += (_, _) => App.Window.Model.Updated -= UpdateProgress;
    }
    private void Back(object sender, RoutedEventArgs e)
    {
        inspection = null; invitation = ""; Render(); Error.IsOpen = false;
    }
    private void Render()
    {
        var reviewing = inspection is not null;
        Acquisition.Visibility = reviewing ? Visibility.Collapsed : Visibility.Visible;
        Acquisition.IsHitTestVisible = !inspecting && !picking; Acquisition.Opacity = inspecting || picking ? .6 : 1;
        Review.Visibility = BackButton.Visibility = reviewing ? Visibility.Visible : Visibility.Collapsed;
        BackButton.IsEnabled = ReceiverName.IsEnabled = !receiving;
        Heading.Text = Strings.Get(reviewing || inspecting ? "receive_review_title" : "receive_choose_method_title");
        Busy.Visibility = inspecting ? Visibility.Visible : Visibility.Collapsed; Busy.IsActive = inspecting;
        PrimaryButtonText = reviewing && !receiving ? Strings.Get("button_receive") : "";
        IsPrimaryButtonEnabled = reviewing && !inspecting && !receiving;
        CloseButtonText = Strings.Get(receiving ? "button_cancel_receive" : "button_cancel");
        Receiving.Visibility = receiving ? Visibility.Visible : Visibility.Collapsed;
    }
    private async void OpenInvitation(object sender, RoutedEventArgs e)
    {
        if (picking || inspecting) return;
        picking = true; Render();
        try { var ticket = await WindowsFiles.OpenInvitationAsync(); if (ticket is not null) { invitation = ticket; await InspectAsync(); } }
        catch (Exception ex) { ShowError(ex); }
        finally { picking = false; Render(); }
    }
    private async Task InspectAsync()
    {
        if (inspecting || receiving) return;
        inspecting = true; Error.IsOpen = false; Render();
        try
        {
            var ticket = invitation.Trim();
            if (ticket.Length == 0 || System.Text.Encoding.UTF8.GetByteCount(ticket) > Core.InvitationDocument.MaximumBytes) throw new InvalidDataException("error_invalid_ticket");
            inspection = await App.Window.Model.Session.RunAsync(c => c.InspectTicket(ticket));
            TransferTitle.Text = inspection.metadata.transferName;
            Summary.Text = Strings.FileSummary(inspection.metadata.fileCount, inspection.metadata.totalSize);
            Sender.Text = inspection.metadata.senderName ?? "";
            Sender.Visibility = string.IsNullOrWhiteSpace(Sender.Text) ? Visibility.Collapsed : Visibility.Visible;
        }
        catch (Exception ex) { inspection = null; ShowError(ex); }
        finally { inspecting = false; Render(); }
    }
    private async void Receive(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        args.Cancel = true;
        if (inspection is null || receiving) return;
        receiving = true; Error.IsOpen = false; Render();
        ProgressLabel.Text = Strings.Get("progress_connecting");
        try
        {
            var ticket = invitation.Trim(); var name = ReceiverName.Text.Trim(); var directory = App.Window.Model.Preferences.ReceiveDirectory;
            await App.Window.Model.Session.RunAsync(c => c.Receive(ticket, directory, name));
            finished = true;
        }
        catch (Exception ex) { if (!closeRequested) ShowError(ex); }
        finally
        {
            receiving = false; await App.Window.Model.RefreshAsync(true); Render();
            if (finished || closeRequested) Hide();
        }
    }
    private void UpdateProgress()
    {
        if (!receiving || inspection is null) return;
        var item = App.Window.Model.Incoming.FirstOrDefault(t => t.Transfer.transferId == inspection.metadata.transferId);
        if (item is not null)
        {
            ReceiveProgress.IsIndeterminate = item.IsIndeterminate; ReceiveProgress.Value = item.Progress;
            ProgressLabel.Text = item.ProgressText;
            if (closeRequested) _ = CancelAsync();
        }
    }
    private async Task CancelAsync()
    {
        if (cancelling || inspection is null) return;
        var item = App.Window.Model.Incoming.FirstOrDefault(t => t.Transfer.transferId == inspection.metadata.transferId && t.CanStop);
        if (item is null) return;
        cancelling = true;
        try { await App.Window.Model.Session.RunAsync(c => c.CancelTransfer(item.Transfer.transferId)); }
        catch (Exception ex) { ShowError(ex); }
        finally { cancelling = false; }
    }
    private void OnClosing(ContentDialog sender, ContentDialogClosingEventArgs args)
    {
        args.Cancel = inspecting || picking || receiving;
        if (receiving) { closeRequested = true; _ = CancelAsync(); }
    }
    private void ShowError(Exception ex) { Error.Message = Strings.Error(ex); Error.IsOpen = true; }
}
