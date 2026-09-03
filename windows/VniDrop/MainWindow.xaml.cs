using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;
using VniDrop.ViewModels;
using VniDrop.Views;

namespace VniDrop;

public sealed partial class MainWindow : Window
{
    public AppViewModel Model { get; }
    private readonly DispatcherQueueTimer refreshTimer;
    private readonly HashSet<string> shownRequests = [];
    private readonly SemaphoreSlim dialogs = new(1);
    private bool checkingRequests;
    private bool closing;
    private bool canClose;
    private readonly Queue<string> invitations;
    private bool openingInvitations;
    private ContentDialog? activeDialog;
    private readonly NativeNotifications notifications = new();
    private bool active;
    public NativeShare NativeShare { get; }

    public MainWindow(string profile, string[] invitations)
    {
        App.Window = this;
        Model = new(profile); this.invitations = new(invitations);
        InitializeComponent();
        NativeShare = new(this);
        AppWindow.Resize(new Windows.Graphics.SizeInt32(1200, 900));
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets/app-icon.ico"));
        SystemBackdrop = new MicaBackdrop();
        refreshTimer = DispatcherQueue.CreateTimer(); refreshTimer.Interval = TimeSpan.FromMilliseconds(350);
        refreshTimer.Tick += async (_, _) => await Model.RefreshAsync();
        Model.Updated += OnUpdated;
        Activated += (_, args) => active = args.WindowActivationState != WindowActivationState.Deactivated;
        AppWindow.Closing += OnClosing;
        Root.Loaded += async (_, _) => await StartAsync();
        Navigation.SelectedItem = Navigation.MenuItems[0];
    }
    private async Task StartAsync(bool reset = false)
    {
        await Model.StartAsync(reset);
        if (!Model.Ready) return;
        Startup.Visibility = Visibility.Collapsed;
        ApplyAppearance(); refreshTimer.Start();
        await DrainInvitationsAsync();
    }
    public void ApplyAppearance()
    {
        Root.RequestedTheme = Model.Preferences.Theme switch
        { "Light" => ElementTheme.Light, "Dark" => ElementTheme.Dark, _ => ElementTheme.Default };
        AppWindow.TitleBar.PreferredTheme = Model.Preferences.Theme switch
        { "Light" => Microsoft.UI.Windowing.TitleBarTheme.Light, "Dark" => Microsoft.UI.Windowing.TitleBarTheme.Dark, _ => Microsoft.UI.Windowing.TitleBarTheme.UseDefaultAppMode };
    }

