using InstantPad.Application.Persistence;
using InstantPad.Domain.Sounds;

namespace InstantPad.Application.Sounds;

/// <summary>Favorites live apart from the pads, so they survive when a pad or interface is removed.</summary>
public sealed class FavoritesService
{
    private readonly IFavoritesRepository _repository;
    private readonly SoundLibrary _library;
    private readonly List<SoundSource> _favorites;

    public FavoritesService(IFavoritesRepository repository, SoundLibrary library)
    {
        _repository = repository;
        _library = library;
        _favorites = repository.Load().ToList();
    }

    public IReadOnlyList<SoundSource> Favorites => _favorites;

    public event Action? Changed;

    public bool IsFavorite(string path) => _favorites.Exists(favorite => favorite.IsSameSoundAs(path));

    /// <summary>Returns true when the sound is a favorite after the call.</summary>
    public async Task<bool> ToggleAsync(SoundSource source)
    {
        bool add = !IsFavorite(source.Path);
        if (add)
        {
            // Saved locally so a favorite survives the sound leaving the site.
            await _library.EnsureDownloadedAsync(source);
            _favorites.Add(source);
        }
        else
        {
            _favorites.RemoveAll(favorite => favorite.IsSameSoundAs(source.Path));
        }

        _repository.Save(_favorites);
        Changed?.Invoke();
        return add;
    }
}
