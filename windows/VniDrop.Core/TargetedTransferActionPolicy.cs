using VniDrop.Native;

namespace VniDrop.Core;

public readonly record struct TargetedTransferActionAvailability(
    bool CanStart,
    bool CanCancel,
    bool CanDelete,
    bool CanOpenActions,
    bool ShowBusy);

public static class TargetedTransferActionPolicy
{
    public static TargetedTransferActionAvailability Evaluate(
        TargetedTransferRole role,
        TargetedTransferState state,
        bool receiveRunning,
        bool mutationBusy)
    {
        var terminal = state is TargetedTransferState.Completed
            or TargetedTransferState.Declined
            or TargetedTransferState.Cancelled
            or TargetedTransferState.Failed
            or TargetedTransferState.Deleted;
        var canStart = role == TargetedTransferRole.Receiver
            && state is TargetedTransferState.Approved or TargetedTransferState.Interrupted;

        return new TargetedTransferActionAvailability(
            CanStart: canStart && !receiveRunning && !mutationBusy,
            CanCancel: !terminal && !mutationBusy,
            CanDelete: terminal && !receiveRunning && !mutationBusy,
            CanOpenActions: !mutationBusy && (!receiveRunning || !terminal),
            ShowBusy: receiveRunning || mutationBusy);
    }
}
