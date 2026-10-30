using InstantPad.Application.Localization;
using InstantPad.Application.Persistence;
using InstantPad.Application.Settings;
using InstantPad.Domain.Board;
using InstantPad.Domain.Sounds;

namespace InstantPad.Infrastructure.Persistence;

public sealed class JsonBoardRepository(AppDataFolder folder) : IBoardRepository
{
    private string InterfacesFile => folder.File("interfaces.json");

    private string SoundsFile => folder.File("library.json");

    public PadBoard Load() => new(
        JsonFile.Load<List<PadInterface>>(InterfacesFile),
        JsonFile.Load<List<Sound>>(SoundsFile),
        Strings.DefaultInterfaceName);

    public void Save(PadBoard board)
    {
        JsonFile.Save(InterfacesFile, board.Interfaces);
        JsonFile.Save(SoundsFile, board.Sounds);
    }
}

public sealed class JsonFavoritesRepository(AppDataFolder folder) : IFavoritesRepository
{
    private string FavoritesFile => folder.File("favorites.json");

    public IReadOnlyList<SoundSource> Load() => JsonFile.Load<List<SoundSource>>(FavoritesFile);

    public void Save(IEnumerable<SoundSource> favorites) => JsonFile.Save(FavoritesFile, favorites.ToList());
}

public sealed class JsonSettingsRepository(AppDataFolder folder) : ISettingsRepository
{
    private string SettingsFile => folder.File("settings.json");

    public AppSettings Load() => JsonFile.Load<AppSettings>(SettingsFile);

    public void Save(AppSettings settings) => JsonFile.Save(SettingsFile, settings);
}
