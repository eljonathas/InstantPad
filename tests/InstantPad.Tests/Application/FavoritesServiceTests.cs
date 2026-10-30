using InstantPad.Application.Audio;
using InstantPad.Application.Persistence;
using InstantPad.Application.Sounds;
using InstantPad.Domain.Sounds;
using Xunit;

namespace InstantPad.Tests.Application;

public sealed class FavoritesServiceTests
{
    private readonly FakeRepository _repository = new();
    private readonly FakeCatalog _catalog = new();

    private FavoritesService CreateService() => new(_repository, new SoundLibrary(_catalog, new FakeFiles()));

    private static SoundSource OnlineSound() =>
        new() { Name = "a", Path = "a.mp3", SoundUrl = "https://example.test/a.mp3" };

    [Fact]
    public async Task FavoritingDownloadsAndSaves()
    {
        var service = CreateService();

        Assert.True(await service.ToggleAsync(OnlineSound()));

        Assert.Equal("a.mp3", _catalog.Downloaded);
        Assert.True(service.IsFavorite("A.MP3"));
        Assert.Single(_repository.Saved);
    }

    [Fact]
    public async Task TogglingAgainRemoves()
    {
        var service = CreateService();
        await service.ToggleAsync(OnlineSound());

        Assert.False(await service.ToggleAsync(OnlineSound()));

        Assert.Empty(service.Favorites);
        Assert.Empty(_repository.Saved);
    }

    [Fact]
    public async Task ASoundThatCannotBeFetchedIsNotFavorited()
    {
        var service = CreateService();
        var missing = new SoundSource { Name = "a", Path = "sumiu.mp3" };

        await Assert.ThrowsAsync<SoundUnavailableException>(() => service.ToggleAsync(missing));

        Assert.Empty(service.Favorites);
    }

    [Fact]
    public async Task ChangesAreAnnounced()
    {
        var service = CreateService();
        int changes = 0;
        service.Changed += () => changes++;

        await service.ToggleAsync(OnlineSound());

        Assert.Equal(1, changes);
    }

    private sealed class FakeRepository : IFavoritesRepository
    {
        public List<SoundSource> Saved { get; private set; } = [];

        public IReadOnlyList<SoundSource> Load() => Saved;

        public void Save(IEnumerable<SoundSource> favorites) => Saved = favorites.ToList();
    }

    private sealed class FakeCatalog : ISoundCatalog
    {
        public string? Downloaded { get; private set; }

        public Task<CatalogPage> SearchAsync(string query, int page, CancellationToken token) =>
            throw new NotSupportedException();

        public Task<CatalogPage> TrendingAsync(int page, CancellationToken token) =>
            throw new NotSupportedException();

        public Task DownloadAsync(string soundUrl, string destination, CancellationToken token)
        {
            Downloaded = destination;
            return Task.CompletedTask;
        }
    }

    private sealed class FakeFiles : ISoundFileStore
    {
        public string PathForDownload(string soundUrl) => soundUrl;

        public string Import(string sourcePath) => sourcePath;

        public bool Exists(string path) => false;
    }
}
