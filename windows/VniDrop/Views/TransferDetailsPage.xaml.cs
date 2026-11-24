using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Platform;
using VniDrop.ViewModels;

namespace VniDrop.Views;

public sealed partial class TransferDetailsPage : Page
{
    public TransferItem Item { get; private set; } = null!;
    private InvitationStatus? artifactStatus;
    private TransferDetailsPageActions actions = null!;
    public TransferDetailsPage()
    {
        InitializeComponent();
        Loaded += (_, _) => { App.Window.Model.Updated += Update; Update(); };
        Unloaded += (_, _) => App.Window.Model.Updated -= Update;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        Item = (TransferItem)e.Parameter;
        actions = new(Item);
        Bindings.Update();
    }
    private async void Update()
    {
        var send = Item.Direction == InvitationDirection.Send;
        ShareButton.Visibility = send && Item.LifecycleStatus is InvitationStatus.Sharing or InvitationStatus.Importing ? Visibility.Visible : Visibility.Collapsed;
        AccessRow.Visibility = send ? Visibility.Visible : Visibility.Collapsed;
        ReceiversButton.Visibility = send ? Visibility.Visible : Visibility.Collapsed;
        Destinations.Visibility = send ? Visibility.Visible : Visibility.Collapsed;
        StopButton.Visibility = Item.CanStop ? Visibility.Visible : Visibility.Collapsed;
        DeleteButton.Visibility = Item.CanDelete ? Visibility.Visible : Visibility.Collapsed;
        ActionsDivider.Visibility = Item.CanStop && Item.CanDelete ? Visibility.Visible : Visibility.Collapsed;
        ActionsSection.Visibility = Item.CanStop || Item.CanDelete ? Visibility.Visible : Visibility.Collapsed;
        StopButton.Title = Strings.Get(TransferPresentation.StopActionKey(Item.Direction));
        ActivityButton.Value = Item.ActivityCount > 0 ? Item.ActivityCount.ToString() : "";
        ReceiversButton.Value = Item.ReceiverCount > 0 ? Item.ReceiverCount.ToString() : "";
        ReceiversButton.Description = Item.ReceiverSummary;
        if (artifactStatus != Item.LifecycleStatus) { artifactStatus = Item.LifecycleStatus; await LoadFilesAsync(); }
    }
    private async Task LoadFilesAsync()
    {
        FilesSection.Visibility = Item.Direction == InvitationDirection.Receive ? Visibility.Visible : Visibility.Collapsed;
        if (Item.Direction != InvitationDirection.Receive) return;
        await App.Window.Model.PerformAsync(async () =>
        {
            var artifacts = await App.Window.Model.Session.RunAsync(c => c.ListReceivedArtifacts());
            Files.ItemsSource = artifacts.Where(a => a.transferLocalId == Item.Transfer.localId).Select(a =>
            {
                var button = new HyperlinkButton { Content = a.relativePath };
                button.Click += (_, _) => { try { WindowsFiles.OpenFolder(Path.GetDirectoryName(a.locator)!); } catch (Exception ex) { App.Window.Model.Report(ex); } };
                return button;
            }).ToArray();
        });
    }
    private void Back(object sender, RoutedEventArgs e)
    {
        if (!App.Window.GoBack()) App.Window.ShowTransfers(Item.Direction == InvitationDirection.Receive);
    }
    private async void Share(object sender, RoutedEventArgs e) => await actions.ShareAsync();
    private async void Activity(object sender, RoutedEventArgs e) => await actions.ActivityAsync();
    private async void Receivers(object sender, RoutedEventArgs e) => await actions.ReceiversAsync();
    private async void Stop(object sender, RoutedEventArgs e) => await actions.StopAsync();
    private async void Delete(object sender, RoutedEventArgs e)
    { if (await actions.DeleteAsync()) Back(sender, e); }
}
