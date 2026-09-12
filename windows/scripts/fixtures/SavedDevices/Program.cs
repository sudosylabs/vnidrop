using System.Diagnostics;
using VniDrop.Core;
using VniDrop.Native;

var root = Path.GetFullPath(args.Single());
if (Directory.Exists(root)) throw new ArgumentException("Use a new fixture directory.");
Directory.CreateDirectory(root);
var profile = Path.Combine(root, "sender");
new AppPreferences { Username = "Windows", Theme = "Dark", RelayMode = CoreRelayMode.LocalOnly }.Save(profile);
await using var sender = new CoreSession(profile);
await using var receiver = new CoreSession(Path.Combine(root, "receiver"));
await sender.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
await receiver.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
var source = Path.Combine(root, "Photos été.txt");
await File.WriteAllTextAsync(source, "Saved devices visual verification");
var share = await sender.RunAsync(c => c.ShareFiles([new(SourceKind.Path, source, null, false)],
    new(47, "Holiday photos", "Windows", TransferAccessMode.Public)));
await receiver.RunAsync(c => c.Receive(share.ticket, Path.Combine(root, "invitation"), "Travel laptop with a long device name"));
var senderId = (await sender.SnapshotAsync()).Status.endpointId;
var receiverId = (await receiver.SnapshotAsync()).Status.endpointId;
await Until(async () => (await sender.SnapshotAsync()).EligibleDevices.Length > 0);
await sender.RunAsync(c => c.RequestSavedDevicePairing(receiverId));
await Until(async () => (await receiver.SnapshotAsync()).Relationships.Any(r => r.state == DeviceRelationshipState.PendingIncoming));
await receiver.RunAsync(c => c.RespondToDevicePairing(senderId, true));
await Until(async () => (await sender.SnapshotAsync()).Devices.Length == 1);
using var preparation = await sender.RunAsync(c => c.NewTargetedTransferPreparation(receiverId));
var transfer = await sender.RunAsync(_ => preparation.Send([new(SourceKind.Path, source, null, false)], "Completed holiday photos"));
await Until(async () => (await receiver.SnapshotAsync()).Offers.Length == 1);
await receiver.RunAsync(c => c.RespondToTargetedOffer(transfer.id, true));
await receiver.RunAsync(c => c.ReceiveTargetedTransfer(transfer.id, Path.Combine(root, "targeted")));
await Until(async () => (await sender.SnapshotAsync()).TargetedTransfers.Single().state == TargetedTransferState.Completed);
Console.WriteLine($"QA profile: {profile}");

static async Task Until(Func<Task<bool>> predicate)
{
    var timer = Stopwatch.StartNew();
    while (!await predicate())
    {
        if (timer.Elapsed > TimeSpan.FromSeconds(20)) throw new TimeoutException("Fixture state did not arrive.");
        await Task.Delay(30);
    }
}
