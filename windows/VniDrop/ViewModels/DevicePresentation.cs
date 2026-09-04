using System.Globalization;
using Microsoft.UI.Xaml;
using VniDrop.Core;
using VniDrop.Controls;
using VniDrop.Native;
using VniDrop.Platform;

namespace VniDrop.ViewModels;

public enum DevicePageAction
{
    None,
    AcceptPairing,
    DeclinePairing,
    RequestPairing,
    DeclineEligibility,
    Unblock,
}

public enum DeviceTransferAction
{
    None,
    Receive,
    Resume,
    Cancel,
    Delete,
}

public sealed class SavedDeviceItem
{
    public SavedDeviceItem(SavedDevice device, int transferCount, bool isBusy)
    {
        Device = device;
        Name = !string.IsNullOrWhiteSpace(device.localLabel)
            ? device.localLabel!
            : !string.IsNullOrWhiteSpace(device.remoteDisplayName)
                ? device.remoteDisplayName
                : Strings.Get("saved_devices_unnamed");
        Detail = !string.IsNullOrWhiteSpace(device.localLabel)
            && !string.Equals(device.localLabel, device.remoteDisplayName, StringComparison.Ordinal)
                ? Strings.Format("saved_devices_authenticated_name", ("name", device.remoteDisplayName))
                : Strings.Format("saved_devices_endpoint", ("deviceId", device.endpointId));
        TransferCount = transferCount.ToString(CultureInfo.CurrentCulture);
        IsBusy = isBusy;
    }

    public SavedDevice Device { get; }

    public string Id => Device.endpointId;

    public string Name { get; }

    public string Detail { get; }

    public string TransferCount { get; }

    public bool IsBusy { get; }

    public bool IsEnabled => !IsBusy;

    public Visibility BusyVisibility => IsBusy ? Visibility.Visible : Visibility.Collapsed;

    public Visibility CountVisibility => TransferCount == "0" ? Visibility.Collapsed : Visibility.Visible;

    public string AutomationName => $"{Name}. {Detail}";
}

public sealed class DeviceActionRowItem
{
    private DeviceActionRowItem(
        string key,
        string endpointId,
        string name,
        string detail,
        string glyph,
        DevicePageAction primaryAction,
        string primaryText,
        DevicePageAction secondaryAction,
        string secondaryText,
        bool isBusy)
    {
        Key = key;
        EndpointId = endpointId;
        Name = name;
        Detail = detail;
        Glyph = glyph;
        PrimaryAction = primaryAction;
        PrimaryText = primaryText;
        SecondaryAction = secondaryAction;
        SecondaryText = secondaryText;
        IsBusy = isBusy;
    }

    public string Key { get; }

    public string EndpointId { get; }

    public string Name { get; }

    public string Detail { get; }

    public string Glyph { get; }

    public DevicePageAction PrimaryAction { get; }

    public string PrimaryText { get; }

    public DevicePageAction SecondaryAction { get; }

    public string SecondaryText { get; }

    public bool IsBusy { get; }

    public bool IsEnabled => !IsBusy;

    public Visibility BusyVisibility => IsBusy ? Visibility.Visible : Visibility.Collapsed;

    public Visibility PrimaryVisibility => PrimaryAction == DevicePageAction.None
        ? Visibility.Collapsed
        : Visibility.Visible;

    public Visibility SecondaryVisibility => SecondaryAction == DevicePageAction.None
        ? Visibility.Collapsed
        : Visibility.Visible;

    public string AutomationName => $"{Name}. {Detail}";

