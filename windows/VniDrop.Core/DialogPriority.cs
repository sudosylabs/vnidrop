namespace VniDrop.Core;

public enum DialogPriority
{
    Standard,
    Attention,
}

public static class DialogPriorityPolicy
{
    public static bool IsHigher(DialogPriority candidate, DialogPriority current) => candidate > current;

    public static bool ShouldRunBefore(
        DialogPriority candidatePriority,
        long candidateSequence,
        DialogPriority currentPriority,
        long currentSequence) =>
        IsHigher(candidatePriority, currentPriority)
        || (candidatePriority == currentPriority && candidateSequence < currentSequence);

    public static bool ShouldRequestYield(
        DialogPriority active,
        DialogPriority incoming,
        bool activeCanYield) => activeCanYield && IsHigher(incoming, active);
}
