namespace VniDrop.Core;

public sealed class BugReportConfiguration
{
    public Uri SubmissionUri { get; }
    internal string IngestKey { get; }

    private BugReportConfiguration(Uri endpoint, string key)
    {
        SubmissionUri = new(endpoint.AbsoluteUri.TrimEnd('/') + "/v1/bugs");
        IngestKey = key;
    }

    public static BugReportConfiguration? Create(string endpoint, string key)
    {
        endpoint = endpoint.Trim();
        key = key.Trim();
        if (endpoint.Length == 0 && key.Length == 0) return null;
        if (!Uri.TryCreate(endpoint, UriKind.Absolute, out var uri) ||
            uri.Host.Length == 0 || uri.UserInfo.Length != 0 || uri.Query.Length != 0 || uri.Fragment.Length != 0 ||
            endpoint.Any(char.IsWhiteSpace) ||
            (uri.Scheme != "https" && !(uri.Scheme == "http" && uri.IsLoopback)) ||
            key.Length is 0 or > 4096 || key.Any(char.IsControl))
            throw new InvalidDataException("windows_report_unconfigured");
        return new(uri, key);
    }
}
