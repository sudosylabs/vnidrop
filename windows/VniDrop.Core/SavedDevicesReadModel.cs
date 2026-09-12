using VniDrop.Native;

namespace VniDrop.Core;

public enum SavedDeviceTransferDirection
{
    Outgoing,
    Incoming,
}

public enum SavedDeviceTransferAction
{
    Receive,
    Resume,
    Cancel,
    Delete,
}

public enum SavedDeviceNotificationKind
{
    PairingRequest,
    TargetedOffer,
    TargetedReceiveCompleted,
    TargetedReceiveFailed,
    TargetedSendCompleted,
    TargetedSendFailed,
}

public sealed record SavedDevicesReadInputs(
    IReadOnlyList<PairingEligibilitySummary> Eligibilities,
    IReadOnlyList<DeviceRelationship> Relationships,
    IReadOnlyList<SavedDevice> SavedDevices,
    IReadOnlyList<PendingTargetedOffer> PendingOffers,
    IReadOnlyList<TargetedTransfer> TargetedTransfers)
{
    public static SavedDevicesReadInputs FromSnapshot(CoreSnapshot snapshot) => new(
        snapshot.EligibleDevices,
        snapshot.Relationships,
        snapshot.Devices,
        snapshot.Offers,
        snapshot.TargetedTransfers);
}

public sealed record SavedDeviceTransferFact(
    string Id,
    TargetedTransferRole Role,
    string PeerEndpointId,
    string? PeerDisplayName,
    SavedDeviceTransferDirection Direction,
    string TransferName,
    ulong FileCount,
    ulong TotalSize,
    ulong VerifiedBytes,
    TargetedTransferState State,
    long CreatedAt,
    long UpdatedAt,
    IReadOnlyList<SavedDeviceTransferAction> AvailableActions,
    double? ProgressFraction)
{
    public TargetedTransferActionAvailability EvaluateAvailability(bool receiveRunning, bool mutationBusy) =>
        TargetedTransferActionPolicy.Evaluate(Role, State, receiveRunning, mutationBusy);
}

public sealed record SavedDeviceNotificationFact(
    string Id,
    string State,
    SavedDeviceNotificationKind? Kind,
    string? TransferName,
    string? DeviceName,
    bool Pending);

public sealed record SavedDevicesReadSnapshot(
    IReadOnlyList<PairingEligibilitySummary> Eligibilities,
    IReadOnlyList<DeviceRelationship> PendingRelationships,
    IReadOnlyList<SavedDevice> SavedDevices,
    IReadOnlyList<PendingTargetedOffer> PendingOffers,
    IReadOnlyList<SavedDeviceTransferFact> TargetedTransfers,
    IReadOnlyDictionary<string, string> DisplayNames,
    PairingPromptCandidate? NextPairingPrompt,
    IReadOnlyList<SavedDeviceNotificationFact> Notifications);

public static class SavedDevicesReadModel
{
    public static SavedDevicesReadSnapshot Derive(
        SavedDevicesReadInputs inputs,
        IReadOnlySet<string>? dismissedKeys = null)
    {
        var displayNames = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var device in inputs.SavedDevices)
        {
            if (DisplayName(device) is { } name)
            {
                displayNames[device.endpointId] = name;
            }
        }

        var pendingRelationships = inputs.Relationships
            .Where(relationship => relationship.state is DeviceRelationshipState.PendingIncoming
                or DeviceRelationshipState.PendingOutgoing)
            .OrderByDescending(relationship => relationship.updatedAt)
            .ToArray();
        var eligibilities = inputs.Eligibilities
            .OrderByDescending(eligibility => eligibility.createdAt)
            .ToArray();
        var savedDevices = inputs.SavedDevices
            .OrderByDescending(device => device.createdAt)
            .ToArray();
        var pendingOffers = inputs.PendingOffers
            .OrderBy(offer => offer.receivedAt)
            .ToArray();
        var targetedTransfers = inputs.TargetedTransfers
            .Where(transfer => transfer.state != TargetedTransferState.Deleted)
            .OrderByDescending(transfer => transfer.updatedAt)
            .Select(transfer => TransferItem(transfer, displayNames.GetValueOrDefault(PeerEndpointId(transfer))))
            .ToArray();

