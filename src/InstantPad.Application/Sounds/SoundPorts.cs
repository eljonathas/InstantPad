namespace InstantPad.Application.Sounds;

public sealed record CatalogSound(string Name, string SoundUrl, string Color);

public sealed record CatalogPage(IReadOnlyList<CatalogSound> Sounds, bool HasMore);

/// <summary>The online sound library.</summary>
public interface ISoundCatalog
{
    Task<CatalogPage> SearchAsync(string query, int page, CancellationToken token = default);

    Task<CatalogPage> TrendingAsync(int page, CancellationToken token = default);

    Task DownloadAsync(string soundUrl, string destination, CancellationToken token = default);
}

/// <summary>The folder where the app keeps its audio files.</summary>
public interface ISoundFileStore
{
    string PathForDownload(string soundUrl);

    /// <summary>Copies an outside file into the store and returns its new path.</summary>
    string Import(string sourcePath);

    /// <summary>Copies a stored sound to a place the user chose.</summary>
    void Export(string storedPath, string destination);

    bool Exists(string path);
}

/// <summary>The catalog could not be reached.</summary>
public sealed class CatalogUnavailableException(string message, Exception inner) : Exception(message, inner);
