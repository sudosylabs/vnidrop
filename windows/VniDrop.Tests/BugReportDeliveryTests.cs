using System.Net;
using System.Text;
using System.Text.Json;
using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class BugReportDeliveryTests : IDisposable
{
    private readonly string profile = Path.Combine(Path.GetTempPath(), "vnidrop-report-delivery", Guid.NewGuid().ToString("N"));
    private const string InstallId = "16bdca40-12f9-4c7d-91b3-f005ac3420fb";
    private static readonly BugReportDraft Draft = new(" A transfer failed. ", " It should complete. ", "Start a transfer.", "", false);
    private static readonly BugReportEnvironment EnvironmentDetails = new("0.3.3", "Windows", "TEST-PC", "Example model", "X64", DateTimeOffset.UtcNow);

    [Theory]
    [InlineData("https://example.test", "fixture")]
    [InlineData("https://example.test/reports/", "fixture")]
    [InlineData("http://127.0.0.1:1234", "fixture")]
    public void ConfigurationRoutesToTheBugIngestEndpoint(string endpoint, string key)
    {
        var configuration = BugReportConfiguration.Create(endpoint, key);
        Assert.Equal(endpoint.TrimEnd('/') + "/v1/bugs", configuration!.SubmissionUri.AbsoluteUri);
    }

    [Theory]
    [InlineData("http://example.test", "fixture")]
    [InlineData("https://user:password@example.test", "fixture")]
    [InlineData("https://example.test?key=wrong", "fixture")]
    [InlineData("https://example.test#wrong", "fixture")]
    [InlineData("", "fixture")]
    [InlineData("https://example.test", "")]
    [InlineData("https://example.test", "fixture\r\nInjected: value")]
    public void InvalidConfigurationCannotSubmit(string endpoint, string key) =>
        Assert.Throws<InvalidDataException>(() => BugReportConfiguration.Create(endpoint, key));

    [Fact]
    public void UnconfiguredDevelopmentBuildHasNoTransport() => Assert.Null(BugReportConfiguration.Create("", ""));

    [Fact]
    public void SubmissionRetriesReuseTheDocumentUntilTheDraftChanges()
    {
        var submission = new BugReportSubmission();
        var first = submission.Prepare(Draft, EnvironmentDetails, profile, InstallId);
        Assert.Same(first, submission.Prepare(Draft with { }, EnvironmentDetails with { CreatedAt = DateTimeOffset.UtcNow.AddMinutes(1) }, profile, InstallId));
        var edited = submission.Prepare(Draft with { Expected = "A different expectation." }, EnvironmentDetails, profile, InstallId);
        Assert.NotEqual(first.Id, edited.Id);
        submission.Clear();
        Assert.NotEqual(edited.Id, submission.Prepare(Draft with { Expected = "A different expectation." }, EnvironmentDetails, profile, InstallId).Id);
    }

    [Fact]
    public void PayloadMatchesTheServiceAndHonorsLogOptOut()
    {
        Directory.CreateDirectory(Path.Combine(profile, "logs"));
        File.WriteAllText(Path.Combine(profile, "logs", "vnidrop.log"), "private-log-marker");
        var report = new BugReportSubmission().Prepare(Draft, EnvironmentDetails, profile, InstallId);
        using var body = JsonDocument.Parse(report.Body);
        var json = body.RootElement;
        Assert.Equal(report.Id, json.GetProperty("id").GetString());
        Assert.Equal(InstallId, json.GetProperty("installId").GetString());
        Assert.Equal(1, json.GetProperty("schemaVersion").GetInt32());
        Assert.Equal("windows-native", json.GetProperty("platform").GetString());
        Assert.Equal(EnvironmentDetails.CreatedAt.ToUnixTimeMilliseconds(), json.GetProperty("timestampMillis").GetInt64());
        Assert.Equal("A transfer failed.", json.GetProperty("whatHappened").GetString());
        Assert.False(json.GetProperty("includeLogs").GetBoolean());
        Assert.Equal("", json.GetProperty("logs").GetString());
        Assert.Equal("TEST-PC", json.GetProperty("device").GetProperty("deviceName").GetString());
    }

    [Fact]
    public void JsonEscapingCannotExceedTheRequestLimitAndLogsRemainRedacted()
    {
        Directory.CreateDirectory(Path.Combine(profile, "logs"));
        File.WriteAllText(Path.Combine(profile, "logs", "vnidrop.log"),
            string.Concat(Enumerable.Repeat("🙂\t", 40000)) + "\nvnd1:private-test-ticket\nrecent-tail");
        var report = new BugReportSubmission().Prepare(Draft with { IncludeLogs = true }, EnvironmentDetails, profile, InstallId);
        Assert.InRange(report.Body.Length, 1, BugReportSubmission.MaxRequestBytes);
        using var body = JsonDocument.Parse(report.Body);
        var logs = body.RootElement.GetProperty("logs").GetString()!;
        Assert.DoesNotContain("private-test-ticket", logs);
        Assert.Contains("[redacted-ticket]", logs);
        Assert.DoesNotContain('\uFFFD', logs);
        Assert.EndsWith("recent-tail", logs);
    }

    [Fact]
    public void OversizedUnicodeDescriptionsFailBeforeSending() =>
        Assert.Throws<InvalidDataException>(() => new BugReportSubmission().Prepare(
            Draft with { WhatHappened = new string('é', 2001) }, EnvironmentDetails, profile, InstallId));

    [Fact]
    public async Task TransportPostsJsonAndRequiresTheMatchingAcknowledgement()
    {
        var report = new BugReportSubmission().Prepare(Draft, EnvironmentDetails, profile, InstallId);
        using var client = new HttpClient(new Handler(async (request, token) =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            Assert.Equal("https://example.test/base/v1/bugs", request.RequestUri!.AbsoluteUri);
            Assert.Equal("fixture", Assert.Single(request.Headers.GetValues("X-VniDrop-Key")));
            Assert.Equal(InstallId, Assert.Single(request.Headers.GetValues("X-VniDrop-Install-Id")));
            Assert.Equal("application/json", request.Content!.Headers.ContentType!.MediaType);
            Assert.Equal(report.Body.ToArray(), await request.Content.ReadAsByteArrayAsync(token));
            return JsonResponse(HttpStatusCode.Accepted, JsonSerializer.Serialize(new { ok = true, id = report.Id }));
        }));
        await new BugReportTransport(BugReportConfiguration.Create("https://example.test/base", "fixture")!, client).SendAsync(report);
    }

    [Theory]
    [InlineData("{}")]
    [InlineData("{\"ok\":true,\"id\":\"wrong\"}")]
    [InlineData("{\"ok\":false,\"id\":\"$id\"}")]
    [InlineData("{\"ok\":true,\"ok\":true,\"id\":\"$id\"}")]
    [InlineData("{\"ok\":true,\"id\":\"$id\"} trailing")]
    [InlineData("not-json")]
    public async Task SuccessfulHttpStatusDoesNotHideInvalidAcknowledgement(string acknowledgement)
    {
        var report = new BugReportSubmission().Prepare(Draft, EnvironmentDetails, profile, InstallId);
        using var client = new HttpClient(new Handler((_, _) => Task.FromResult(JsonResponse(HttpStatusCode.OK, acknowledgement.Replace("$id", report.Id)))));
        var error = await Assert.ThrowsAsync<InvalidDataException>(() => Transport(client).SendAsync(report));
        Assert.Equal("windows_report_unconfirmed", error.Message);
    }

    [Theory]
    [InlineData(401, "windows_report_service_configuration")]
    [InlineData(403, "windows_report_service_configuration")]
    [InlineData(429, "windows_report_rate_limited")]
    [InlineData(500, "bug_report_submit_failed")]
    [InlineData(302, "bug_report_submit_failed")]
    public async Task RejectionPreservesTheDraftForRetry(int status, string expectedError)
    {
        var submission = new BugReportSubmission();
        var report = submission.Prepare(Draft, EnvironmentDetails, profile, InstallId);
        using var client = new HttpClient(new Handler((_, _) => Task.FromResult(JsonResponse((HttpStatusCode)status, "{}"))));
        var error = await Assert.ThrowsAsync<InvalidDataException>(() => Transport(client).SendAsync(report));
        Assert.Equal(expectedError, error.Message);
        Assert.Same(report, submission.Prepare(Draft, EnvironmentDetails, profile, InstallId));
    }

    [Fact]
    public async Task OversizedAcknowledgementIsRejected()
    {
        var report = new BugReportSubmission().Prepare(Draft, EnvironmentDetails, profile, InstallId);
        using var client = new HttpClient(new Handler((_, _) => Task.FromResult(JsonResponse(HttpStatusCode.OK, new string(' ', 16385)))));
        await Assert.ThrowsAsync<InvalidDataException>(() => Transport(client).SendAsync(report));
    }

    [Fact]
    public async Task CancellationStopsTheInFlightRequest()
    {
        var report = new BugReportSubmission().Prepare(Draft, EnvironmentDetails, profile, InstallId);
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        using var client = new HttpClient(new Handler(async (_, token) =>
        {
            entered.SetResult();
            await Task.Delay(Timeout.InfiniteTimeSpan, token);
            throw new InvalidOperationException("Request was not cancelled");
        }));
        using var cancellation = new CancellationTokenSource();
        var sending = Transport(client).SendAsync(report, cancellation.Token);
        await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => sending);
    }

    private static BugReportTransport Transport(HttpClient client) => new(BugReportConfiguration.Create("https://example.test", "fixture")!, client);
    private static HttpResponseMessage JsonResponse(HttpStatusCode status, string body) => new(status) { Content = new StringContent(body, Encoding.UTF8, "application/json") };
    private sealed class Handler(Func<HttpRequestMessage, CancellationToken, Task<HttpResponseMessage>> send) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken) => send(request, cancellationToken);
    }
    public void Dispose() { if (Directory.Exists(profile)) Directory.Delete(profile, recursive: true); }
}
