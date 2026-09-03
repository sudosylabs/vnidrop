using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Platform;
using VniDrop.ViewModels;
using Windows.ApplicationModel.DataTransfer;

namespace VniDrop.Views;

public sealed partial class TransfersPage : Page
{
    public AppViewModel Model => App.Window.Model;
    public ObservableCollection<TransferItem> Items => receive ? Model.Incoming : Model.Outgoing;
    public string Heading => Strings.Get(receive ? "receive_title" : "send_title");
    public string ListHeading => Strings.Get(receive ? "receive_history_title" : "send_transfers_title");
    public string PrimaryLabel => Strings.Get(receive ? "button_receive_files" : "button_create_new_transfer");
    public string EmptyTitle => Strings.Get(receive ? "receive_empty_title" : "send_empty_title");
    public string EmptyBody => Strings.Get(receive ? "receive_empty_body" : "send_empty_body");
    public string EmptyGlyph => receive ? "\uE896" : "\uE724";
    private readonly bool receive;
    public TransfersPage(bool receive)
    {
        this.receive = receive; InitializeComponent();
        HistoryActions.Visibility = receive ? Visibility.Visible : Visibility.Collapsed;
        Loaded += (_, _) => { Model.Updated += Update; Update(); };
        Unloaded += (_, _) => Model.Updated -= Update;
        SizeChanged += (_, e) =>
        {
            var narrow = e.NewSize.Width < 580;
            Grid.SetColumn(CreateTransfer, narrow ? 0 : 1); Grid.SetRow(CreateTransfer, narrow ? 1 : 0);
            CreateTransfer.HorizontalAlignment = narrow ? HorizontalAlignment.Left : HorizontalAlignment.Right;
            ((Grid)Content).Width = Math.Min(1120, e.NewSize.Width);
        };
        var keyboard = new Microsoft.UI.Xaml.Input.KeyboardAccelerator { Key = Windows.System.VirtualKey.N, Modifiers = Windows.System.VirtualKeyModifiers.Control };
        keyboard.Invoked += (_, args) => { Create(this, new()); args.Handled = true; }; KeyboardAccelerators.Add(keyboard);
    }
    private void Update()
    {
        Empty.Visibility = Items.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        CreateTransfer.Visibility = ListHeader.Visibility = Items.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        ClearHistoryButton.IsEnabled = Items.Any(t => t.CanDelete);
    }
    private async void Create(object sender, RoutedEventArgs e)
    { if (receive) await App.Window.ShowReceiveAsync(); else await App.Window.ShowDraftAsync(); }
    private void OpenTransfer(object sender, ItemClickEventArgs e) => App.Window.ShowPage(new TransferDetailsPage((TransferItem)e.ClickedItem));
    private void RowChanged(ListViewBase sender, ContainerContentChangingEventArgs args)
    { if (args.Item is TransferItem item) AutomationProperties.SetName(args.ItemContainer, item.AutomationName); }
    private async void ShareTransfer(object sender, RoutedEventArgs e) => await new TransferDetailsPageActions((TransferItem)((MenuFlyoutItem)sender).Tag).ShareAsync();
    private async void StopTransfer(object sender, RoutedEventArgs e) => await new TransferDetailsPageActions((TransferItem)((MenuFlyoutItem)sender).Tag).StopAsync();
    private async void DeleteTransfer(object sender, RoutedEventArgs e) => await new TransferDetailsPageActions((TransferItem)((MenuFlyoutItem)sender).Tag).DeleteAsync();
    private void OpenFolder(object sender, RoutedEventArgs e)
    { try { WindowsFiles.OpenFolder(Model.Preferences.ReceiveDirectory); } catch (Exception ex) { Model.Report(ex); } }
    private async void ClearHistory(object sender, RoutedEventArgs e)
    {
        if (await App.Window.DialogAsync(Strings.Get("receive_clear_history_title"), Strings.Get("receive_clear_history_description"), Strings.Get("button_clear")) == ContentDialogResult.Primary)
            await Model.PerformAsync(() => Model.Session.RunAsync(c => c.DeleteReceiveHistory()));
    }
    private void DragOverFiles(object sender, DragEventArgs e)
    {
        if (Model.Ready && e.DataView.Contains(StandardDataFormats.StorageItems)) e.AcceptedOperation = DataPackageOperation.Copy;
    }
    private async void DropFiles(object sender, DragEventArgs e)
    {
        var deferral = e.GetDeferral();
        try
        {
            if (!Model.Ready || !e.DataView.Contains(StandardDataFormats.StorageItems)) return;
            var items = await e.DataView.GetStorageItemsAsync();
            var sources = receive ? null : items.Select(i => WindowsFiles.Source(i.Path)).ToArray();
            deferral.Complete(); deferral = null;
            if (receive) { if (items.Count == 1) await App.Window.OpenInvitationAsync(items[0].Path); }
            else await App.Window.ShowDraftAsync(sources: sources);
        }
        catch (Exception ex) { Model.Report(ex); }
        finally { deferral?.Complete(); }
    }
}
