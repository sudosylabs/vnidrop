using VniDrop.Core;
using VniDrop.Native;
using VniDrop.Platform;
using Microsoft.UI.Xaml;
using VniDrop.Controls;

namespace VniDrop.ViewModels;

public sealed class TransferItem : ObservableModel
{
    public StoredTransfer Transfer { get; private set; }
    public string Name => Transfer.transferName ?? Strings.Get("receive_unknown_transfer");
    public ulong PreviewTransferId => Transfer.direction == "send" ? Transfer.transferId : 0;
    private string previewPath = "";
    public string PreviewPath { get => previewPath; set => Set(ref previewPath, value); }
    public string Summary => Strings.FileSummary(Transfer.fileCount, Transfer.totalSize);
    public string FileCount => Transfer.fileCount.ToString("N0");
    public string TransferSize => Strings.Size(Transfer.totalSize);
    public string Access => Strings.Get(Transfer.accessMode == TransferAccessMode.ApprovalRequired ? "send_access_approval" : "send_access_anyone");
    public string CatalogDetail => Transfer.direction == "send" ? Summary + " · " + Access : Summary;
    public InvitationStatus LifecycleStatus { get; private set; }
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
    public bool CanShare => Transfer.direction == "send" && Transfer.status == "sharing" && Transfer.ticket is not null;
    public bool CanStop => Transfer.status is "sharing" or "importing" or "receiving";
    public bool CanDelete => Transfer.direction == "send" || !CanStop;
    public Visibility ShareVisibility => CanShare ? Visibility.Visible : Visibility.Collapsed;
    public Visibility StopVisibility => CanStop ? Visibility.Visible : Visibility.Collapsed;
    public bool IsReceiving => Transfer.direction == "receive" && Transfer.status == "receiving";
    public bool ShowProgress { get; private set; }
    public double Progress { get; private set; }
    public bool IsIndeterminate { get; private set; } = true;
    public string ProgressText { get; private set; } = "";
    public string ReceiverSummary { get; private set; } = "";
    public int ActivityCount { get; private set; }
    public int ReceiverCount { get; private set; }
    public Visibility ActivityCountVisibility => ActivityCount > 0 ? Visibility.Visible : Visibility.Collapsed;
    public Visibility ReceiverCountVisibility => ReceiverCount > 0 ? Visibility.Visible : Visibility.Collapsed;
    public TransferItem(StoredTransfer transfer)
    {
        Transfer = transfer;
        LifecycleStatus = InvitationStatusCodec.Parse(transfer.status);
    }
    public void Update(StoredTransfer transfer, IReadOnlyList<CoreEvent> events, IReadOnlyList<ReceiverRequest> requests)
    {
        Transfer = transfer;
        LifecycleStatus = InvitationStatusCodec.Parse(transfer.status);
        var receivers = requests.Where(r => r.transferId == transfer.transferId).ToArray();
        var pending = receivers.Count(r => r.status is "requested" or "accepted");
        var completed = receivers.Count(r => r.status == "completed");
        ReceiverCount = pending + completed;
        ActivityCount = events.Count(e => e.transferId == transfer.transferId && TransferPresentation.ActivityKey(e) is not null);
        ReceiverSummary = pending + completed == 0 ? Strings.Get("transfer_receivers_description") :
            string.Join(" · ", new[] { pending > 0 ? Strings.Format("transfer_receivers_pending", ("count", pending)) : null, completed > 0 ? Strings.Format("transfer_receivers_completed_count", ("count", completed)) : null }.Where(s => s is not null));
        var progress = transfer.status == "sharing"
            ? receivers.Where(r => r.status == "accepted").Select(r => TransferPresentation.ReceiverProgress(events, transfer.transferId, r.remoteEndpointId, transfer.totalSize)).FirstOrDefault(p => p?.LabelKey == "progress_sending")
            : TransferPresentation.Progress(events, transfer.transferId, transfer.direction, transfer.totalSize);
        ShowProgress = CanStop && (progress is not null || transfer.status is "receiving" or "importing");
        IsIndeterminate = progress?.Fraction is null;
        Progress = (progress?.Fraction ?? 0) * 100;
        ProgressText = Strings.Get(progress?.LabelKey ?? (transfer.status == "importing" ? "progress_preparing" : "progress_connecting"));
        if (progress?.Fraction is { } fraction) ProgressText += " · " + fraction.ToString("P0");
        if (progress?.FileName is { } name) ProgressText += " · " + name;
        Changed(null);
    }
}
