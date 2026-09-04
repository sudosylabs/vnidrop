using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;
using VniDrop.Services;
using VniDrop.ViewModels;
using VniDrop.Views;
using VniDrop.Views.Settings;

namespace VniDrop;

public sealed partial class MainWindow : Window
{
    public AppViewModel Model { get; }
    private readonly DispatcherQueueTimer refreshTimer;
    private readonly HashSet<string> shownRequests = [];
    private bool checkingRequests;
    private bool closing;
    private bool canClose;
    private readonly Queue<string> invitations;
    private bool openingInvitations;
    private readonly NativeNotifications notifications = new();
    private bool active;
    public NativeShare NativeShare { get; }
    public DialogService Dialogs { get; }

    public MainWindow(string profile, string[] invitations)
    {
        App.Window = this;
        Model = new(profile); this.invitations = new(invitations);
        InitializeComponent();
        Navigation.IsEnabled = false;
        Model.PropertyChanged += ModelPropertyChanged;
        Dialogs = new(Root);
        NativeShare = new(this);
        AppWindow.Resize(new Windows.Graphics.SizeInt32(1200, 900));
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets/app-icon.ico"));
        SystemBackdrop = new MicaBackdrop();
        refreshTimer = DispatcherQueue.CreateTimer(); refreshTimer.Interval = TimeSpan.FromMilliseconds(350);
        refreshTimer.Tick += async (_, _) => await Model.RefreshAsync();
        Model.Updated += OnUpdated;
        var back = new Microsoft.UI.Xaml.Input.KeyboardAccelerator
        {
            Key = Windows.System.VirtualKey.Left,
            Modifiers = Windows.System.VirtualKeyModifiers.Menu,
        };
        back.Invoked += (_, args) => args.Handled = GoBack();
        Root.KeyboardAccelerators.Add(back);
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

    private void ModelPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        if (args.PropertyName is not (nameof(AppViewModel.Ready) or nameof(AppViewModel.Maintaining))) return;
        if (DispatcherQueue.HasThreadAccess) UpdateInteractionState();
        else DispatcherQueue.TryEnqueue(UpdateInteractionState);
    }

    private void UpdateInteractionState()
    {
        Navigation.IsEnabled = CanNavigate;
        ContentFrame.IsEnabled = CanNavigate;
    }

    private bool CanNavigate => WindowInteractionPolicy.AllowsNavigation(Model.Ready, Model.Maintaining, closing);