        return new SavedDevicesReadSnapshot(
            eligibilities,
            pendingRelationships,
            savedDevices,
            pendingOffers,
            targetedTransfers,
            displayNames,
            PairingPromptPolicy.Next(inputs.Relationships, inputs.Eligibilities, dismissedKeys ?? new HashSet<string>()),
            Notifications(pendingRelationships, pendingOffers, targetedTransfers, displayNames));
    }

    public static string? DisplayName(SavedDevice? device)
    {
        if (!string.IsNullOrWhiteSpace(device?.localLabel))
        {
            return device!.localLabel;
        }

        return string.IsNullOrWhiteSpace(device?.remoteDisplayName) ? null : device!.remoteDisplayName;
    }

    public static string PeerEndpointId(TargetedTransfer transfer) =>
        transfer.role == TargetedTransferRole.Sender
            ? transfer.receiverEndpointId
            : transfer.senderEndpointId;

    public static SavedDeviceTransferFact TransferItem(TargetedTransfer transfer, string? peerDisplayName)
    {
        var outgoing = transfer.role == TargetedTransferRole.Sender;
        return new SavedDeviceTransferFact(
            transfer.id,
            transfer.role,
            PeerEndpointId(transfer),
            peerDisplayName,
            outgoing ? SavedDeviceTransferDirection.Outgoing : SavedDeviceTransferDirection.Incoming,
            transfer.transferName,
            transfer.fileCount,
            transfer.totalSize,
            transfer.verifiedBytes,
            transfer.state,
            transfer.createdAt,
            transfer.updatedAt,
            Actions(transfer.role, transfer.state),
            ProgressFraction(transfer.role, transfer.state, transfer.verifiedBytes, transfer.totalSize));
    }

    // Direction owns wording: sender Completed/Failed is a peer download, not a local receive.
    public static SavedDeviceNotificationKind? OutcomeKind(
        SavedDeviceTransferDirection direction,
        TargetedTransferState state) => (direction, state) switch
    {
        (SavedDeviceTransferDirection.Incoming, TargetedTransferState.Completed) =>
            SavedDeviceNotificationKind.TargetedReceiveCompleted,
        (SavedDeviceTransferDirection.Incoming, TargetedTransferState.Failed) =>
            SavedDeviceNotificationKind.TargetedReceiveFailed,
        (SavedDeviceTransferDirection.Outgoing, TargetedTransferState.Completed) =>
            SavedDeviceNotificationKind.TargetedSendCompleted,
        (SavedDeviceTransferDirection.Outgoing, TargetedTransferState.Failed) =>
            SavedDeviceNotificationKind.TargetedSendFailed,
        _ => null,
    };

    public static string TitleKey(SavedDeviceNotificationKind kind) => kind switch
    {
        SavedDeviceNotificationKind.PairingRequest => "saved_devices_pending_incoming",
        SavedDeviceNotificationKind.TargetedOffer => "receive_review_title",
        SavedDeviceNotificationKind.TargetedReceiveCompleted => "notifications_receive_completed_title",
        SavedDeviceNotificationKind.TargetedReceiveFailed => "notifications_receive_failed_title",
        SavedDeviceNotificationKind.TargetedSendCompleted => "notifications_receiver_completed_title",
        SavedDeviceNotificationKind.TargetedSendFailed => "notifications_receiver_failed_title",
        _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, null),
    };

    public static string? TitleKey(SavedDeviceNotificationKind? kind) =>
        kind is { } value ? TitleKey(value) : null;

    public static string BodyKey(SavedDeviceNotificationKind kind) => kind switch
    {
        SavedDeviceNotificationKind.PairingRequest => "pairing_request_body",
        SavedDeviceNotificationKind.TargetedOffer => "targeted_offer_body",
        SavedDeviceNotificationKind.TargetedReceiveCompleted => "notifications_receive_completed_body",
        SavedDeviceNotificationKind.TargetedReceiveFailed => "notifications_receive_failed_body",
        SavedDeviceNotificationKind.TargetedSendCompleted => "notifications_receiver_completed_body",
        SavedDeviceNotificationKind.TargetedSendFailed => "notifications_receiver_failed_body",
        _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, null),
    };

    public static string? BodyKey(SavedDeviceNotificationKind? kind) =>
        kind is { } value ? BodyKey(value) : null;

    public static bool RememberNotice(bool pending, bool shown) => shown || !pending;

    private static SavedDeviceTransferAction[] Actions(TargetedTransferRole role, TargetedTransferState state)
    {
        if (state == TargetedTransferState.Deleted)
        {
            return [];
        }

        var availability = TargetedTransferActionPolicy.Evaluate(role, state, receiveRunning: false, mutationBusy: false);
        var actions = new List<SavedDeviceTransferAction>(3);
        if (availability.CanStart)
        {
            actions.Add(state == TargetedTransferState.Interrupted
                ? SavedDeviceTransferAction.Resume
                : SavedDeviceTransferAction.Receive);
        }

        if (availability.CanCancel)
        {
            actions.Add(SavedDeviceTransferAction.Cancel);
        }

        if (availability.CanDelete)
        {
            actions.Add(SavedDeviceTransferAction.Delete);
        }

        return [.. actions];
    }

    private static double? ProgressFraction(
        TargetedTransferRole role,
        TargetedTransferState state,
        ulong verifiedBytes,
        ulong totalSize)
    {
        if (role != TargetedTransferRole.Receiver
            || totalSize == 0
            || !TransferPresentation.ShowsTargetedProgress(state, totalSize))
        {
            return null;
        }

        return Math.Clamp(verifiedBytes / (double)totalSize, 0d, 1d);
    }

    private static SavedDeviceNotificationFact[] Notifications(
        IReadOnlyList<DeviceRelationship> pendingRelationships,
        IReadOnlyList<PendingTargetedOffer> pendingOffers,
        IReadOnlyList<SavedDeviceTransferFact> targetedTransfers,
        IReadOnlyDictionary<string, string> displayNames)
    {
        var notices = new List<SavedDeviceNotificationFact>(
            pendingOffers.Count + pendingRelationships.Count + targetedTransfers.Count);
        foreach (var offer in pendingOffers)
        {
            notices.Add(new SavedDeviceNotificationFact(
                "offer:" + offer.transferId,
                "pending",
                SavedDeviceNotificationKind.TargetedOffer,
                offer.transferName,
                displayNames.GetValueOrDefault(offer.senderEndpointId),
                Pending: true));
        }

        foreach (var relationship in pendingRelationships)
        {
            notices.Add(new SavedDeviceNotificationFact(
                "pairing:" + relationship.remoteEndpointId,
                relationship.state.ToString(),
                relationship.state == DeviceRelationshipState.PendingIncoming
                    ? SavedDeviceNotificationKind.PairingRequest
                    : null,
                null,
                displayNames.GetValueOrDefault(relationship.remoteEndpointId),
                Pending: relationship.state == DeviceRelationshipState.PendingIncoming));
        }

        foreach (var transfer in targetedTransfers)
        {
            notices.Add(new SavedDeviceNotificationFact(
                "targeted:" + transfer.Id,
                transfer.State.ToString(),
                OutcomeKind(transfer.Direction, transfer.State),
                transfer.TransferName,
                transfer.PeerDisplayName,
                Pending: false));
        }

        return [.. notices];
    }
}