    public static DeviceActionRowItem Pending(DeviceRelationship relationship, string name, bool isBusy)
    {
        var incoming = relationship.state == DeviceRelationshipState.PendingIncoming;
        return new DeviceActionRowItem(
            $"relationship:{relationship.remoteEndpointId}",
            relationship.remoteEndpointId,
            name,
            incoming
                ? Strings.Get("saved_devices_pending_incoming")
                : Strings.Get("saved_devices_pending_outgoing"),
            "\uE8FA",
            incoming ? DevicePageAction.AcceptPairing : DevicePageAction.None,
            incoming ? Strings.Get("saved_devices_accept_pairing_action") : string.Empty,
            incoming ? DevicePageAction.DeclinePairing : DevicePageAction.None,
            incoming ? Strings.Get("saved_devices_decline_action") : string.Empty,
            isBusy);
    }

    public static DeviceActionRowItem Eligible(PairingEligibilitySummary eligible, string name, bool isBusy)
    {
        return new DeviceActionRowItem(
            $"eligibility:{eligible.peerEndpointId}",
            eligible.peerEndpointId,
            name,
            Strings.Format("saved_devices_endpoint", ("deviceId", eligible.peerEndpointId)),
            "\uE8FB",
            DevicePageAction.RequestPairing,
            Strings.Get("saved_devices_remember_action"),
            DevicePageAction.DeclineEligibility,
            Strings.Get("saved_devices_decline_action"),
            isBusy);
    }

    public static DeviceActionRowItem Blocked(string endpointId, bool isBusy)
    {
        return new DeviceActionRowItem(
            $"blocked:{endpointId}",
            endpointId,
            endpointId,
            Strings.Format("saved_devices_endpoint", ("deviceId", endpointId)),
            "\uE733",
            DevicePageAction.Unblock,
            Strings.Get("windows_unblock"),
            DevicePageAction.None,
            string.Empty,
            isBusy);
    }
}

public sealed class DeviceTransferItem
{
    public DeviceTransferItem(
        TargetedTransfer transfer,
        string peerName,
        bool mutationBusy,
        bool receiveRunning)
    {
        Transfer = transfer;
        PeerName = peerName;
        var availability = TargetedTransferActionPolicy.Evaluate(
            transfer.role,
            transfer.state,
            receiveRunning,
            mutationBusy);
        IsBusy = availability.ShowBusy;
        IsPrimaryEnabled = availability.CanStart;
        IsSecondaryEnabled = IsTerminal(transfer.state)
            ? availability.CanDelete
            : availability.CanCancel;
        IsMoreEnabled = availability.CanOpenActions;

        var incoming = transfer.role == TargetedTransferRole.Receiver;
        Direction = Strings.Format(
            incoming ? "saved_devices_transfer_direction_incoming" : "saved_devices_transfer_direction_outgoing",
            ("device", peerName));
        Summary = $"{Strings.Format("saved_devices_transfer_files", ("count", transfer.fileCount), ("size", Strings.Size(transfer.totalSize)))} · {Direction}";
        Status = Strings.Get(StatusKey(transfer.state));
        StatusTone = Tone(transfer.state);
        StatusGlyph = Glyph(transfer.state, incoming);
        Progress = transfer.totalSize == 0
            ? 0
            : Math.Clamp(transfer.verifiedBytes * 100d / transfer.totalSize, 0d, 100d);
        ProgressText = Strings.Format(
            "saved_devices_transfer_progress",
            ("verified", Strings.Size(transfer.verifiedBytes)),
            ("total", Strings.Size(transfer.totalSize)));

        if (incoming && transfer.state == TargetedTransferState.Approved)
        {
            PrimaryAction = DeviceTransferAction.Receive;
            PrimaryText = Strings.Get("saved_devices_transfer_receive");
        }
        else if (incoming && transfer.state == TargetedTransferState.Interrupted)
        {
            PrimaryAction = DeviceTransferAction.Resume;
            PrimaryText = Strings.Get("saved_devices_transfer_resume");
        }

        SecondaryAction = IsTerminal(transfer.state)
            ? DeviceTransferAction.Delete
            : DeviceTransferAction.Cancel;
        SecondaryText = Strings.Get(SecondaryAction == DeviceTransferAction.Delete
            ? "saved_devices_transfer_delete"
            : "saved_devices_transfer_cancel");
    }

