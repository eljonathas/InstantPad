using System.IO;
using System.Net.Http;
using System.Net;
using InstantPad.Application.Localization;
using InstantPad.Application.Sounds;

namespace InstantPad.Infrastructure.Catalog;

public sealed class MyInstantsCatalog(HttpClient http) : ISoundCatalog
{
    private const string BaseUrl = "https://www.myinstants.com";

    public Task<CatalogPage> SearchAsync(string query, int page, CancellationToken token = default) =>
        FetchPageAsync($"{BaseUrl}/en/search/?name={Uri.EscapeDataString(query)}&page={page}", page, token);

    public Task<CatalogPage> TrendingAsync(int page, CancellationToken token = default) =>
        FetchPageAsync($"{BaseUrl}/en/index/br/?page={page}", page, token);

    public async Task DownloadAsync(string soundUrl, string destination, CancellationToken token = default)
    {
        var partial = destination + ".part";
        try
        {
            await using (var source = await http.GetStreamAsync(soundUrl, token))
            await using (var target = File.Create(partial))
                await source.CopyToAsync(target, token);

            File.Move(partial, destination, overwrite: true);
        }
        catch (Exception exception) when (IsConnectionFailure(exception) || exception is IOException)
        {
            throw new CatalogUnavailableException(string.Format(Strings.SoundDownloadFailed, exception.Message), exception);
        }
    }

    private async Task<CatalogPage> FetchPageAsync(string url, int page, CancellationToken token)
    {
        try
        {
            using var response = await http.GetAsync(url, token);

            // Past the last page the site answers 404 rather than an empty list.
            if (response.StatusCode == HttpStatusCode.NotFound)
                return new CatalogPage([], HasMore: false);

            response.EnsureSuccessStatusCode();
            return MyInstantsPageParser.Parse(await response.Content.ReadAsStringAsync(token), BaseUrl, page);
        }
        catch (Exception exception) when (IsConnectionFailure(exception))
        {
            throw new CatalogUnavailableException(Strings.CatalogNoConnection, exception);
        }
    }

    private static bool IsConnectionFailure(Exception exception) =>
        exception is HttpRequestException or TaskCanceledException;
}
