using System.Net.Http;
using System.Reflection;
using InstantPad.Application.Hotkeys;
using InstantPad.Application.Sounds;
using InstantPad.Application.Updates;
using InstantPad.Desktop.Services;
using InstantPad.Desktop.ViewModels;
using InstantPad.Desktop.Views;
using InstantPad.Infrastructure.Audio;
using InstantPad.Infrastructure.Catalog;
using InstantPad.Infrastructure.Persistence;
using InstantPad.Infrastructure.Updates;
using InstantPad.Infrastructure.VirtualCable;

namespace InstantPad.Desktop;

/// <summary>The one place that knows which implementation goes behind each port.</summary>
public static class CompositionRoot
{
    private const string RepositoryOwner = "eljonathas";
    private const string RepositoryName = "InstantPad";
    private const string BrowserUserAgent =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

    public static MainWindow CreateMainWindow(IApplicationRestarter restarter)
    {
        var http = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
        http.DefaultRequestHeaders.UserAgent.ParseAdd(BrowserUserAgent);

        var folder = new AppDataFolder();
        var boardRepository = new JsonBoardRepository(folder);
        var favoritesRepository = new JsonFavoritesRepository(folder);
        var settingsRepository = new JsonSettingsRepository(folder);
        var soundFiles = new SoundFileStore(folder);

        var engine = new AudioEngine();
        var player = new ClipPlayer(engine);
        var catalog = new MyInstantsCatalog(http);
        var soundLibrary = new SoundLibrary(catalog, soundFiles);
        var favorites = new FavoritesService(favoritesRepository, soundLibrary);

        var hotkeyHost = new GlobalHotkeyHost();
        var hotkeys = new HotkeyService(hotkeyHost);
        var dialogs = new DialogService(hotkeys);
        var settings = settingsRepository.Load();

        var display = new DisplayViewModel();
        var mixer = new MixerViewModel(engine, settings);
        var board = new BoardViewModel(
            boardRepository.Load(), boardRepository, player, soundFiles, favorites,
            hotkeys, settings, display, dialogs, new AccentTheme());
        var rowActions = new SoundRowActions(board, soundLibrary, favorites, player, display);
        var browser = new BrowserViewModel(catalog, soundLibrary, favorites, rowActions);
        var favoritesPanel = new FavoritesViewModel(favorites, rowActions);
        var setup = new SetupViewModel(
            engine, new VbCable(http), settingsRepository, settings, board, display, dialogs);

        var updates = new UpdateService(
            new GitHubReleaseSource(http, RepositoryOwner, RepositoryName),
            new SelfReplacingInstaller(http, Environment.ProcessPath!),
            restarter,
            Assembly.GetEntryAssembly()!.GetName().Version!);

        var shell = new ShellViewModel(
            display, mixer, board, browser, favoritesPanel, setup, new UpdateViewModel(updates),
            hotkeys, engine, settings, settingsRepository);

        return new MainWindow(shell, rowActions, hotkeyHost, engine);
    }
}
