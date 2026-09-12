using VniDrop.Core;
using VniDrop.Native;
using Xunit;

namespace VniDrop.Tests;

public class SavedDevicesReadModelTests
{
    [Theory]
    [InlineData(TargetedTransferState.Preparing)]
    [InlineData(TargetedTransferState.Offering)]
    [InlineData(TargetedTransferState.AwaitingApproval)]
    [InlineData(TargetedTransferState.Approved)]
    [InlineData(TargetedTransferState.Connecting)]
    [InlineData(TargetedTransferState.Transferring)]
    [InlineData(TargetedTransferState.Interrupted)]
    public void TransferActionMatrixIsDerivedFromEvaluate(TargetedTransferState state)
    {
        var outgoing = Item(state, TargetedTransferRole.Sender);
        var incoming = Item(state, TargetedTransferRole.Receiver);
        Assert.Equal([SavedDeviceTransferAction.Cancel], outgoing.AvailableActions);
        Assert.Equal(
            state switch
            {
                TargetedTransferState.Approved => new[]
                {
                    SavedDeviceTransferAction.Receive,
                    SavedDeviceTransferAction.Cancel,
                },
                TargetedTransferState.Interrupted => new[]
                {
                    SavedDeviceTransferAction.Resume,
                    SavedDeviceTransferAction.Cancel,
                },
                _ => [SavedDeviceTransferAction.Cancel],
            },
            incoming.AvailableActions);
        AssertAvailabilityMatchesEvaluate(outgoing);
        AssertAvailabilityMatchesEvaluate(incoming);
    }

    [Theory]
    [InlineData(TargetedTransferState.Completed)]
    [InlineData(TargetedTransferState.Declined)]
    [InlineData(TargetedTransferState.Cancelled)]
    [InlineData(TargetedTransferState.Failed)]
    public void TerminalTransfersExposeDeleteForBothRoles(TargetedTransferState state)
    {
        foreach (var role in new[] { TargetedTransferRole.Sender, TargetedTransferRole.Receiver })
        {
            var item = Item(state, role);
            Assert.Equal([SavedDeviceTransferAction.Delete], item.AvailableActions);
            AssertAvailabilityMatchesEvaluate(item);
        }
    }

    [Fact]
    public void DurableRoleOwnsDirectionEvenWhenEndpointIdentityHasChanged()
    {
        var outgoing = Transfer(
            "outgoing",
            TargetedTransferRole.Sender,
            TargetedTransferState.Completed,
            updatedAt: 1,
            senderEndpointId: "retired-local-identity",
            receiverEndpointId: "phone");
        var incoming = Transfer(
            "incoming",
            TargetedTransferRole.Receiver,
            TargetedTransferState.Completed,
            updatedAt: 2,
            senderEndpointId: "laptop",
            receiverEndpointId: "retired-local-identity");

        var items = SavedDevicesReadModel.Derive(Inputs(targetedTransfers: [outgoing, incoming]))
            .TargetedTransfers
            .ToDictionary(item => item.Id);

        Assert.Equal(SavedDeviceTransferDirection.Outgoing, items["outgoing"].Direction);
        Assert.Equal("phone", items["outgoing"].PeerEndpointId);
        Assert.Equal(SavedDeviceTransferDirection.Incoming, items["incoming"].Direction);
        Assert.Equal("laptop", items["incoming"].PeerEndpointId);
    }

    [Fact]
    public void ProgressFactsAreReceiverOnlyAndSurviveInterruption()
    {
        foreach (var state in new[]
        {
            TargetedTransferState.Connecting,
            TargetedTransferState.Transferring,
            TargetedTransferState.Interrupted,
        })
        {
            Assert.Equal(0.4d, Item(state, TargetedTransferRole.Receiver).ProgressFraction!.Value);
            Assert.Null(Item(state, TargetedTransferRole.Sender).ProgressFraction);
        }

        Assert.Null(Item(TargetedTransferState.Approved, TargetedTransferRole.Receiver).ProgressFraction);
        Assert.Null(Item(TargetedTransferState.Completed, TargetedTransferRole.Receiver).ProgressFraction);
        Assert.Null(SavedDevicesReadModel.TransferItem(
            Transfer("empty", TargetedTransferRole.Receiver, TargetedTransferState.Transferring, 1, totalSize: 0),
            null).ProgressFraction);
    }

