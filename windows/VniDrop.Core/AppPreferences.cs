using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using VniDrop.Native;

namespace VniDrop.Core;

public sealed record AppPreferences
{
    public string Username { get; init; } = Environment.MachineName;
    public string ReceiveDirectory { get; init; } = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");
    public string Theme { get; init; } = "System";
    public bool Notifications { get; init; }
    public CoreRelayMode RelayMode { get; init; } = CoreRelayMode.Automatic;
    public string[] RelayUrls { get; init; } = [];
    public string DiagnosticsInstallId { get; init; } = "";

    [JsonIgnore]
    public CoreNetworkConfig NetworkConfiguration => new(RelayMode, RelayMode is CoreRelayMode.Automatic or CoreRelayMode.LocalOnly ? [] : RelayUrls);

    public static AppPreferences Load(string directory)
    {
        var native = Path.Combine(directory, "windows-preferences.json");
        if (File.Exists(native))
        {
            if (new FileInfo(native).Length > 1024 * 1024) throw new InvalidDataException("windows_preferences_invalid");
            try { return (JsonSerializer.Deserialize<AppPreferences>(File.ReadAllText(native)) ?? throw new InvalidDataException("windows_preferences_invalid")).Validated(); }
            catch (JsonException ex) { throw new InvalidDataException("windows_preferences_invalid", ex); }
        }
        var legacy = Path.Combine(directory, "app_preferences.preferences_pb");
        if (!File.Exists(legacy)) return new();
        if (new FileInfo(legacy).Length > 1024 * 1024) throw new InvalidDataException("windows_preferences_invalid");
        return ImportLegacy(File.ReadAllBytes(legacy));
    }

    public void Save(string directory)
    {
        Validated();
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "windows-preferences.json");
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
            File.Move(temporary, path, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    // DataStore's PreferenceMap is a protobuf map<string, Value>. Reading it leaves the Kotlin profile intact.
    public static AppPreferences ImportLegacy(byte[] bytes)
    {
        var values = new Dictionary<string, object>();
        var map = new ProtoReader(bytes);
        while (map.More)
        {
            var tag = map.Varint();
            if (tag != 10) { map.Skip(tag); continue; }
            var entry = new ProtoReader(map.Bytes());
            string? key = null;
            object? value = null;
            while (entry.More)
            {
                var field = entry.Varint();
                if (field == 10) key = Encoding.UTF8.GetString(entry.Bytes());
                else if (field == 18)
                {
                    var item = new ProtoReader(entry.Bytes());
                    while (item.More)
                    {
                        var type = item.Varint();
                        if (type == 42) value = new UTF8Encoding(false, true).GetString(item.Bytes());
                        else if (type == 8) value = item.Varint() != 0;
                        else item.Skip(type);
                    }
                }
                else entry.Skip(field);
            }
            if (key is not null && value is not null) values[key] = value;
        }
        foreach (var policy in new[] { "relay_mode", "relay_urls" })
            if (values.TryGetValue(policy, out var value) && value is not string) throw new InvalidDataException("windows_preferences_invalid");
        string Text(string key, string fallback = "") => values.GetValueOrDefault(key) as string ?? fallback;
        var defaults = new AppPreferences();
        var mode = Text("relay_mode", "Automatic");
        if (!Enum.TryParse<CoreRelayMode>(mode, out var relayMode) || !Enum.IsDefined(relayMode))
            throw new InvalidDataException("windows_preferences_invalid");
        return (defaults with
        {
            Username = Text("username", defaults.Username), ReceiveDirectory = Text("receive_folder_value", defaults.ReceiveDirectory),
            Theme = Text("theme_mode", "System"), Notifications = values.GetValueOrDefault("notifications_enabled") is true,
            RelayMode = relayMode, RelayUrls = Text("relay_urls").Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries),
            DiagnosticsInstallId = Text("diagnostics_install_id"),
        }).Validated();
    }

    private AppPreferences Validated()
    {
        if (!Enum.IsDefined(RelayMode) || RelayUrls is null || RelayUrls.Any(url => url is null) ||
            string.IsNullOrWhiteSpace(Username) || string.IsNullOrWhiteSpace(ReceiveDirectory) || !Path.IsPathFullyQualified(ReceiveDirectory) ||
            Theme is not ("System" or "Light" or "Dark")) throw new InvalidDataException("windows_preferences_invalid");
        return this;
    }

    private sealed class ProtoReader(byte[] data)
    {
        private int position;
        public bool More => position < data.Length;
        public ulong Varint()
        {
            ulong value = 0;
            for (var shift = 0; shift < 64; shift += 7)
            {
                if (!More) throw new InvalidDataException("windows_preferences_invalid");
                var next = data[position++];
                if (shift == 63 && next > 1) throw new InvalidDataException("windows_preferences_invalid");
                value |= (ulong)(next & 127) << shift;
                if (next < 128) return value;
            }
            throw new InvalidDataException("windows_preferences_invalid");
        }
        public byte[] Bytes()
        {
            var size = checked((int)Varint());
            if (size > data.Length - position) throw new InvalidDataException("windows_preferences_invalid");
            var result = data.AsSpan(position, size).ToArray(); position += size; return result;
        }
        public void Skip(ulong tag)
        {
            switch (tag & 7)
            {
                case 0: Varint(); break;
                case 1: position = checked(position + 8); break;
                case 2: Bytes(); break;
                case 5: position = checked(position + 4); break;
                default: throw new InvalidDataException("windows_preferences_invalid");
            }
            if (position > data.Length) throw new InvalidDataException("windows_preferences_invalid");
        }
    }
}
