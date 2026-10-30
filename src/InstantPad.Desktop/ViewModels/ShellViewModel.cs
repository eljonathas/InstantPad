using InstantPad.Application.Audio;
using InstantPad.Application.Hotkeys;
using InstantPad.Application.Localization;
using InstantPad.Application.Persistence;
using InstantPad.Application.Settings;
using InstantPad.Desktop.Localization;

namespace InstantPad.Desktop.ViewModels;

/// <summary>The whole device: joins the panels and runs what spans more than one of them.</summary>
public sealed class ShellViewModel
{
    private readonly AppSettings _settings;
    private readonly ISettingsRepository _settingsRepository;

    public ShellViewModel(
        DisplayViewModel display,
        MixerViewModel mixer,
        BoardViewModel board,
        BrowserViewModel browser,
        FavoritesViewModel favorites,
        SetupViewModel setup,
        UpdateViewModel update,
        HotkeyService hotkeys,
        IAudioMix mix,
        AppSettings settings,
        ISettingsRepository settingsRepository)
    {
        Display = display;
        Mixer = mixer;
        Board = board;
        Browser = browser;
        Favorites = favorites;
        Setup = setup;
        Update = update;
        Hotkeys = hotkeys;
        _settings = settings;
        _settingsRepository = settingsRepository;

        hotkeys.RegistrationFailed += failed =>
            display.ShowError(string.Format(Strings.HotkeyInUse, string.Join(", ", failed)));

        // Device failures arrive on an audio thread.
        var dispatcher = System.Windows.Threading.Dispatcher.CurrentDispatcher;
        Localizer.Instance.LanguageChanged += () => dispatcher.BeginInvoke(RefreshTexts);
        mix.DeviceFailed += message => dispatcher.BeginInvoke(() => display.ShowError(message));
    }

    public DisplayViewModel Display { get; }

    public MixerViewModel Mixer { get; }

    public BoardViewModel Board { get; }

    public BrowserViewModel Browser { get; }

    public FavoritesViewModel Favorites { get; }

    public SetupViewModel Setup { get; }

    public UpdateViewModel Update { get; }

    public HotkeyService Hotkeys { get; }

    public async Task StartAsync()
    {
        Board.Start();

        await Task.WhenAll(
            Setup.LoadDevicesAsync(),
            Browser.ShowTrendingAsync(),
            Update.CheckAndInstallAsync());
    }

    public void UpdateFrame()
    {
        Mixer.UpdateMeters();
        Board.UpdatePlayingState();
    }

    private void RefreshTexts()
    {
        Display.Show(Strings.Ready);
        Mixer.RefreshTexts();
        Browser.RefreshTexts();
        Favorites.RefreshTexts();
        Setup.RefreshTexts();
        Update.RefreshTexts();
    }

    public void SaveState()
    {
        _settingsRepository.Save(_settings);
        Board.Save();
    }
}
