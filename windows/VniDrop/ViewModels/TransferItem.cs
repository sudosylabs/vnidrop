using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;
using Microsoft.UI.Xaml;
using VniDrop.Controls;

namespace VniDrop.ViewModels;

public sealed class TransferItem : ObservableModel
{
    private InvitationTransfer current;
    public StoredTransfer Transfer => current.Value;
    public InvitationDirection Direction => current.Direction;
    public string Name => Transfer.transferName ?? Strings.Get("receive_unknown_transfer");
    public ulong PreviewTransferId => Direction == InvitationDirection.Send ? Transfer.transferId : 0;
    private string previewPath = "";
    public string PreviewPath { get => previewPath; set => Set(ref previewPath, value); }
    public string Summary => Strings.FileSummary(Transfer.fileCount, Transfer.totalSize);
    public string FileCount => Transfer.fileCount.ToString("N0");
    public string TransferSize => Strings.Size(Transfer.totalSize);
    public string Access => Strings.Get(Transfer.accessMode == TransferAccessMode.ApprovalRequired ? "send_access_approval" : "send_access_anyone");
    public string CatalogDetail => Direction == InvitationDirection.Send ? Summary + " · " + Access : Summary;
    public InvitationStatus LifecycleStatus => current.Status;
    public string Status => Strings.Get(LifecycleStatus switch
    {
        InvitationStatus.Importing => "status_preparing",
        InvitationStatus.Sharing => "status_available",
        InvitationStatus.Receiving => "status_receiving",
        InvitationStatus.Done => "status_completed",
        InvitationStatus.Cancelled => "status_cancelled",
        InvitationStatus.Stopped => "status_stopped",
        InvitationStatus.Failed => "status_failed",
        InvitationStatus.Unknown => "progress_working",
        _ => throw new ArgumentOutOfRangeException(nameof(LifecycleStatus)),
    });
    public string StatusGlyph => LifecycleStatus switch
    {
        InvitationStatus.Sharing or InvitationStatus.Done => "\uE73E",
        InvitationStatus.Failed => "\uEA39",
        InvitationStatus.Cancelled or InvitationStatus.Stopped => "\uE711",
        InvitationStatus.Importing or InvitationStatus.Receiving or InvitationStatus.Unknown => "\uE895",
        _ => throw new ArgumentOutOfRangeException(nameof(LifecycleStatus)),
    };
    public StatusTone StatusTone => LifecycleStatus switch
    {
        InvitationStatus.Sharing => StatusTone.Accent,
        InvitationStatus.Done => StatusTone.Success,
        InvitationStatus.Failed => StatusTone.Critical,
        InvitationStatus.Cancelled or InvitationStatus.Stopped => StatusTone.Neutral,
        InvitationStatus.Importing or InvitationStatus.Receiving or InvitationStatus.Unknown => StatusTone.Warning,
        _ => throw new ArgumentOutOfRangeException(nameof(LifecycleStatus)),
    };
    public string Date => DateTimeOffset.FromUnixTimeMilliseconds(Transfer.createdAt).ToLocalTime().ToString("g");
    public string AutomationName => Name + ", " + Summary + ", " + Status;
    public bool CanShare => Direction == InvitationDirection.Send && LifecycleStatus == InvitationStatus.Sharing && Transfer.ticket is not null;
    public bool CanStop => LifecycleStatus is InvitationStatus.Sharing or InvitationStatus.Importing or InvitationStatus.Receiving;
    public bool CanDelete => Direction == InvitationDirection.Send || !CanStop;
    public Visibility ShareVisibility => CanShare ? Visibility.Visible : Visibility.Collapsed;
    public Visibility StopVisibility => CanStop ? Visibility.Visible : Visibility.Collapsed;
    public bool IsReceiving => Direction == InvitationDirection.Receive && LifecycleStatus == InvitationStatus.Receiving;
    public bool ShowProgress { get; private set; }
    public double Progress { get; private set; }
    public bool IsIndeterminate { get; private set; } = true;
    public string ProgressText { get; private set; } = "";
    public string ReceiverSummary { get; private set; } = "";
    public int ActivityCount { get; private set; }
    public int ReceiverCount { get; private set; }
    public Visibility ActivityCountVisibility => ActivityCount > 0 ? Visibility.Visible : Visibility.Collapsed;
    public Visibility ReceiverCountVisibility => ReceiverCount > 0 ? Visibility.Visible : Visibility.Collapsed;
    public TransferItem(InvitationTransfer transfer)
    {
        current = transfer;
    }
    public void Update(InvitationTransfer updated, IReadOnlyList<CoreEvent> events, IReadOnlyList<InvitationReceiver> requests)
    {
        current = updated;
        var transfer = Transfer;
        var receivers = requests.Where(r => r.Value.transferId == transfer.transferId).ToArray();
        var pending = receivers.Count(r => r.Status is ReceiverStatus.Requested or ReceiverStatus.Accepted);
        var completed = receivers.Count(r => r.Status == ReceiverStatus.Completed);
        ReceiverCount = pending + completed;
        ActivityCount = events.Count(e => e.transferId == transfer.transferId && TransferPresentation.ActivityKey(e) is not null);
        ReceiverSummary = pending + completed == 0 ? Strings.Get("transfer_receivers_description") :
            string.Join(" · ", new[] { pending > 0 ? Strings.Format("transfer_receivers_pending", ("count", pending)) : null, completed > 0 ? Strings.Format("transfer_receivers_completed_count", ("count", completed)) : null }.Where(s => s is not null));
        var progress = LifecycleStatus == InvitationStatus.Sharing
            ? receivers.Where(r => r.Status == ReceiverStatus.Accepted).Select(r => TransferPresentation.ReceiverProgress(events, transfer.transferId, r.Value.remoteEndpointId, transfer.totalSize)).FirstOrDefault(p => p?.LabelKey == "progress_sending")
            : TransferPresentation.Progress(events, transfer.transferId, transfer.direction, transfer.totalSize);
        ShowProgress = CanStop && (progress is not null || LifecycleStatus is InvitationStatus.Receiving or InvitationStatus.Importing);
        IsIndeterminate = progress?.Fraction is null;
        Progress = (progress?.Fraction ?? 0) * 100;
        ProgressText = Strings.Get(progress?.LabelKey ?? (LifecycleStatus == InvitationStatus.Importing ? "progress_preparing" : "progress_connecting"));
        if (progress?.Fraction is { } fraction) ProgressText += " · " + fraction.ToString("P0");
        if (progress?.FileName is { } name) ProgressText += " · " + name;
        Changed(null);
    }
}
