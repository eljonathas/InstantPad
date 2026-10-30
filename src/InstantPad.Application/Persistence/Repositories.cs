using InstantPad.Application.Settings;
using InstantPad.Domain.Board;
using InstantPad.Domain.Sounds;

namespace InstantPad.Application.Persistence;

public interface IBoardRepository
{
    PadBoard Load();

    void Save(PadBoard board);
}

public interface IFavoritesRepository
{
    IReadOnlyList<SoundSource> Load();

    void Save(IEnumerable<SoundSource> favorites);
}

public interface ISettingsRepository
{
    AppSettings Load();

    void Save(AppSettings settings);
}