    public TargetedTransfer Transfer { get; }

    public string Id => Transfer.id;

    public string PeerId => Transfer.role == TargetedTransferRole.Sender
        ? Transfer.receiverEndpointId
        : Transfer.senderEndpointId;

    public string PeerName { get; }

    public string Title => Transfer.transferName;

    public string Direction { get; }

    public string Summary { get; }

    public string Status { get; }

    public StatusTone StatusTone { get; }

    public string StatusGlyph { get; }

    public double Progress { get; }

    public string ProgressText { get; }

    public DeviceTransferAction PrimaryAction { get; } = DeviceTransferAction.None;

    public string PrimaryText { get; } = string.Empty;

    public DeviceTransferAction SecondaryAction { get; }

    public string SecondaryText { get; }

    public bool IsBusy { get; }

    public bool IsPrimaryEnabled { get; }

    public bool IsSecondaryEnabled { get; }

    public bool IsMoreEnabled { get; }

    public Visibility BusyVisibility => IsBusy ? Visibility.Visible : Visibility.Collapsed;

    public Visibility PrimaryVisibility => PrimaryAction == DeviceTransferAction.None
        ? Visibility.Collapsed
        : Visibility.Visible;

    public Visibility ProgressVisibility => Transfer.totalSize > 0
        && (Transfer.verifiedBytes > 0
            || Transfer.state is TargetedTransferState.Transferring or TargetedTransferState.Interrupted)
            ? Visibility.Visible
            : Visibility.Collapsed;

    public string AutomationName => $"{Title}. {Summary}. {Status}";

    private static bool IsTerminal(TargetedTransferState state)
    {
        return state is TargetedTransferState.Completed
            or TargetedTransferState.Declined
            or TargetedTransferState.Cancelled
            or TargetedTransferState.Failed
            or TargetedTransferState.Deleted;
    }

    private static string StatusKey(TargetedTransferState state)
    {
        return state switch
        {
            TargetedTransferState.Preparing => "status_preparing",
            TargetedTransferState.Offering => "status_offering",
            TargetedTransferState.AwaitingApproval => "status_awaiting_approval",
            TargetedTransferState.Approved => "status_approved",
            TargetedTransferState.Connecting => "status_connecting",
            TargetedTransferState.Transferring => "status_transferring",
            TargetedTransferState.Interrupted => "status_interrupted",
            TargetedTransferState.Completed => "status_completed",
            TargetedTransferState.Declined => "status_declined",
            TargetedTransferState.Cancelled or TargetedTransferState.Deleted => "status_cancelled",
            TargetedTransferState.Failed => "status_failed",
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, null),
        };
    }

    private static StatusTone Tone(TargetedTransferState state)
    {
        return state switch
        {
            TargetedTransferState.Completed => StatusTone.Success,
            TargetedTransferState.Failed => StatusTone.Critical,
            TargetedTransferState.Interrupted or TargetedTransferState.AwaitingApproval => StatusTone.Warning,
            TargetedTransferState.Offering
                or TargetedTransferState.Approved
                or TargetedTransferState.Connecting
                or TargetedTransferState.Transferring => StatusTone.Accent,
            _ => StatusTone.Neutral,
        };
    }

    private static string Glyph(TargetedTransferState state, bool incoming)
    {
        return state switch
        {
            TargetedTransferState.Preparing or TargetedTransferState.AwaitingApproval => "\uE823",
            TargetedTransferState.Offering => "\uE724",
            TargetedTransferState.Approved or TargetedTransferState.Completed => "\uE73E",
            TargetedTransferState.Connecting => "\uE968",
            TargetedTransferState.Transferring => incoming ? "\uE896" : "\uE898",
            TargetedTransferState.Interrupted => "\uE7BA",
            TargetedTransferState.Failed => "\uEA39",
            TargetedTransferState.Declined or TargetedTransferState.Cancelled or TargetedTransferState.Deleted => "\uE711",
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, null),
        };
    }
}
