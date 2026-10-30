using System.Collections.ObjectModel;
using InstantPad.Application.Sounds;

namespace InstantPad.Desktop.ViewModels;

public sealed class FavoritesViewModel : ObservableObject
{
    private readonly FavoritesService _favorites;
    private readonly SoundRowActions _actions;
    private string _status = "";

    public FavoritesViewModel(FavoritesService favorites, SoundRowActions actions)
    {
        _favorites = favorites;
        _actions = actions;
        _favorites.Changed += Rebuild;
        Rebuild();
    }

    public ObservableCollection<SoundRowViewModel> Rows { get; } = [];

    public string Status { get => _status; private set => Set(ref _status, value); }

    private void Rebuild()
    {
        Rows.Clear();
        foreach (var favorite in _favorites.Favorites)
            Rows.Add(new SoundRowViewModel(favorite, isFavorite: true, _actions));

        Status = Rows.Count == 0
            ? "Nenhum favorito ainda. Use a estrela nos resultados ou o botão direito em um pad."
            : Rows.Count == 1 ? "1 favorito" : $"{Rows.Count} favoritos";
    }
}
