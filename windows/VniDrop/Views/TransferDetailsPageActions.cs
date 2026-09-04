using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Core;
using VniDrop.Platform;
using VniDrop.Services;
using VniDrop.ViewModels;

namespace VniDrop.Views;

public sealed class TransferDetailsPageActions(TransferItem item)
{
    private static TextBlock Text(string text, bool secondary = false)
    {
        var block = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        if (secondary) block.Style = (Style)Application.Current.Resources["VniDropSecondaryTextStyle"];
        return block;
    }
    private static StackPanel Panel(params UIElement[] children)
    {
        var panel = new StackPanel { Spacing = 14, MaxWidth = 520 };
        foreach (var child in children) panel.Children.Add(child);
        return panel;
    }
    private async Task ShowAsync(string title, UIElement content) =>
        await App.Window.ShowDialogAsync(new ContentDialog
        {
            Title = title,
            Content = content,
            CloseButtonText = Strings.Get("button_close"),
            CloseButtonStyle = (Style)Application.Current.Resources["VniDropDialogButtonStyle"],
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = (CornerRadius)Application.Current.Resources["VniDropCardCornerRadius"],
        });

    public async Task ShareAsync()
    {
        var panel = Panel();
        if (item.Transfer.ticket is not { } ticket)
        {
            panel.Children.Add(Text(Strings.Get("transfer_event_preparing"), true));
            await ShowAsync(Strings.Get("transfer_share_title"), panel); return;
        }
        await App.Window.ShowDialogAsync(new ShareTransferDialog(ticket, item.Name));
    }

    public async Task ActivityAsync()
    {
        var events = App.Window.Model.Events.Where(e => e.transferId == item.Transfer.transferId)
            .Select(e => (Event: e, Key: TransferPresentation.ActivityKey(e))).Where(e => e.Key is not null).OrderByDescending(e => e.Event.timestamp).ToArray();
        var panel = Panel();
        if (events.Length == 0) panel.Children.Add(Text(Strings.Get("transfer_no_activity"), true));
        foreach (var (entry, key) in events)
        {
            var row = new Grid { ColumnSpacing = 12, Padding = new(0, 8, 0, 8) };
            row.ColumnDefinitions.Add(new() { Width = new(20) }); row.ColumnDefinitions.Add(new() { Width = new(1, GridUnitType.Star) });
            row.Children.Add(new FontIcon { Glyph = "\uE916", FontSize = 14, Opacity = 0.72 });
            var content = new StackPanel { Spacing = 3 }; content.Children.Add(Text(Strings.Get(key!)));
            content.Children.Add(Text(DateTimeOffset.FromUnixTimeMilliseconds(entry.timestamp).ToLocalTime().ToString("g"), true)); Grid.SetColumn(content, 1); row.Children.Add(content); panel.Children.Add(row);
        }
        await ShowAsync(Strings.Get("transfer_activity_title"), new ScrollViewer { Content = panel, MaxHeight = 560 });
    }

    public async Task ReceiversAsync()
    {
        var receivers = App.Window.Model.Snapshot?.Requests.Where(r => r.transferId == item.Transfer.transferId).OrderByDescending(r => r.requestedAt).ToArray() ?? [];
        var panel = Panel();
        if (receivers.Length == 0) panel.Children.Add(Text(Strings.Get("transfer_no_receivers"), true));
        foreach (var receiver in receivers)
        {
            var name = receiver.receiverName ?? receiver.receiverDeviceName ?? Strings.Get("transfer_nearby_device");
            var block = new StackPanel { Spacing = 5, Padding = new(0, 9, 0, 9) };
            block.Children.Add(new TextBlock { Text = name, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis });
            if (receiver.receiverDeviceName is { } device && device != name) block.Children.Add(Text(device, true));
            var progress = receiver.status is "requested" or "accepted" ? TransferPresentation.ReceiverProgress(App.Window.Model.Events, item.Transfer.transferId, receiver.remoteEndpointId, item.Transfer.totalSize) : null;
            if (progress is not null)
            {
                block.Children.Add(new ProgressBar { IsIndeterminate = progress.Fraction is null, Value = (progress.Fraction ?? 0) * 100 });
                block.Children.Add(Text(Strings.Get(progress.LabelKey) + (progress.Fraction is { } p ? " · " + p.ToString("P0") : ""), true));
            }
            else block.Children.Add(Text(Strings.Get(receiver.status switch { "requested" => "transfer_receiver_requested", "accepted" => "transfer_receiver_accepted", "completed" => "transfer_receiver_completed", "refused" => "transfer_receiver_refused", "expired" => "transfer_receiver_expired", "failed" => "transfer_receiver_failed", _ => "transfer_receiver_unknown" }), true));
            if (!string.IsNullOrWhiteSpace(receiver.reason)) block.Children.Add(Text(receiver.reason!, true));
            panel.Children.Add(block);
        }
        await ShowAsync(Strings.Get("transfer_receivers_title"), new ScrollViewer { Content = panel, MaxHeight = 560 });
    }

    public async Task StopAsync()
    {
        if (!item.CanStop) return;
        await App.Window.Model.PerformAsync(() => App.Window.Model.Session.RunAsync(c => c.CancelTransfer(item.Transfer.transferId)));
    }
    public async Task<bool> DeleteAsync()
    {
        if (!item.CanDelete) return false;
        var body = item.Transfer.direction == "receive" ? Strings.Format("receive_delete_history_description", ("transferName", item.Name)) : Strings.Format("transfer_delete_description", ("transferName", item.Name));
        if (await App.Window.DialogAsync(Strings.Get("transfer_delete_title"), body, Strings.Get("button_delete_transfer"), intent: DialogIntent.Destructive) != ContentDialogResult.Primary) return false;
        return await App.Window.Model.PerformAsync(() => App.Window.Model.Session.RunAsync(c => c.DeleteTransfer(item.Transfer.transferId)));
    }
}
