using System.Windows.Input;
using InstantPad.Application.Audio;
using InstantPad.Application.Localization;
using InstantPad.Application.Sounds;
using InstantPad.Desktop.Services;
using InstantPad.Domain.Sounds;

namespace InstantPad.Desktop.ViewModels;

/// <summary>A sound in a list: a MyInstants result or a favorite.</summary>
public sealed class SoundRowViewModel : ObservableObject
{
    private bool _isFavorite;

    public SoundRowViewModel(SoundSource source, bool isFavorite, SoundRowActions actions)
    {
        Source = source;
        _isFavorite = isFavorite;

        PreviewCommand = new AsyncCommand(() => actions.PreviewAsync(this));
        PlayCommand = new AsyncCommand(() => actions.PlayAsync(this));
        AddToPadCommand = new AsyncCommand(() => actions.AddToPadAsync(this));
        FavoriteCommand = new AsyncCommand(() => actions.ToggleFavoriteAsync(this));
        SaveToComputerCommand = new AsyncCommand(() => actions.SaveToComputerAsync(Source));
    }

    public SoundSource Source { get; }

    public ICommand PreviewCommand { get; }

    public ICommand PlayCommand { get; }

    public ICommand AddToPadCommand { get; }

    public ICommand FavoriteCommand { get; }

    public ICommand SaveToComputerCommand { get; }

    public string Name => Source.Name;

    public string Color => Source.Color;

    public string Star => IsFavorite ? "★" : "☆";

    public bool IsFavorite
    {
        get => _isFavorite;
        set
        {
            if (Set(ref _isFavorite, value))
                Raise(nameof(Star));
        }
    }
}

/// <summary>What the buttons of a sound row do, shared by the search results and the favorites.</summary>
public sealed class SoundRowActions(
    BoardViewModel board,
    SoundLibrary library,
    FavoritesService favorites,
    ISoundPlayer player,
    DisplayViewModel display,
    IDialogService dialogs)
{
    private const float FullVolume = 1f;

    public async Task SaveToComputerAsync(SoundSource source)
    {
        if (dialogs.PickSaveLocation(source.SuggestedFileName) is not { } destination)
            return;

        try
        {
            display.Show(string.Format(Strings.Downloading, source.Name));
            await library.SaveToComputerAsync(source, destination);
            display.Show(string.Format(Strings.Saved, System.IO.Path.GetFileName(destination)));
        }
        catch (Exception exception) when (exception is SoundUnavailableException or CatalogUnavailableException)
        {
            display.ShowError(string.Format(Strings.FailedSound, source.Name, exception.Message));
        }
    }

    public Task PreviewAsync(SoundRowViewModel row) =>
        DownloadingAsync(row, async () =>
        {
            await player.PreviewAsync(row.Source.Path, FullVolume, row.Source);
            display.Show(string.Format(Strings.Listening, row.Name));
        });

    public Task PlayAsync(SoundRowViewModel row) =>
        DownloadingAsync(row, async () =>
        {
            await player.PlayAsync(row.Source.Path, FullVolume, row.Source);
            display.Show(string.Format(Strings.InGameSound, row.Name));
        });

    public async Task AddToPadAsync(SoundRowViewModel row)
    {
        if (board.PadForNewSound() is { } pad)
            await AddToPadAsync(row, pad);
        else
            display.ShowError(Strings.InterfaceFullHint);
    }

    public Task AddToPadAsync(SoundRowViewModel row, PadViewModel pad) =>
        DownloadingAsync(row, () => board.PlaceAsync(row.Name, row.Source.Path, row.Color, pad));

    public Task ToggleFavoriteAsync(SoundRowViewModel row) => ToggleFavoriteAsync(row.Source);

    public async Task ToggleFavoriteAsync(SoundSource source)
    {
        try
        {
            display.Show(string.Format(Strings.Downloading, source.Name));
            bool isFavorite = await favorites.ToggleAsync(source);
            display.Show(isFavorite ? $"★ {source.Name}" : source.Name);
        }
        catch (Exception exception) when (exception is SoundUnavailableException or CatalogUnavailableException)
        {
            display.ShowError(string.Format(Strings.FailedSound, source.Name, exception.Message));
        }
    }

    private async Task DownloadingAsync(SoundRowViewModel row, Func<Task> action)
    {
        try
        {
            display.Show(string.Format(Strings.Downloading, row.Name));
            await library.EnsureDownloadedAsync(row.Source);
            await action();
        }
        catch (Exception exception) when (exception is SoundUnavailableException or CatalogUnavailableException)
        {
            display.ShowError(string.Format(Strings.FailedSound, row.Name, exception.Message));
        }
    }
}
