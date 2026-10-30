using System.Net;
using System.Net.Http;
using InstantPad.Application.Updates;
using InstantPad.Infrastructure.Updates;
using Xunit;

namespace InstantPad.Tests.Updates;

public sealed class GitHubReleaseSourceTests
{
    private const string LatestUrl = "https://api.github.com/repos/owner/repo/releases/latest";

    private const string ValidRelease = """
        {
          "tag_name": "v1.4.2",
          "draft": false,
          "assets": [
            { "name": "InstantPad.exe.sha256", "browser_download_url": "https://github.com/owner/repo/releases/download/v1.4.2/InstantPad.exe.sha256" },
            { "name": "InstantPad.exe", "browser_download_url": "https://github.com/owner/repo/releases/download/v1.4.2/InstantPad.exe" }
          ]
        }
        """;

    private static Task<ReleaseInfo?> GetLatest(StubHttpHandler handler) =>
        new GitHubReleaseSource(new HttpClient(handler), "owner", "repo").GetLatestAsync(CancellationToken.None);

    [Fact]
    public async Task ReadsVersionAndAssetsFromTheReleaseJson()
    {
        var release = await GetLatest(new StubHttpHandler().Respond(LatestUrl, ValidRelease));

        Assert.NotNull(release);
        Assert.Equal(new Version(1, 4, 2), release.Version);
        Assert.EndsWith("/v1.4.2/InstantPad.exe", release.ExecutableUrl.ToString());
        Assert.EndsWith("/v1.4.2/InstantPad.exe.sha256", release.ChecksumUrl.ToString());
    }

    [Fact]
    public async Task IdentifiesItselfToTheApi()
    {
        var handler = new StubHttpHandler().Respond(LatestUrl, ValidRelease);

        await GetLatest(handler);

        Assert.Contains("InstantPad", handler.Requests[0].Headers.UserAgent.ToString());
    }

    [Theory]
    [InlineData(HttpStatusCode.NotFound)]
    [InlineData(HttpStatusCode.Forbidden)]
    [InlineData(HttpStatusCode.TooManyRequests)]
    [InlineData(HttpStatusCode.InternalServerError)]
    public async Task ErrorStatusMeansNoRelease(HttpStatusCode status)
    {
        Assert.Null(await GetLatest(new StubHttpHandler().Respond(LatestUrl, "{}", status)));
    }

    [Fact]
    public async Task NetworkFailureMeansNoRelease()
    {
        Assert.Null(await GetLatest(new StubHttpHandler().Fail(LatestUrl)));
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("{}")]
    [InlineData("""{ "tag_name": "nightly", "assets": [] }""")]
    [InlineData("""{ "tag_name": "v1.0.0", "assets": [] }""")]
    [InlineData("""{ "tag_name": "v1.0.0", "assets": [ { "name": "InstantPad.exe", "browser_download_url": "https://x.test/a" } ] }""")]
    public async Task IncompleteReleaseMeansNoRelease(string json)
    {
        Assert.Null(await GetLatest(new StubHttpHandler().Respond(LatestUrl, json)));
    }

    [Fact]
    public async Task RefusesDownloadsThatAreNotHttps()
    {
        var json = ValidRelease.Replace("https://github.com", "http://github.com");

        Assert.Null(await GetLatest(new StubHttpHandler().Respond(LatestUrl, json)));
    }

    [Fact]
    public async Task CallerCancellationIsNotSwallowed()
    {
        var source = new GitHubReleaseSource(new HttpClient(new StubHttpHandler()), "owner", "repo");
        using var cancelled = new CancellationTokenSource();
        await cancelled.CancelAsync();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => source.GetLatestAsync(cancelled.Token));
    }
}
