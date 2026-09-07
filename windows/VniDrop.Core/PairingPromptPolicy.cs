using VniDrop.Native;

namespace VniDrop.Core;

public enum PairingPromptKind
{
    IncomingRequest,
    Eligibility,
}

public sealed record PairingPromptCandidate(
    PairingPromptKind Kind,
    string PeerEndpointId,
    string? RemoteDisplayName,
    string Key);

public static class PairingPromptPolicy
{
    public static PairingPromptCandidate? Next(
        IEnumerable<DeviceRelationship> relationships,
        IEnumerable<PairingEligibilitySummary> eligibilities,
        IReadOnlySet<string> dismissedKeys)
    {
        var eligibilityRows = eligibilities.ToArray();
        var incoming = relationships
            .Where(relationship => relationship.state == DeviceRelationshipState.PendingIncoming)
            .OrderByDescending(relationship => relationship.updatedAt)
            .Select(relationship => new PairingPromptCandidate(
                PairingPromptKind.IncomingRequest,
                relationship.remoteEndpointId,
                eligibilityRows.FirstOrDefault(eligibility =>
                    eligibility.peerEndpointId == relationship.remoteEndpointId)?.remoteDisplayName,
                IncomingKey(relationship)))
            .ToArray();
        var nextIncoming = incoming.FirstOrDefault(candidate => !dismissedKeys.Contains(candidate.Key));
        if (nextIncoming is not null)
        {
            return nextIncoming;
        }

        return eligibilityRows
            .OrderByDescending(eligibility => eligibility.createdAt)
            .Select(eligibility => new PairingPromptCandidate(
                PairingPromptKind.Eligibility,
                eligibility.peerEndpointId,
                eligibility.remoteDisplayName,
                EligibilityKey(eligibility)))
            .FirstOrDefault(candidate => !dismissedKeys.Contains(candidate.Key));
    }

    public static bool IsPending(
        PairingPromptCandidate candidate,
        IEnumerable<DeviceRelationship> relationships,
        IEnumerable<PairingEligibilitySummary> eligibilities) =>
        candidate.Kind == PairingPromptKind.IncomingRequest
            ? relationships.Any(relationship =>
                relationship.state == DeviceRelationshipState.PendingIncoming
                && relationship.remoteEndpointId == candidate.PeerEndpointId
                && IncomingKey(relationship) == candidate.Key)
            : eligibilities.Any(eligibility =>
                eligibility.peerEndpointId == candidate.PeerEndpointId
                && EligibilityKey(eligibility) == candidate.Key);

    public static bool HasPending(
        IEnumerable<ReceiverRequest> receiverRequests,
        IEnumerable<PendingTargetedOffer> targetedOffers,
        IEnumerable<DeviceRelationship> relationships,
        IEnumerable<PairingEligibilitySummary> eligibilities) =>
        receiverRequests.Any(request => request.status == "requested")
        || targetedOffers.Any()
        || relationships.Any(relationship => relationship.state == DeviceRelationshipState.PendingIncoming)
        || eligibilities.Any();

    private static string IncomingKey(DeviceRelationship relationship) =>
        $"pairing:{relationship.remoteEndpointId}:{relationship.generation}";

    private static string EligibilityKey(PairingEligibilitySummary eligibility) =>
        $"eligibility:{eligibility.peerEndpointId}:{eligibility.sessionId}";
}
