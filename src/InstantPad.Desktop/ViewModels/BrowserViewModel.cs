using System.Collections.ObjectModel;
using System.Windows.Input;
using InstantPad.Application.Localization;
using InstantPad.Application.Sounds;

namespace InstantPad.Desktop.ViewModels;

/// <summary>The MyInstants screen: search, trending sounds and paging.</summary>
public sealed class BrowserViewModel : ObservableObject
{
    private readonly ISoundCatalog _catalog;
    private readonly SoundLibrary _library;
    private readonly FavoritesService _favorites;
    private readonly SoundRowActions _actions;

    private string _query = "";
    private string? _activeQuery;
    private int _page = 1;
    private Func<string> _describeStatus = () => "";
    private bool _hasMore;

    public BrowserViewModel(
        ISoundCatalog catalog, SoundLibrary library, FavoritesService favorites, SoundRowActions actions)
    {
        _catalog = catalog;
        _library = library;
        _favorites = favorites;
        _actions = actions;
        _favorites.Changed += RefreshFavoriteMarks;

        SearchCommand = new AsyncCommand(SearchAsync);
        TrendingCommand = new AsyncCommand(ShowTrendingAsync);
        LoadMoreCommand = new AsyncCommand(LoadMoreAsync);
    }

    public ObservableCollection<SoundRowViewModel> Results { get; } = [];

    public ICommand SearchCommand { get; }

    public ICommand TrendingCommand { get; }

    public ICommand LoadMoreCommand { get; }

    public string Query { get => _query; set => Set(ref _query, value); }

    public string Status => _describeStatus();

    public bool HasMore { get => _hasMore; private set => Set(ref _hasMore, value); }

    public Task ShowTrendingAsync()
    {
        _activeQuery = null;
        _page = 1;
        return LoadPageAsync(reset: true);
    }

    public void RefreshTexts() => Raise(nameof(Status));

    private Task SearchAsync()
    {
        var query = Query.Trim();
        if (query.Length == 0)
            return Task.CompletedTask;

        _activeQuery = query;
        _page = 1;
        return LoadPageAsync(reset: true);
    }

    private Task LoadMoreAsync()
    {
        _page++;
        return LoadPageAsync(reset: false);
    }

    private async Task LoadPageAsync(bool reset)
    {
        SetStatus(() => Strings.Loading);
        HasMore = false;
        try
        {
            var page = _activeQuery is { } query
                ? await _catalog.SearchAsync(query, _page)
                : await _catalog.TrendingAsync(_page);

            if (reset)
                Results.Clear();
            foreach (var sound in page.Sounds)
                Results.Add(ToRow(sound));

            SetStatus(DescribeResults);
            HasMore = page.HasMore;
        }
        catch (CatalogUnavailableException)
        {
            SetStatus(() => Strings.NoCatalogConnection);
        }
    }

    private string DescribeResults() => Results.Count == 0
        ? Strings.NothingFound
        : $"{_activeQuery ?? Strings.TrendingTitle} ({Results.Count})";

    private void SetStatus(Func<string> describe)
    {
        _describeStatus = describe;
        Raise(nameof(Status));
    }

    private SoundRowViewModel ToRow(CatalogSound sound)
    {
        var source = _library.ToSource(sound);
        return new SoundRowViewModel(source, _favorites.IsFavorite(source.Path), _actions);
    }

    private void RefreshFavoriteMarks()
    {
        foreach (var row in Results)
            row.IsFavorite = _favorites.IsFavorite(row.Source.Path);
    }
}