    [Fact]
    public void DurableReadsProduceOneSortedJoinedPlatformSnapshot()
    {
        var snapshot = SavedDevicesReadModel.Derive(
            Inputs(
                eligibilities: [Eligibility("dismissed", 4), Eligibility("eligible", 2)],
                relationships:
                [
                    Relationship("old-outgoing", DeviceRelationshipState.PendingOutgoing, 1),
                    Relationship("incoming", DeviceRelationshipState.PendingIncoming, 3),
                    Relationship("saved", DeviceRelationshipState.Saved, 5),
                ],
                savedDevices:
                [
                    Device("peer", "Desk", "Remote", 2),
                    Device("older", null, "Laptop", 1),
                ],
                pendingOffers: [Offer("later", 9), Offer("earlier", 7)],
                targetedTransfers:
                [
                    Transfer("old", TargetedTransferRole.Sender, TargetedTransferState.Completed, 1),
                    Transfer("deleted", TargetedTransferRole.Sender, TargetedTransferState.Deleted, 3),
                    Transfer("new", TargetedTransferRole.Receiver, TargetedTransferState.Interrupted, 2),
                ]),
            dismissedKeys: new HashSet<string> { "eligibility:dismissed:session-dismissed" });

        Assert.Equal(["dismissed", "eligible"], snapshot.Eligibilities.Select(item => item.peerEndpointId));
        Assert.Equal(["incoming", "old-outgoing"], snapshot.PendingRelationships.Select(item => item.remoteEndpointId));
        Assert.Equal(["peer", "older"], snapshot.SavedDevices.Select(item => item.endpointId));
        Assert.Equal(["earlier", "later"], snapshot.PendingOffers.Select(item => item.transferId));
        Assert.Equal(["new", "old"], snapshot.TargetedTransfers.Select(item => item.Id));
        Assert.Equal("Desk", snapshot.TargetedTransfers.Single(item => item.Id == "old").PeerDisplayName);
        Assert.Equal(PairingPromptKind.IncomingRequest, snapshot.NextPairingPrompt?.Kind);
        Assert.Equal("incoming", snapshot.NextPairingPrompt?.PeerEndpointId);
    }

    [Fact]
    public void RelationshipRowsDoNotSynthesizeSavedDevicesOrTerminalDecisions()
    {
        var snapshot = SavedDevicesReadModel.Derive(Inputs(relationships:
        [
            Relationship("saved-without-device", DeviceRelationshipState.Saved, 4),
            Relationship("revoked", DeviceRelationshipState.Revoked, 3),
            Relationship("blocked", DeviceRelationshipState.Blocked, 2),
            Relationship("pending", DeviceRelationshipState.PendingOutgoing, 1),
        ]));

        Assert.Equal(["pending"], snapshot.PendingRelationships.Select(item => item.remoteEndpointId));
        Assert.Empty(snapshot.SavedDevices);
        Assert.Null(snapshot.NextPairingPrompt);
        Assert.Null(snapshot.Notifications.Single(notice => notice.Id == "pairing:pending").Kind);
    }

    [Fact]
    public void LifecycleInputsRemainIndependentDurableHistoryFacts()
    {
        var snapshot = SavedDevicesReadModel.Derive(Inputs(
            pendingOffers: [Offer("pending-live-offer", 1)],
            targetedTransfers:
            [
                Transfer("cancelled", TargetedTransferRole.Sender, TargetedTransferState.Cancelled, 4),
                Transfer("approval-race", TargetedTransferRole.Receiver, TargetedTransferState.Cancelled, 3),
                Transfer("failed-attempt", TargetedTransferRole.Sender, TargetedTransferState.Failed, 2),
                Transfer("retry", TargetedTransferRole.Sender, TargetedTransferState.Offering, 1),
            ]));

        Assert.Equal(
            ["cancelled", "approval-race", "failed-attempt", "retry"],
            snapshot.TargetedTransfers.Select(item => item.Id));
        Assert.Equal(
            [SavedDeviceTransferAction.Delete],
            snapshot.TargetedTransfers.Single(item => item.Id == "cancelled").AvailableActions);
        Assert.Equal(
            [SavedDeviceTransferAction.Delete],
            snapshot.TargetedTransfers.Single(item => item.Id == "approval-race").AvailableActions);
        Assert.Equal(
            [SavedDeviceTransferAction.Delete],
            snapshot.TargetedTransfers.Single(item => item.Id == "failed-attempt").AvailableActions);
        Assert.Equal(
            [SavedDeviceTransferAction.Cancel],
            snapshot.TargetedTransfers.Single(item => item.Id == "retry").AvailableActions);
        Assert.Equal(["pending-live-offer"], snapshot.PendingOffers.Select(item => item.transferId));
        Assert.Null(Notice(snapshot, "targeted:cancelled").Kind);
        Assert.Null(Notice(snapshot, "targeted:approval-race").Kind);
    }

