using System.Net;
using System.Net.Sockets;
using System.Text;
using VniDrop.Native;

namespace VniDrop.Core;

public static class RelaySettingsPolicy
{
    public const int MaximumUrls = 8;
    private const int MaximumUrlBytes = 2_048;
    private const string HttpsPrefix = "https://";

    public static bool UsesCustomUrls(CoreRelayMode mode) =>
        mode is CoreRelayMode.StrictCustom or CoreRelayMode.CustomWithDirectFallback;

    public static bool TryNormalize(
        CoreRelayMode mode,
        IEnumerable<string> relayUrls,
        IReadOnlyList<string> retainedUrls,
        out string[] normalizedUrls)
    {
        if (!UsesCustomUrls(mode))
        {
            normalizedUrls = [.. retainedUrls];
            return true;
        }

        var urls = relayUrls
            .Select(url => url.Trim())
            .Where(url => url.Length > 0)
            .ToArray();
        if (urls.Length is 0 or > MaximumUrls)
        {
            normalizedUrls = [];
            return false;
        }

        var normalized = new List<string>(urls.Length);
        var unique = new HashSet<string>(StringComparer.Ordinal);
        foreach (var url in urls)
        {
            if (!TryNormalizeUrl(url, out var value) || !unique.Add(value))
            {
                normalizedUrls = [];
                return false;
            }
            normalized.Add(value);
        }

        normalizedUrls = [.. normalized];
        return true;
    }

    private static bool TryNormalizeUrl(string raw, out string normalized)
    {
        normalized = "";
        if (!raw.StartsWith(HttpsPrefix, StringComparison.OrdinalIgnoreCase) ||
            Encoding.UTF8.GetByteCount(raw) > MaximumUrlBytes ||
            raw.Any(character => char.IsWhiteSpace(character) || char.IsControl(character)))
        {
            return false;
        }

        var remainder = raw[HttpsPrefix.Length..];
        if (remainder.Length == 0 || remainder.Contains('?') || remainder.Contains('#'))
        {
            return false;
        }

        var slash = remainder.IndexOf('/');
        var authority = slash < 0 ? remainder : remainder[..slash];
        var path = slash < 0 ? "" : remainder[slash..];
        if (path is not ("" or "/") || !TryNormalizeAuthority(authority, out var normalizedAuthority))
        {
            return false;
        }

        normalized = HttpsPrefix + (normalizedAuthority.EndsWith(":443", StringComparison.Ordinal)
            ? normalizedAuthority[..^4]
            : normalizedAuthority);
        return true;
    }

    private static bool TryNormalizeAuthority(string authority, out string normalized)
    {
        normalized = "";
        if (string.IsNullOrWhiteSpace(authority) || authority.Contains('@'))
        {
            return false;
        }

        if (authority.StartsWith('['))
        {
            var closingBracket = authority.IndexOf(']');
            if (closingBracket <= 1)
            {
                return false;
            }
            var address = authority[1..closingBracket];
            var suffix = authority[(closingBracket + 1)..];
            if (address.Any(character => !Uri.IsHexDigit(character) && character is not (':' or '.')) ||
                !IPAddress.TryParse(address, out var parsed) ||
                parsed.AddressFamily != AddressFamily.InterNetworkV6 ||
                !IsValidPortSuffix(suffix))
            {
                return false;
            }
            normalized = "[" + address.ToLowerInvariant() + "]" + suffix;
            return true;
        }

        if (authority.Count(character => character == ':') > 1)
        {
            return false;
        }
        var separator = authority.LastIndexOf(':');
        var host = separator < 0 ? authority : authority[..separator];
        var port = separator < 0 ? "" : authority[separator..];
        if (string.IsNullOrWhiteSpace(host) || host.StartsWith('.') || host.EndsWith('.') ||
            host.StartsWith('-') || host.EndsWith('-') ||
            host.Any(character => !char.IsLetterOrDigit(character) && character is not ('.' or '-')) ||
            !IsValidPortSuffix(port))
        {
            return false;
        }
        normalized = host.ToLowerInvariant() + port;
        return true;
    }

    private static bool IsValidPortSuffix(string suffix)
    {
        if (suffix.Length == 0)
        {
            return true;
        }
        var port = suffix[1..];
        return suffix.StartsWith(':') && port.Length > 0 && port.All(char.IsDigit) &&
            int.TryParse(port, out var value) && value is >= 1 and <= 65_535;
    }
}