    private void Navigate(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.IsSettingsSelected) { ShowPage(new SettingsPage()); return; }
        if (args.SelectedItemContainer?.Tag is not string tag) return;
        ShowPage(tag switch { "send" => new TransfersPage(false), "receive" => new TransfersPage(true), "devices" => new DevicesPage(), _ => new TransfersPage(false) });
    }
    public void ShowPage(Page page) => ContentFrame.Content = page;
    public void ShowTransfers(bool receive)
    {
        Navigation.SelectedItem = Navigation.MenuItems[receive ? 1 : 0];
        ShowPage(new TransfersPage(receive));
    }
    public async Task ShowDraftAsync(SavedDevice? receiver = null, IReadOnlyList<DraftSource>? sources = null)
    {
        if (activeDialog is not null || !Model.Ready) return;
        var dialog = new DraftPage(receiver);
        if (sources is not null) dialog.Select(sources);
        await ShowDialogAsync(dialog);
        if (dialog.Result is ShareResult share && Model.Outgoing.FirstOrDefault(t => t.Transfer.transferId == share.transferId) is { } item)
        {
            ShowTransfers(false);
            ShowPage(new TransferDetailsPage(item));
            await new TransferDetailsPageActions(item).ShareAsync();
        }
        else if (dialog.Result is not null)
        {
            Navigation.SelectedItem = NavDevices;
            ShowPage(new DevicesPage());
        }
    }
    public async Task ShowReceiveAsync(string? ticket = null) => await ShowDialogAsync(new ReceivePage(ticket));
    public void EnableNavigation(bool enabled)
    {
        foreach (var item in Navigation.MenuItems.OfType<NavigationViewItem>()) item.IsEnabled = enabled;
        if (Navigation.SettingsItem is NavigationViewItem settings) settings.IsEnabled = enabled;
    }
    public async Task OpenInvitationAsync(string path)
    {
        invitations.Enqueue(path);
        await DrainInvitationsAsync();
    }
    private async Task DrainInvitationsAsync()
    {
        if (openingInvitations || !Model.Ready) return;
        openingInvitations = true;
        try
        {
            while (Model.Ready && invitations.TryDequeue(out var path))
                await Model.PerformAsync(async () =>
                {
                    var text = await InvitationDocument.ReadAsync(path);
                    Navigation.SelectedItem = Navigation.MenuItems[1];
                    await ShowReceiveAsync(text);
                });
        }
        finally { openingInvitations = false; }
    }
    public async Task<ContentDialogResult> DialogAsync(string title, object content, string primary, string? secondary = null)
    {
        return await ShowDialogAsync(new ContentDialog { Title = title, Content = content,
            PrimaryButtonText = primary, SecondaryButtonText = secondary ?? "", CloseButtonText = Strings.Get("button_cancel") });
    }
    public async Task<ContentDialogResult> ShowDialogAsync(ContentDialog dialog)
    {
        await dialogs.WaitAsync();
        try
        {
            activeDialog = dialog;
            dialog.XamlRoot = Root.XamlRoot; dialog.RequestedTheme = Root.RequestedTheme;
            return await dialog.ShowAsync();
        }
        finally { activeDialog = null; dialogs.Release(); }
    }

    private async void OnUpdated()
    {
        if (Model.Snapshot is { } snapshot) notifications.Update(snapshot, Model.Preferences.Notifications && !active);
        await DrainInvitationsAsync();
        if (Model.Ready && !checkingRequests && !closing) await CheckRequestsAsync();
    }
    private async Task CheckRequestsAsync()
    {
        if (Model.Snapshot is not { } snapshot) return;
        checkingRequests = true;
        try
        {
            foreach (var request in snapshot.Requests.Where(r => r.status == "requested"))
            {
                if (!shownRequests.Add("invitation:" + request.id)) continue;
                var result = await DialogAsync(Strings.Get("approval_connection_request"), Strings.Format("approval_request_body",
                    ("receiver", request.receiverName ?? request.receiverDeviceName ?? Strings.Get("approval_nearby_device")), ("transferName", request.transferName)),
                    Strings.Get("button_approve"), Strings.Get("button_refuse"));
                if (Model.Ready && !closing && result != ContentDialogResult.None)
                {
                    var success = await Model.PerformAsync(() => Model.Session.RunAsync(c => c.RespondReceiverRequest(request.id, result == ContentDialogResult.Primary, null)));
                    if (!success) shownRequests.Remove("invitation:" + request.id);
                }
            }
            foreach (var offer in snapshot.Offers)
            {
                if (!shownRequests.Add("offer:" + offer.transferId)) continue;
                var sender = snapshot.Devices.FirstOrDefault(d => d.endpointId == offer.senderEndpointId);
                var name = sender?.localLabel ?? sender?.remoteDisplayName ?? Strings.Get("saved_devices_unnamed");
                var result = await DialogAsync(Strings.Get("receive_review_title"), $"{name}\n{offer.transferName}\n{Strings.Format("saved_devices_transfer_files", ("count", offer.fileCount), ("size", Strings.Size(offer.totalSize)))}",
                    Strings.Get("button_receive"), Strings.Get("button_refuse"));
                if (!Model.Ready || closing || result == ContentDialogResult.None) continue;
                var success = await Model.PerformAsync(async () =>
                {
                    var response = await Model.Session.RunAsync(c => c.RespondToTargetedOffer(offer.transferId, result == ContentDialogResult.Primary));
                    if (response is TargetedOfferResponse.Approved approved)
                        _ = Model.PerformAsync(() => Model.Session.RunAsync(c => c.ReceiveTargetedTransfer(approved.transferId, Model.Preferences.ReceiveDirectory)));
                });
                if (!success) shownRequests.Remove("offer:" + offer.transferId);
            }
        }
        finally { checkingRequests = false; }
    }
    private async void ReviewRequests(object sender, RoutedEventArgs e)
    {
        ShowPage(new DevicesPage());
        shownRequests.Clear();
        if (!checkingRequests) await CheckRequestsAsync();
    }
    private void ErrorClosed(InfoBar sender, InfoBarClosedEventArgs args) => Model.ClearError();
    private async void RetryStartup(object sender, RoutedEventArgs e) => await StartAsync();
    private async void ResetIdentity(object sender, RoutedEventArgs e)
    {
        if (await DialogAsync(Strings.Get("app_identity_reset_title"), Strings.Get("app_identity_reset_confirmation"), Strings.Get("app_identity_reset_action")) == ContentDialogResult.Primary)
            await StartAsync(true);
    }
    private async void OnClosing(Microsoft.UI.Windowing.AppWindow sender, Microsoft.UI.Windowing.AppWindowClosingEventArgs args)
    {
        if (canClose) return;
        args.Cancel = true;
        if (activeDialog is not null) { activeDialog.Hide(); return; }
        if (closing || Model.Maintaining) return;
        closing = true;
        try
        {
            if (Model.Ready)
            {
                var facts = await Model.Session.RunAsync(c => c.RuntimeObligationFacts());
                if (facts.activeInvitationTransfers + facts.activeTargetedTransfers + facts.invitationProviderAvailability + facts.targetedProviderAvailability + facts.targetedPreparations > 0 &&
                    await DialogAsync(Strings.Get("windows_close_title"), Strings.Get("windows_close_body"), Strings.Get("button_close")) != ContentDialogResult.Primary) return;
            }
            refreshTimer.Stop(); NativeShare.Dispose(); await Model.Session.DisposeAsync();
            canClose = true; Close();
        }
        catch (Exception ex) { Model.Report(ex); }
        finally { closing = false; }
    }
}
