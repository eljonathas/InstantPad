using System.IO;
using InstantPad.Application.Audio;
using InstantPad.Domain.Hotkeys;
using InstantPad.Domain.Sounds;
using InstantPad.Infrastructure.Persistence;
using Xunit;

namespace InstantPad.Tests.Infrastructure;

public sealed class PersistenceTests : IDisposable
{
    private readonly string _root = Path.Combine(Path.GetTempPath(), "InstantPadTests-" + Guid.NewGuid().ToString("N"));
    private readonly AppDataFolder _folder;

    public PersistenceTests() => _folder = new AppDataFolder(_root);

    public void Dispose() => Directory.Delete(_root, recursive: true);

    [Fact]
    public void TheBoardSurvivesASaveAndLoad()
    {
        var repository = new JsonBoardRepository(_folder);
        var board = repository.Load();
        var hotkey = new Hotkey(VirtualKeys.F1, HotkeyModifiers.Control);
        var sound = board.Place(board.Interfaces[0], 3, "a", "a.mp3", "#ff0000");
        sound.Hotkey = hotkey;

        repository.Save(board);
        var loaded = repository.Load();

        Assert.Equal(board.Interfaces[0].Id, loaded.Interfaces[0].Id);
        Assert.Equal(3, loaded.Sounds[0].Slot);
        Assert.Equal(hotkey, loaded.Sounds[0].Hotkey);
    }

    [Fact]
    public void ALibraryFromTheOldBanksIsMigratedOnLoad()
    {
        File.WriteAllText(_folder.File("library.json"), """[{ "Name": "a", "Path": "a.mp3", "Slot": 25 }]""");

        var board = new JsonBoardRepository(_folder).Load();

        Assert.Equal("Banco B", board.Interfaces[0].Name);
        Assert.Equal(1, board.Sounds[0].Slot);
    }

    [Fact]
    public void FavoritesSurviveASaveAndLoad()
    {
        var repository = new JsonFavoritesRepository(_folder);

        repository.Save([new SoundSource { Name = "x", Path = "y.mp3", SoundUrl = "z" }]);

        var favorite = Assert.Single(repository.Load());
        Assert.Equal("y.mp3", favorite.Path);
        Assert.Equal("z", favorite.SoundUrl);
    }

    [Fact]
    public void ADamagedFileIsKeptAsBackup()
    {
        var path = _folder.File("settings.json");
        File.WriteAllText(path, "{ quebrado");

        var settings = new JsonSettingsRepository(_folder).Load();

        Assert.Equal(1, settings.MicVolume);
        Assert.True(File.Exists(path + ".bak"));
    }

    [Fact]
    public void ExportingCopiesTheSoundToTheChosenPlace()
    {
        var store = new SoundFileStore(_folder);
        var stored = Path.Combine(_folder.Sounds, "a.mp3");
        var destination = _folder.File("baixado.mp3");
        File.WriteAllText(stored, "audio");

        store.Export(stored, destination);

        Assert.Equal("audio", File.ReadAllText(destination));
        Assert.True(File.Exists(stored));
    }

    [Fact]
    public void ExportingToAnInvalidPlaceReportsTheFailure()
    {
        var store = new SoundFileStore(_folder);
        var stored = Path.Combine(_folder.Sounds, "a.mp3");
        File.WriteAllText(stored, "audio");

        Assert.Throws<SoundUnavailableException>(() => store.Export(stored, _folder.File("pasta-que-nao-existe\\a.mp3")));
    }

    [Fact]
    public void DownloadsLandInsideTheStoreWithASafeName()
    {
        var path = new SoundFileStore(_folder).PathForDownload("https://www.myinstants.com/media/sounds/som%3Fa.mp3");

        Assert.Equal(_folder.Sounds, Path.GetDirectoryName(path));
        Assert.DoesNotContain('?', Path.GetFileName(path));
    }
}
