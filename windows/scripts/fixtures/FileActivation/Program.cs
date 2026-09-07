using VniDrop.Core;
using VniDrop.Native;

var root = Path.GetFullPath(args.Single());
if (Directory.Exists(root)) throw new ArgumentException("Use a new fixture directory.");
Directory.CreateDirectory(root);
var source = Path.Combine(root, "Activation review.txt");
await File.WriteAllTextAsync(source, "VniDrop file activation review");
await using var sender = new CoreSession(Path.Combine(root, "sender"));
await sender.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
var share = await sender.RunAsync(c => c.ShareFiles(
    [new(SourceKind.Path, source, null, false)],
    new(73, "Activation review", "File activation fixture", TransferAccessMode.ApprovalRequired)));
await File.WriteAllTextAsync(Path.Combine(root, "Valid invitation été.VND"), share.ticket);
await File.WriteAllTextAsync(Path.Combine(root, "Invalid invitation été.vnd"), "invalid-invitation");
Console.WriteLine($"Invitation fixtures: {root}");
Console.WriteLine("The sender stops after export. Verify invitation review and cancel without receiving.");
