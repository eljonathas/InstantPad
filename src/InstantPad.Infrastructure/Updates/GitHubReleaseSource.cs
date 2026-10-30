using System.Net.Http;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Text.Json.Serialization;
using InstantPad.Application.Updates;

namespace InstantPad.Infrastructure.Updates;

/// <summary>Reads the latest published release through the public GitHub API.</summary>
public sealed class GitHubReleaseSource : IUpdateSource
{
    public const string ExecutableAsset = "InstantPad.exe";
    public const string ChecksumAsset = "InstantPad.exe.sha256";

    private const string UserAgent = "InstantPad";

    private readonly HttpClient _http;
    private readonly Uri _latestReleaseUrl;

    public GitHubReleaseSource(HttpClient http, string owner, string repository)
    {
        _http = http;
        _latestReleaseUrl = new Uri($"https://api.github.com/repos/{owner}/{repository}/releases/latest");
    }

    public async Task<ReleaseInfo?> GetLatestAsync(CancellationToken cancellationToken)
    {
        try
        {
            using var request = CreateRequest();
            using var response = await _http.SendAsync(request, cancellationToken);

            // 404 means nothing was published yet, 403 and 429 mean the hourly API limit.
            if (!response.IsSuccessStatusCode)
                return null;

            var json = await response.Content.ReadAsStringAsync(cancellationToken);
            return ToReleaseInfo(JsonSerializer.Deserialize<GitHubRelease>(json));
        }
        catch (HttpRequestException)
        {
            return null;
        }
        catch (TaskCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            return null;
        }
        catch (JsonException)
        {
            return null;
        }
    }

    private HttpRequestMessage CreateRequest()
    {
        var request = new HttpRequestMessage(HttpMethod.Get, _latestReleaseUrl);
        request.Headers.UserAgent.ParseAdd(UserAgent);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
        return request;
    }

    private static ReleaseInfo? ToReleaseInfo(GitHubRelease? release)
    {
        if (release == null || !ReleaseVersion.TryParse(release.TagName, out var version))
            return null;

        var executable = FindAssetUrl(release, ExecutableAsset);
        var checksum = FindAssetUrl(release, ChecksumAsset);
        return executable == null || checksum == null ? null : new ReleaseInfo(version, executable, checksum);
    }

    private static Uri? FindAssetUrl(GitHubRelease release, string assetName)
    {
        var address = release.Assets?.FirstOrDefault(asset => asset.Name == assetName)?.DownloadUrl;
        return Uri.TryCreate(address, UriKind.Absolute, out var url) && url.Scheme == Uri.UriSchemeHttps ? url : null;
    }

    private sealed record GitHubRelease(
        [property: JsonPropertyName("tag_name")] string? TagName,
        [property: JsonPropertyName("assets")] IReadOnlyList<GitHubAsset>? Assets);

    private sealed record GitHubAsset(
        [property: JsonPropertyName("name")] string? Name,
        [property: JsonPropertyName("browser_download_url")] string? DownloadUrl);
}