    [Fact]
    public void DisplayNamePrefersLocalLabelThenAuthenticatedRemoteName()
    {
        Assert.Equal("Desk", SavedDevicesReadModel.DisplayName(Device("peer", "Desk", "Remote", 1)));
        Assert.Equal("Laptop", SavedDevicesReadModel.DisplayName(Device("peer", "  ", "Laptop", 1)));
        Assert.Equal("Laptop", SavedDevicesReadModel.DisplayName(Device("peer", null, "Laptop", 1)));
        Assert.Null(SavedDevicesReadModel.DisplayName(Device("peer", " ", null, 1)));
        Assert.Null(SavedDevicesReadModel.DisplayName(null));
    }

    [Fact]
    public void SenderCompletedAndFailedUseSendWordingNotReceiveWording()
    {
        var snapshot = SavedDevicesReadModel.Derive(Inputs(targetedTransfers:
        [
            Transfer("sent-ok", TargetedTransferRole.Sender, TargetedTransferState.Completed, 4),
            Transfer("sent-fail", TargetedTransferRole.Sender, TargetedTransferState.Failed, 3),
            Transfer("got-ok", TargetedTransferRole.Receiver, TargetedTransferState.Completed, 2),
            Transfer("got-fail", TargetedTransferRole.Receiver, TargetedTransferState.Failed, 1),
        ]));

        Assert.Equal(SavedDeviceNotificationKind.TargetedSendCompleted, Notice(snapshot, "targeted:sent-ok").Kind);
        Assert.Equal(SavedDeviceNotificationKind.TargetedSendFailed, Notice(snapshot, "targeted:sent-fail").Kind);
        Assert.Equal(SavedDeviceNotificationKind.TargetedReceiveCompleted, Notice(snapshot, "targeted:got-ok").Kind);
        Assert.Equal(SavedDeviceNotificationKind.TargetedReceiveFailed, Notice(snapshot, "targeted:got-fail").Kind);

        Assert.Equal("notifications_receiver_completed_title", SavedDevicesReadModel.TitleKey(Notice(snapshot, "targeted:sent-ok").Kind));
        Assert.Equal("notifications_receiver_failed_title", SavedDevicesReadModel.TitleKey(Notice(snapshot, "targeted:sent-fail").Kind));
        Assert.Equal("notifications_receive_completed_title", SavedDevicesReadModel.TitleKey(Notice(snapshot, "targeted:got-ok").Kind));
        Assert.Equal("notifications_receive_failed_title", SavedDevicesReadModel.TitleKey(Notice(snapshot, "targeted:got-fail").Kind));
        Assert.NotEqual(
            SavedDevicesReadModel.TitleKey(SavedDeviceNotificationKind.TargetedSendCompleted),
            SavedDevicesReadModel.TitleKey(SavedDeviceNotificationKind.TargetedReceiveCompleted));
        Assert.NotEqual(
            SavedDevicesReadModel.TitleKey(SavedDeviceNotificationKind.TargetedSendFailed),
            SavedDevicesReadModel.TitleKey(SavedDeviceNotificationKind.TargetedReceiveFailed));
    }

    [Fact]
    public void UserDrivenTerminalStatesAreNotNotifiable()
    {
        var snapshot = SavedDevicesReadModel.Derive(Inputs(targetedTransfers:
        [
            Transfer("cancelled", TargetedTransferRole.Receiver, TargetedTransferState.Cancelled, 3),
            Transfer("declined", TargetedTransferRole.Sender, TargetedTransferState.Declined, 2),
            Transfer("active", TargetedTransferRole.Receiver, TargetedTransferState.Transferring, 1),
        ]));

        Assert.All(
            snapshot.Notifications.Where(notice => notice.Id.StartsWith("targeted:", StringComparison.Ordinal)),
            notice => Assert.Null(notice.Kind));
    }

    [Fact]
    public void IncomingPairingAndOffersAreNotifiablePrompts()
    {
        var snapshot = SavedDevicesReadModel.Derive(Inputs(
            eligibilities: [Eligibility("incoming", 1, "Alice")],
            relationships:
            [
                Relationship("incoming", DeviceRelationshipState.PendingIncoming, 2),
                Relationship("outgoing", DeviceRelationshipState.PendingOutgoing, 1),
            ],
            savedDevices: [Device("peer", "Studio", "Remote", 1)],
            pendingOffers: [Offer("photos", 1, "peer")]));

        var offer = Notice(snapshot, "offer:photos");
        Assert.Equal(SavedDeviceNotificationKind.TargetedOffer, offer.Kind);
        Assert.Equal("Studio", offer.DeviceName);
        Assert.Equal("receive_review_title", SavedDevicesReadModel.TitleKey(offer.Kind));

        var incoming = Notice(snapshot, "pairing:incoming");
        Assert.Equal(SavedDeviceNotificationKind.PairingRequest, incoming.Kind);
        Assert.Equal("saved_devices_pending_incoming", SavedDevicesReadModel.TitleKey(incoming.Kind));
        Assert.Null(Notice(snapshot, "pairing:outgoing").Kind);
    }

