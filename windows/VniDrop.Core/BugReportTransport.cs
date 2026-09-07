using System.Net;
using System.Net.Http.Headers;
using System.Text.Json;

namespace VniDrop.Core;

public sealed class BugReportTransport(BugReportConfiguration configuration, HttpClient? client = null)
{
    private static readonly HttpClient DefaultClient = new(new SocketsHttpHandler
    {
        AllowAutoRedirect = false,
        ConnectTimeout = TimeSpan.FromSeconds(10),
        PooledConnectionLifetime = TimeSpan.FromMinutes(5),
    });

    public async Task SendAsync(PreparedBugReport report, CancellationToken cancellationToken = default)
    {
        if (report.Body.Length > BugReportSubmission.MaxRequestBytes)
            throw new InvalidDataException("windows_report_text_too_long");
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(30));
        using var request = new HttpRequestMessage(HttpMethod.Post, configuration.SubmissionUri);
        request.Headers.Add("X-VniDrop-Key", configuration.IngestKey);
        request.Headers.Add("X-VniDrop-Install-Id", report.InstallId);
        request.Headers.Accept.Add(new("application/json"));
        request.Content = new ByteArrayContent(report.Body.ToArray());
        request.Content.Headers.ContentType = new MediaTypeHeaderValue("application/json") { CharSet = "utf-8" };
        try
        {
            using var response = await (client ?? DefaultClient).SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token);
            if (!response.IsSuccessStatusCode)
                throw new InvalidDataException(response.StatusCode switch
                {
                    HttpStatusCode.Unauthorized or HttpStatusCode.Forbidden or HttpStatusCode.NotFound => "windows_report_service_configuration",
                    HttpStatusCode.TooManyRequests => "windows_report_rate_limited",
                    _ => "bug_report_submit_failed",
                });
            await using var stream = await response.Content.ReadAsStreamAsync(timeout.Token);
            var bytes = new byte[16 * 1024 + 1];
            var read = 0;
            while (read < bytes.Length)
            {
                var count = await stream.ReadAsync(bytes.AsMemory(read), timeout.Token);
                if (count == 0) break;
                read += count;
            }
            if (read == bytes.Length) throw new InvalidDataException("windows_report_unconfirmed");
            using var acknowledgement = JsonDocument.Parse(bytes.AsMemory(0, read), new() { MaxDepth = 32 });
            var root = acknowledgement.RootElement;
            var keys = new HashSet<string>(StringComparer.Ordinal);
            if (root.ValueKind != JsonValueKind.Object || root.EnumerateObject().Any(property => !keys.Add(property.Name)) ||
                !root.TryGetProperty("ok", out var ok) || ok.ValueKind != JsonValueKind.True ||
                !root.TryGetProperty("id", out var id) || id.ValueKind != JsonValueKind.String || id.GetString() != report.Id)
                throw new InvalidDataException("windows_report_unconfirmed");
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new InvalidDataException("windows_report_timeout");
        }
        catch (HttpRequestException)
        {
            throw new InvalidDataException("windows_report_connection_failed");
        }
        catch (JsonException)
        {
            throw new InvalidDataException("windows_report_unconfirmed");
        }
    }
}
