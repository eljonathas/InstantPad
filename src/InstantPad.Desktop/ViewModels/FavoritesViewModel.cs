using System.Collections.ObjectModel;
using InstantPad.Application.Localization;
using InstantPad.Application.Sounds;

namespace InstantPad.Desktop.ViewModels;

public sealed class FavoritesViewModel : ObservableObject
{
    private readonly FavoritesService _favorites;
    private readonly SoundRowActions _actions;

    public FavoritesViewModel(FavoritesService favorites, SoundRowActions actions)
    {
        _favorites = favorites;
        _actions = actions;
        _favorites.Changed += Rebuild;
        Rebuild();
    }

    public ObservableCollection<SoundRowViewModel> Rows { get; } = [];

    public string Status => Rows.Count switch
    {
        0 => Strings.FavoritesEmpty,
        1 => Strings.FavoritesOne,
        _ => string.Format(Strings.FavoritesCount, Rows.Count),
    };

    public void RefreshTexts() => Raise(nameof(Status));

    private void Rebuild()
    {
        Rows.Clear();
        foreach (var favorite in _favorites.Favorites)
            Rows.Add(new SoundRowViewModel(favorite, isFavorite: true, _actions));

        Raise(nameof(Status));
    }
}
