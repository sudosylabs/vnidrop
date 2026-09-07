using VniDrop.Core;
using VniDrop.Native;

var root = Path.GetFullPath(args.Single());
if (Directory.Exists(root)) throw new ArgumentException("Use a new fixture directory.");
Directory.CreateDirectory(root);
var profile = Path.Combine(root, "sender");
new AppPreferences { Username = "Windows", Theme = "Dark", RelayMode = CoreRelayMode.LocalOnly }.Save(profile);
await using var sender = new CoreSession(profile);
await sender.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
var source = Path.Combine(root, "Layout review");
Directory.CreateDirectory(source);
for (var index = 1; index <= 22; index++)
    await File.WriteAllTextAsync(Path.Combine(source, $"File {index:00}.txt"), new string('x', 1024));
var share = await sender.RunAsync(c => c.ShareFiles(
    [new(SourceKind.Path, source, null, true)],
    new(43, "Layout review", "Windows", TransferAccessMode.Public)));
for (var index = 1; index <= 12; index++)
{
    await using var receiver = new CoreSession(Path.Combine(root, $"receiver-{index:00}"));
    await receiver.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
    await receiver.RunAsync(c => c.Receive(share.ticket, Path.Combine(root, $"received-{index:00}"), $"Review device {index:00}"))
        .WaitAsync(TimeSpan.FromSeconds(30));
    foreach (var candidate in await sender.RunAsync(c => c.ListPairingEligibilities()))
        await sender.RunAsync(c => c.DeclinePairingEligibility(candidate.peerEndpointId));
    Console.WriteLine($"Completed fixture receiver {index}/12");
}
var requests = await sender.RunAsync(c => c.ListReceiverRequests(43));
if (requests.Length != 12) throw new InvalidOperationException($"Expected 12 receivers, got {requests.Length}.");
foreach (var candidate in await sender.RunAsync(c => c.ListPairingEligibilities()))
    await sender.RunAsync(c => c.DeclinePairingEligibility(candidate.peerEndpointId));
Console.WriteLine($"QA profile: {profile}");
