using System.Text;
using System.Text.Json;
using VniDrop.Core;
using VniDrop.Native;

if (args.Length != 2 || args[0] is not ("seed" or "verify")) throw new ArgumentException("Usage: UpgradeProfile seed|verify <profile>");
var profile = Path.GetFullPath(args[1]);
var parent = Path.GetDirectoryName(profile)!;
var expectedPath = Path.Combine(parent, "upgrade-expected.json");
var legacyPath = Path.Combine(profile, "app_preferences.preferences_pb");
var destination = Path.Combine(parent, "received été");
var contents = "Existing received file — preserved across the native upgrade";
if (args[0] == "seed")
{
    if (Directory.Exists(profile) && Directory.EnumerateFileSystemEntries(profile).Any()) throw new IOException("Upgrade fixture needs an empty profile");
    Directory.CreateDirectory(profile);
    var source = Path.Combine(parent, "received-é.txt");
    await File.WriteAllTextAsync(source, contents);
    var entries = new List<byte>();
    void Text(string key, string value) => entries.AddRange(Field(10, Field(10, Encoding.UTF8.GetBytes(key)).Concat(Field(18, Field(42, Encoding.UTF8.GetBytes(value)))).ToArray()));
    Text("username", "Migration test");
    Text("theme_mode", "Dark");
    Text("receive_folder_value", destination);
    Text("relay_mode", "LocalOnly");
    Text("diagnostics_install_id", "native-upgrade-fixture");
    entries.AddRange(Field(10, Field(10, Encoding.UTF8.GetBytes("notifications_enabled")).Concat(Field(18, [8, 1])).ToArray()));
    await File.WriteAllBytesAsync(legacyPath, entries.ToArray());
    await using var sender = new CoreSession(Path.Combine(parent, "fixture-sender"));
    await using var receiver = new CoreSession(profile);
    await sender.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
    await receiver.InitializeAsync(new(CoreRelayMode.LocalOnly, []));
    var share = await sender.RunAsync(c => c.ShareFiles([new(SourceKind.Path, source, null, false)], new(9033, "Existing transfer", "Old sender", TransferAccessMode.Public)));
    await receiver.RunAsync(c => c.Receive(share.ticket, destination, "Migration test")).WaitAsync(TimeSpan.FromSeconds(30));
    var identity = (await receiver.RunAsync(c => c.Status())).endpointId;
    var file = (await receiver.RunAsync(c => c.ListReceivedArtifacts())).Single().locator;
    await File.WriteAllTextAsync(expectedPath, JsonSerializer.Serialize(new Expected(identity, file, Convert.ToBase64String(entries.ToArray()))));
}
else
{
    var expected = JsonSerializer.Deserialize<Expected>(await File.ReadAllTextAsync(expectedPath))!;
    if (Convert.ToBase64String(await File.ReadAllBytesAsync(legacyPath)) != expected.LegacyPreferences) throw new IOException("Upgrade modified the Compose preferences");
    var preferences = AppPreferences.Load(profile);
    if (preferences.Username != "Migration test" || preferences.Theme != "Dark" || !preferences.Notifications || preferences.ReceiveDirectory != destination ||
        preferences.RelayMode != CoreRelayMode.LocalOnly || preferences.DiagnosticsInstallId != "native-upgrade-fixture") throw new IOException("Legacy preferences did not migrate");
    await using var session = new CoreSession(profile);
    await session.InitializeAsync(preferences.NetworkConfiguration);
    if ((await session.RunAsync(c => c.Status())).endpointId != expected.Identity) throw new IOException("Upgrade changed the existing identity");
    var transfer = (await session.RunAsync(c => c.ListTransfers())).Single();
    if (transfer.transferId != 9033 || transfer.direction != "receive" || transfer.status != "done") throw new IOException("Upgrade changed transfer history");
    if ((await session.RunAsync(c => c.ListReceivedArtifacts())).Single().locator != expected.File || await File.ReadAllTextAsync(expected.File) != contents) throw new IOException("Upgrade changed received files");
}
Console.WriteLine($"PASS: upgrade profile {args[0]} (identity, Compose preferences, received history and file bytes).");

static byte[] Field(byte tag, byte[] data)
{
    var bytes = new List<byte> { tag };
    var remaining = data.Length;
    do { bytes.Add((byte)((remaining & 127) | (remaining > 127 ? 128 : 0))); remaining >>= 7; } while (remaining > 0);
    bytes.AddRange(data);
    return bytes.ToArray();
}
record Expected(string Identity, string File, string LegacyPreferences);