    private void Navigate(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.IsSettingsSelected)
        {
            NavigateRoot(typeof(SettingsPage));
            return;
        }
        if (args.SelectedItemContainer?.Tag is not string tag) return;
        NavigateRoot(tag);
    }

    private void NavigateInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
    {
        if (!ContentFrame.CanGoBack) return;
        if (args.IsSettingsInvoked && ReferenceEquals(sender.SelectedItem, sender.SettingsItem))
        {
            NavigateRoot(typeof(SettingsPage));
            return;
        }
        if (args.InvokedItemContainer?.Tag is string tag
            && sender.SelectedItem is NavigationViewItem selected
            && string.Equals(selected.Tag as string, tag, StringComparison.Ordinal))
            NavigateRoot(tag);
    }

    private void NavigateRoot(string tag)
    {
        switch (tag)
        {
            case "send": NavigateRoot(typeof(TransfersPage), false); break;
            case "receive": NavigateRoot(typeof(TransfersPage), true); break;
            case "devices": NavigateRoot(typeof(DevicesPage)); break;
        }
    }

    private void NavigateRoot(Type pageType, object? parameter = null)
    {
        ContentFrame.Navigate(pageType, parameter);
        ContentFrame.BackStack.Clear();
        Navigation.IsBackEnabled = false;
    }

    public void NavigateTo(Type pageType, object? parameter = null) => ContentFrame.Navigate(pageType, parameter);

    public bool GoBack()
    {
        if (!CanNavigate || !ContentFrame.CanGoBack) return false;
        ContentFrame.GoBack();
        return true;
    }

    private void NavigateBack(NavigationView sender, NavigationViewBackRequestedEventArgs args) => GoBack();

    private void FrameNavigated(object sender, NavigationEventArgs e)
    {
        Navigation.IsBackEnabled = ContentFrame.CanGoBack;
    }

    public void ShowTransfers(bool receive)
    {
        var item = Navigation.MenuItems[receive ? 1 : 0];
        if (!ReferenceEquals(Navigation.SelectedItem, item)) Navigation.SelectedItem = item;
        else NavigateRoot(typeof(TransfersPage), receive);
    }

    public void ShowTransferDetails(TransferItem item) => NavigateTo(typeof(TransferDetailsPage), item);

    public void ShowDevices()
    {
        if (!ReferenceEquals(Navigation.SelectedItem, NavDevices)) Navigation.SelectedItem = NavDevices;
        else NavigateRoot(typeof(DevicesPage));
    }
    public async Task ShowDraftAsync(SavedDevice? receiver = null, IReadOnlyList<DraftSource>? sources = null)
    {
        if (Dialogs.HasActiveDialog || !Model.Ready) return;
        var dialog = new DraftPage(receiver);
        if (sources is not null) dialog.Select(sources);
        await ShowDialogAsync(dialog);
        if (dialog.Result is ShareResult share && Model.Outgoing.FirstOrDefault(t => t.Transfer.transferId == share.transferId) is { } item)
        {
            ShowTransfers(false);
            ShowTransferDetails(item);
            await new TransferDetailsPageActions(item).ShareAsync();
        }
        else if (dialog.Result is not null)
        {
            Navigation.SelectedItem = NavDevices;
            if (ContentFrame.CurrentSourcePageType != typeof(DevicesPage)) NavigateRoot(typeof(DevicesPage));
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
    public Task<ContentDialogResult> DialogAsync(
        string title,
        object content,
        string primary,
        string? secondary = null,
        DialogIntent intent = DialogIntent.Standard,
        DialogPriority priority = DialogPriority.Standard,
        Func<bool>? canShow = null) =>
        Dialogs.DecideAsync(title, content, primary, secondary, intent, priority, canShow);

    public Task<ContentDialogResult> ShowDialogAsync(ContentDialog dialog) => Dialogs.ShowAsync(dialog);

    private async void OnUpdated()
    {
        if (Model.Snapshot is { } snapshot) notifications.Update(snapshot, Model.Preferences.Notifications && !active);
        if (Model.Ready && !checkingRequests && !closing) await CheckRequestsAsync();
        await DrainInvitationsAsync();
    }
    private async Task CheckRequestsAsync()
    {
        checkingRequests = true;
        try
        {
            while (Model.Ready && !closing && Model.Snapshot is { } snapshot)
            {
                var pendingRequests = snapshot.Requests
                    .Where(candidate => candidate.status == "requested")
                    .ToArray();
                var request = pendingRequests.FirstOrDefault(candidate =>
                    !shownRequests.Contains("invitation:" + candidate.id));
                if (request is not null)
                {
                    var key = "invitation:" + request.id;
                    shownRequests.Add(key);
                    var result = await DialogAsync(
                        Strings.Get("approval_connection_request"),
                        Strings.Format(
                            "approval_request_body",
                            ("receiver", request.receiverName ?? request.receiverDeviceName ?? Strings.Get("approval_nearby_device")),
                            ("transferName", request.transferName)),
                        Strings.Get("button_approve"),
                        Strings.Get("button_refuse"),
                        priority: DialogPriority.Attention,
                        canShow: () => Model.Snapshot?.Requests.Any(candidate =>
                            candidate.id == request.id && candidate.status == "requested") == true);
                    if (!Model.Ready || closing || result == ContentDialogResult.None)
                    {
                        return;
                    }

                    var success = await Model.PerformAsync(() => Model.Session.RunAsync(core =>
                        core.RespondReceiverRequest(request.id, result == ContentDialogResult.Primary, null)));
                    if (!success)
                    {
                        shownRequests.Remove(key);
                        return;
                    }
                    continue;
                }
                var offer = snapshot.Offers.FirstOrDefault(candidate =>
                    !shownRequests.Contains("offer:" + candidate.transferId));
                if (offer is not null)
                {
                    var key = "offer:" + offer.transferId;
                    shownRequests.Add(key);
                    var sender = snapshot.Devices.FirstOrDefault(d => d.endpointId == offer.senderEndpointId);
                    var name = sender?.localLabel ?? sender?.remoteDisplayName ?? Strings.Get("saved_devices_unnamed");
                    var result = await DialogAsync(
                        Strings.Get("targeted_offer_title"),
                        PromptContent(
                            Strings.Format("targeted_offer_body", ("device", name), ("transferName", offer.transferName)),
                            Strings.Format("saved_devices_transfer_files", ("count", offer.fileCount), ("size", Strings.Size(offer.totalSize)))),
                        Strings.Get("button_receive"),
                        Strings.Get("button_refuse"),
                        priority: DialogPriority.Attention,
                        canShow: () => Model.Snapshot?.Offers.Any(candidate =>
                            candidate.transferId == offer.transferId) == true);
                    if (!Model.Ready || closing || result == ContentDialogResult.None)
                    {
                        return;
                    }

                    var success = await Model.PerformAsync(async () =>
                    {
                        var response = await Model.Session.RunAsync(c => c.RespondToTargetedOffer(offer.transferId, result == ContentDialogResult.Primary));
                        if (response is TargetedOfferResponse.Approved approved)
                            Model.StartTargetedReceive(approved.transferId, resume: false);
                    });
                    if (!success)
                    {
                        shownRequests.Remove(key);
                        return;
                    }
                    continue;
                }
                var pairing = PairingPromptPolicy.Next(
                    snapshot.Relationships,
                    snapshot.EligibleDevices,
                    shownRequests);
                if (pairing is null)
                {
                    return;
                }

                shownRequests.Add(pairing.Key);
                var deviceName = PairingDeviceName(snapshot, pairing);
                var pairingResult = pairing.Kind == PairingPromptKind.IncomingRequest
                    ? await DialogAsync(
                        Strings.Get("pairing_request_title"),
                        Strings.Format("pairing_request_body", ("device", deviceName)),
                        Strings.Get("pairing_accept"),
                        Strings.Get("pairing_decline"),
                        priority: DialogPriority.Attention,
                        canShow: () => PairingStillPending(Model.Snapshot, pairing))
                    : await DialogAsync(
                        Strings.Get("pairing_allow_title"),
                        PromptContent(deviceName, Strings.Get("pairing_allow_body"), emphasizePrimary: true),
                        Strings.Get("pairing_allow_confirm"),
                        Strings.Get("pairing_decline"),
                        priority: DialogPriority.Attention,
                        canShow: () => PairingStillPending(Model.Snapshot, pairing));
                if (!Model.Ready || closing || pairingResult == ContentDialogResult.None)
                {
                    return;
                }

                var applied = true;
                var pairingSuccess = await Model.PerformAsync(async () =>
                {
                    if (pairing.Kind == PairingPromptKind.IncomingRequest)
                    {
                        applied = await Model.Session.RunAsync(core => core.RespondToDevicePairing(
                            pairing.PeerEndpointId,
                            pairingResult == ContentDialogResult.Primary));
                    }
                    else if (pairingResult == ContentDialogResult.Primary)
                    {
                        applied = await Model.Session.RunAsync(core =>
                            core.RequestSavedDevicePairing(pairing.PeerEndpointId));
                    }
                    else
                    {
                        await Model.Session.RunAsync(core =>
                            core.DeclinePairingEligibility(pairing.PeerEndpointId));
                    }
                });
                if (!pairingSuccess || !applied)
                {
                    shownRequests.Remove(pairing.Key);
                    return;
                }
            }
        }
        finally { checkingRequests = false; }
    }

    private static string PairingDeviceName(CoreSnapshot snapshot, PairingPromptCandidate pairing)
    {
        var saved = snapshot.Devices.FirstOrDefault(device => device.endpointId == pairing.PeerEndpointId);
        var name = saved?.localLabel ?? saved?.remoteDisplayName ?? pairing.RemoteDisplayName;
        if (!string.IsNullOrWhiteSpace(name))
        {
            return name;
        }

        return pairing.PeerEndpointId.Length <= 12
            ? pairing.PeerEndpointId
            : pairing.PeerEndpointId[..8] + "…";
    }

    private static bool PairingStillPending(CoreSnapshot? snapshot, PairingPromptCandidate pairing) =>
        snapshot is not null && PairingPromptPolicy.IsPending(
            pairing,
            snapshot.Relationships,
            snapshot.EligibleDevices);

    private static StackPanel PromptContent(
        string primaryText,
        string secondaryText,
        bool emphasizePrimary = false)
    {
        var primary = new TextBlock
        {
            Text = primaryText,
            TextWrapping = TextWrapping.Wrap,
        };
        if (emphasizePrimary)
        {
            primary.FontWeight = Microsoft.UI.Text.FontWeights.SemiBold;
        }

        var secondary = new TextBlock
        {
            Text = secondaryText,
            TextWrapping = TextWrapping.Wrap,
            Style = (Style)Application.Current.Resources["VniDropSecondaryTextStyle"],
        };
        var content = new StackPanel { Spacing = 8, MaxWidth = 520 };
        content.Children.Add(primary);
        content.Children.Add(secondary);
        return content;
    }
    private async void ReviewRequests(object sender, RoutedEventArgs e)
    {
        ShowDevices();
        shownRequests.Clear();
        if (!checkingRequests) await CheckRequestsAsync();
    }
    private void ErrorClosed(InfoBar sender, InfoBarClosedEventArgs args) => Model.ClearError();
    private async void RetryStartup(object sender, RoutedEventArgs e) => await StartAsync();
    private async void ResetIdentity(object sender, RoutedEventArgs e)
    {
        if (await DialogAsync(Strings.Get("app_identity_reset_title"), Strings.Get("app_identity_reset_confirmation"), Strings.Get("app_identity_reset_action"), intent: DialogIntent.Destructive) == ContentDialogResult.Primary)
            await StartAsync(true);
    }
    private async void OnClosing(Microsoft.UI.Windowing.AppWindow sender, Microsoft.UI.Windowing.AppWindowClosingEventArgs args)
    {
        if (canClose) return;
        args.Cancel = true;
        if (Dialogs.HasActiveDialog) { Dialogs.HideActive(); return; }
        if (closing) return;
        closing = true;
        var preferenceWritesPaused = false;
        UpdateInteractionState();
        try
        {
            await Dialogs.WaitForIdleAsync();
            if (ContentFrame.Content is PreferencesPage preferences && !await preferences.FlushAsync()) return;
            Model.PausePreferenceWrites();
            preferenceWritesPaused = true;
            await Model.FlushPreferenceWritesAsync();
            await Model.WaitForMaintenanceAsync();
            if (Model.Ready)
            {
                var facts = await Model.Session.RunAsync(c => c.RuntimeObligationFacts());
                if (facts.activeInvitationTransfers + facts.activeTargetedTransfers + facts.invitationProviderAvailability + facts.targetedProviderAvailability + facts.targetedPreparations > 0 &&
                    await DialogAsync(Strings.Get("windows_close_title"), Strings.Get("windows_close_body"), Strings.Get("button_close"), intent: DialogIntent.Destructive) != ContentDialogResult.Primary) return;
            }
            refreshTimer.Stop(); NativeShare.Dispose(); await Model.Session.DisposeAsync();
            canClose = true; Close();
        }
        catch (Exception ex) { Model.Report(ex); }
        finally
        {
            if (!canClose && preferenceWritesPaused) Model.ResumePreferenceWrites();
            closing = false;
            if (!canClose) UpdateInteractionState();
        }
    }
}