    [Fact]
    public void RefreshAndRestartNeedOnlyDurableReadsNotAnEventPayload()
    {
        var inputs = Inputs(
            savedDevices: [Device("peer", null, "Phone", 1)],
            targetedTransfers: [Transfer("transfer", TargetedTransferRole.Sender, TargetedTransferState.Failed, 1)]);

        var first = SavedDevicesReadModel.Derive(inputs);
        var second = SavedDevicesReadModel.Derive(inputs);
        Assert.Equal(
            first.TargetedTransfers.Select(item => (item.Id, item.Direction, item.State, item.PeerDisplayName)),
            second.TargetedTransfers.Select(item => (item.Id, item.Direction, item.State, item.PeerDisplayName)));
        Assert.Equal(first.TargetedTransfers.Single().AvailableActions, second.TargetedTransfers.Single().AvailableActions);
        Assert.Equal(
            first.Notifications.Select(notice => (notice.Id, notice.Kind, notice.TransferName)),
            second.Notifications.Select(notice => (notice.Id, notice.Kind, notice.TransferName)));
    }

    private static void AssertAvailabilityMatchesEvaluate(SavedDeviceTransferFact item)
    {
        var availability = TargetedTransferActionPolicy.Evaluate(item.Role, item.State, false, false);
        Assert.Equal(
            availability.CanStart,
            item.AvailableActions.Contains(SavedDeviceTransferAction.Receive)
                || item.AvailableActions.Contains(SavedDeviceTransferAction.Resume));
        Assert.Equal(availability.CanCancel, item.AvailableActions.Contains(SavedDeviceTransferAction.Cancel));
        Assert.Equal(availability.CanDelete, item.AvailableActions.Contains(SavedDeviceTransferAction.Delete));
    }

    private static SavedDeviceNotificationFact Notice(SavedDevicesReadSnapshot snapshot, string id) =>
        Assert.Single(snapshot.Notifications, notice => notice.Id == id);

    private static SavedDeviceTransferFact Item(TargetedTransferState state, TargetedTransferRole role) =>
        SavedDevicesReadModel.Derive(Inputs(targetedTransfers: [Transfer("id", role, state, 1)]))
            .TargetedTransfers
            .Single();

    private static SavedDevicesReadInputs Inputs(
        PairingEligibilitySummary[]? eligibilities = null,
        DeviceRelationship[]? relationships = null,
        SavedDevice[]? savedDevices = null,
        PendingTargetedOffer[]? pendingOffers = null,
        TargetedTransfer[]? targetedTransfers = null) => new(
        eligibilities ?? [],
        relationships ?? [],
        savedDevices ?? [],
        pendingOffers ?? [],
        targetedTransfers ?? []);

    private static PairingEligibilitySummary Eligibility(string peer, long createdAt, string? name = null) => new(
        peer, name, "session-" + peer, 1, createdAt, createdAt + 10);

    private static DeviceRelationship Relationship(string peer, DeviceRelationshipState state, long updatedAt) =>
        new(peer, state, 1, 1, 0, updatedAt);

    private static SavedDevice Device(string id, string? label, string? remoteName, long createdAt) =>
        new(id, label, remoteName, createdAt, createdAt);

    private static PendingTargetedOffer Offer(string id, long receivedAt, string sender = "peer") => new(
        id, sender, "local", "manifest-" + id, "hash-" + id, id, 1, 100, 1, receivedAt);

    private static TargetedTransfer Transfer(
        string id,
        TargetedTransferRole role,
        TargetedTransferState state,
        long updatedAt,
        string? senderEndpointId = null,
        string? receiverEndpointId = null,
        ulong totalSize = 100) => new(
        id,
        role,
        senderEndpointId ?? (role == TargetedTransferRole.Sender ? "local" : "peer"),
        receiverEndpointId ?? (role == TargetedTransferRole.Sender ? "peer" : "local"),
        "manifest-" + id,
        id,
        1,
        totalSize,
        40,
        state,
        0,
        updatedAt);
}
