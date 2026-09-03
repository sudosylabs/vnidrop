using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;
using VniDrop.ViewModels;
using Windows.ApplicationModel.DataTransfer;

namespace VniDrop.Views;

public sealed partial class DraftPage : ContentDialog
{
    private readonly TransferDraft draft;
    private bool rendering;
    private bool picking;
    private bool choosing;
    public object? Result { get; private set; }
    public DraftPage(SavedDevice? receiver = null)
    {
        draft = new(receiver); InitializeComponent(); SenderName.Text = App.Window.Model.Preferences.Username;
        Recipient.Visibility = receiver is null ? Visibility.Collapsed : Visibility.Visible;
        Recipient.Text = receiver is null ? "" : Strings.Format("saved_devices_transfer_direction_outgoing", ("device", receiver.localLabel ?? receiver.remoteDisplayName ?? Strings.Get("saved_devices_unnamed")));
        SenderName.Visibility = AccessOptions.Visibility = receiver is null ? Visibility.Visible : Visibility.Collapsed;
        Render();
    }
    private string MultipleName(int count) => Strings.Format("send_default_transfer_name", ("count", count));
    public void Select(IReadOnlyList<DraftSource> sources) { draft.Select(sources, MultipleName); if (sources.Count > 0) choosing = false; Render(); }
    private void Render()
    {
        rendering = true;
        var review = draft.Sources.Count > 0 && !choosing;
        Heading.Text = Strings.Get(review ? "send_review_title" : "send_choose_file_title");
        if (TransferName.Text != draft.Name) TransferName.Text = draft.Name;
        Sources.ItemsSource = draft.Sources.Select(s => new DraftSourceItem(s)).ToArray();
        SelectionSummary.Text = Strings.Format("send_selected_files_count", ("count", draft.Sources.Count));
        SelectionSummary.Visibility = draft.Sources.Count > 1 ? Visibility.Visible : Visibility.Collapsed;
        ChooseStep.Visibility = review ? Visibility.Collapsed : Visibility.Visible;
        ReviewStep.Visibility = BackButton.Visibility = review ? Visibility.Visible : Visibility.Collapsed;
        ReviewStep.IsHitTestVisible = ChooseStep.IsHitTestVisible = BackButton.IsEnabled = !draft.IsSubmitting && !picking;
        ReviewStep.Opacity = ChooseStep.Opacity = draft.IsSubmitting || picking ? .6 : 1;
        PrimaryButtonText = review ? Strings.Get(draft.Receiver is null ? "button_share_file" : "saved_devices_send_action") : "";
        IsPrimaryButtonEnabled = review && !draft.IsSubmitting && !picking && !string.IsNullOrWhiteSpace(draft.Name);
        Preparation.Visibility = draft.IsSubmitting ? Visibility.Visible : Visibility.Collapsed;
        rendering = false;
    }
    private void NameChanged(object sender, TextChangedEventArgs e)
    {
        if (rendering) return;
        draft.Rename(TransferName.Text);
        IsPrimaryButtonEnabled = !draft.IsSubmitting && !picking && draft.Sources.Count > 0 && !string.IsNullOrWhiteSpace(draft.Name);
    }
    private void AccessChanged(object sender, RoutedEventArgs e) { if (PublicWarning is not null) PublicWarning.IsOpen = Anyone.IsChecked == true; }
    private async Task PickAsync(bool folder)
    {
        if (picking || draft.IsSubmitting) return;
        picking = true; Error.IsOpen = false; Render();
        try { Select(await WindowsFiles.PickAsync(folder)); }
        catch (Exception ex) { Error.Message = Strings.Error(ex); Error.IsOpen = true; }
        finally { picking = false; Render(); }
    }
    private async void ChooseFiles(object sender, RoutedEventArgs e) => await PickAsync(false);
    private async void ChooseFolder(object sender, RoutedEventArgs e) => await PickAsync(true);
    private void ChooseAgain(object sender, RoutedEventArgs e) { choosing = true; Render(); }
    private void ClearSelection(object sender, RoutedEventArgs e) { draft.Clear(); Render(); }
    private void RemoveSource(object sender, RoutedEventArgs e) { draft.Remove((DraftSource)((Button)sender).Tag, MultipleName); Render(); }
    private void OnClosing(ContentDialog sender, ContentDialogClosingEventArgs args) => args.Cancel = draft.IsSubmitting || picking;
    private async void Submit(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        args.Cancel = true;
        var deferral = args.GetDeferral();
        try
        {
            Error.IsOpen = false;
            var pending = draft.SubmitAsync(App.Window.Model.Session, SenderName.Text.Trim(), Approval.IsChecked == true);
            Render(); Result = await pending; await App.Window.Model.RefreshAsync(true); args.Cancel = false;
        }
        catch (Exception ex) { Error.Message = Strings.Error(ex); Error.IsOpen = true; }
        finally { Render(); deferral.Complete(); }
    }
    private void DragOverFiles(object sender, DragEventArgs e)
    { if (!draft.IsSubmitting && !picking && e.DataView.Contains(StandardDataFormats.StorageItems)) e.AcceptedOperation = DataPackageOperation.Copy; }
    private async void DropFiles(object sender, DragEventArgs e)
    {
        var deferral = e.GetDeferral();
        try { if (!draft.IsSubmitting && !picking) Select((await e.DataView.GetStorageItemsAsync()).Select(i => WindowsFiles.Source(i.Path)).ToArray()); }
        catch (Exception ex) { Error.Message = Strings.Error(ex); Error.IsOpen = true; }
        finally { deferral.Complete(); }
    }
}
