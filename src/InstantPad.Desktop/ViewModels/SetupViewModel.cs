using System.Windows.Input;
using InstantPad.Application.Audio;
using InstantPad.Application.Localization;
using InstantPad.Application.Persistence;
using InstantPad.Application.Settings;
using InstantPad.Desktop.Localization;
using InstantPad.Desktop.Services;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Desktop.ViewModels;

public sealed record NumericOption(NumericScheme Scheme, string Label)
{
    public override string ToString() => Label;
}

public sealed record LanguageOption(string? Tag, string Label)
{
    public override string ToString() => Label;
}

/// <summary>The settings screen: language, driver, audio routing and numeric keys.</summary>
public sealed class SetupViewModel : ObservableObject
{
    private static readonly TimeSpan DeviceOpenTimeout = TimeSpan.FromSeconds(8);

    private readonly IAudioRouter _router;
    private readonly IVirtualCable _cable;
    private readonly ISettingsRepository _repository;
    private readonly AppSettings _settings;
    private readonly BoardViewModel _board;
    private readonly DisplayViewModel _display;
    private readonly IDialogService _dialogs;
    private readonly AudioRouting _routing;

    private IReadOnlyList<AudioDevice> _microphones = [];
    private IReadOnlyList<AudioDevice> _gameOutputs = [];
    private IReadOnlyList<AudioDevice> _monitorOutputs = [];
    private IReadOnlyList<NumericOption> _numericOptions = [];
    private IReadOnlyList<LanguageOption> _languageOptions = [];
    private AudioDevice? _microphone;
    private AudioDevice? _gameOutput;
    private AudioDevice? _monitorOutput;
    private NumericOption? _numericOption;
    private LanguageOption? _languageOption;
    private string _driverStatus = "";
    private string _gameInstruction = "";
    private bool _isDriverMissing;
    private bool _isSystemOutputOnCable;
    private bool _isInstallingDriver;
    private bool _isLoadingDevices;

    public SetupViewModel(
        IAudioRouter router,
        IVirtualCable cable,
        ISettingsRepository repository,
        AppSettings settings,
        BoardViewModel board,
        DisplayViewModel display,
        IDialogService dialogs)
    {
        _router = router;
        _cable = cable;
        _repository = repository;
        _settings = settings;
        _board = board;
        _display = display;
        _dialogs = dialogs;
        _routing = new AudioRouting(cable);

        RefreshDevicesCommand = new AsyncCommand(LoadDevicesAsync);
        InstallDriverCommand = new AsyncCommand(InstallDriverAsync);
        SetStopHotkeyCommand = new RelayCommand(SetStopHotkey);
        OpenSoundSettingsCommand = new RelayCommand(dialogs.OpenSoundSettings);

        BuildOptions();
    }

    public ICommand RefreshDevicesCommand { get; }

    public ICommand InstallDriverCommand { get; }

    public ICommand SetStopHotkeyCommand { get; }

    public ICommand OpenSoundSettingsCommand { get; }

    public IReadOnlyList<AudioDevice> Microphones { get => _microphones; private set => Set(ref _microphones, value); }

    public IReadOnlyList<AudioDevice> GameOutputs { get => _gameOutputs; private set => Set(ref _gameOutputs, value); }

    public IReadOnlyList<AudioDevice> MonitorOutputs { get => _monitorOutputs; private set => Set(ref _monitorOutputs, value); }

    public IReadOnlyList<NumericOption> NumericOptions { get => _numericOptions; private set => Set(ref _numericOptions, value); }

    public IReadOnlyList<LanguageOption> LanguageOptions { get => _languageOptions; private set => Set(ref _languageOptions, value); }

    public string DriverStatus { get => _driverStatus; private set => Set(ref _driverStatus, value); }

    public string GameInstruction { get => _gameInstruction; private set => Set(ref _gameInstruction, value); }

    public bool IsDriverMissing { get => _isDriverMissing; private set => Set(ref _isDriverMissing, value); }

    public bool IsSystemOutputOnCable { get => _isSystemOutputOnCable; private set => Set(ref _isSystemOutputOnCable, value); }

    public bool CanInstallDriver => !_isInstallingDriver;

    public AudioDevice? Microphone
    {
        get => _microphone;
        set => ChooseDevice(ref _microphone, value, id =>
        {
            _settings.MicrophoneId = id;
            _router.SetMicrophone(id);
        });
    }

    public AudioDevice? GameOutput
    {
        get => _gameOutput;
        set => ChooseDevice(ref _gameOutput, value, id =>
        {
            _settings.VirtualOutputId = id;
            _router.SetGameOutput(id);
        });
    }

    public AudioDevice? MonitorOutput
    {
        get => _monitorOutput;
        set => ChooseDevice(ref _monitorOutput, value, id =>
        {
            _settings.MonitorOutputId = id;
            _router.SetMonitorOutput(id);
        });
    }

    public NumericOption? NumericOption
    {
        get => _numericOption;
        set
        {
            // The list clears its selection while the options are rebuilt.
            if (value == null || value.Scheme == _settings.NumericScheme)
                return;

            _numericOption = value;
            _settings.NumericScheme = value.Scheme;
            _repository.Save(_settings);
            _board.ApplyHotkeys();
            Raise(nameof(NumericOption));
            Raise(nameof(NumericHint));
            Raise(nameof(StopHotkeyText));
        }
    }

    public LanguageOption? LanguageOption
    {
        get => _languageOption;
        set
        {
            if (value == null || value.Tag == _settings.Language)
                return;

            _languageOption = value;
            _settings.Language = value.Tag;
            _repository.Save(_settings);
            Localizer.Instance.Apply(value.Tag);
        }
    }

    public string NumericHint => DescribeNumericKeys(_settings.NumericScheme);

    public string StopHotkeyText => _board.StopHotkey?.ToString() ?? Strings.SetShortcut;

    public async Task LoadDevicesAsync()
    {
        LoadDeviceLists();
        UpdateRoutingStatus();

        // Separately, so one stuck device does not silence the others.
        await Task.WhenAll(
            OpenDeviceAsync(() => _router.SetMicrophone(Microphone?.Id)),
            OpenDeviceAsync(() => _router.SetGameOutput(GameOutput?.Id)),
            OpenDeviceAsync(() => _router.SetMonitorOutput(MonitorOutput?.Id)));
    }

    public void RefreshTexts()
    {
        BuildOptions();
        UpdateRoutingStatus();
        Raise(nameof(NumericHint));
        Raise(nameof(StopHotkeyText));
    }

    private static string DescribeNumericKeys(NumericScheme scheme)
    {
        if (scheme == NumericScheme.Off)
            return Strings.NumericHintOff;

        var layout = NumericShortcuts.LayoutFor(scheme);
        var keys = string.Format(
            Strings.NumericHint,
            layout.Pads[0], layout.Pads[^1], layout.NextInterface, layout.PreviousInterface, layout.StopAll);

        return scheme == NumericScheme.Numpad ? keys + Strings.NumericHintNumpad : keys;
    }

    private void BuildOptions()
    {
        NumericOptions =
        [
            new(NumericScheme.Numpad, Strings.NumericNumpad),
            new(NumericScheme.CtrlNumpad, Strings.NumericCtrlNumpad),
            new(NumericScheme.AltDigits, Strings.NumericAltDigits),
            new(NumericScheme.Off, Strings.NumericOff),
        ];
        _numericOption = NumericOptions.First(option => option.Scheme == _settings.NumericScheme);
        Raise(nameof(NumericOption));

        LanguageOptions =
        [
            new(null, Strings.LanguageSystem),
            new("pt", "Português"),
            new("en", "English"),
        ];
        _languageOption = LanguageOptions.First(option => option.Tag == _settings.Language);
        Raise(nameof(LanguageOption));
    }

    private void LoadDeviceLists()
    {
        _isLoadingDevices = true;
        try
        {
            var microphones = _router.ListMicrophones();
            var outputs = _router.ListOutputs();

            Microphones = _routing.MicrophonesFrom(microphones);
            GameOutputs = _routing.GameOutputsFrom(outputs);
            MonitorOutputs = _routing.MonitorOutputsFrom(outputs);

            var choice = _routing.Choose(
                microphones,
                outputs,
                new SavedRouting(_settings.MicrophoneId, _settings.VirtualOutputId, _settings.MonitorOutputId),
                new DefaultDevices(_router.DefaultMicrophoneId(), _router.DefaultOutputId()));

            Microphone = choice.Microphone;
            GameOutput = choice.GameOutput;
            MonitorOutput = choice.MonitorOutput;

            _settings.MicrophoneId = choice.Microphone?.Id;
            _settings.VirtualOutputId = choice.GameOutput?.Id;
            _settings.MonitorOutputId = choice.MonitorOutput?.Id;
        }
        finally
        {
            _isLoadingDevices = false;
        }
    }

    private void ChooseDevice(
        ref AudioDevice? field,
        AudioDevice? value,
        Action<string?> apply,
        [System.Runtime.CompilerServices.CallerMemberName] string? propertyName = null)
    {
        if (!Set(ref field, value, propertyName) || _isLoadingDevices)
            return;

        _ = OpenDeviceAsync(() => apply(value?.Id));
        UpdateRoutingStatus();
        _repository.Save(_settings);
    }

    /// <summary>A driver in a bad state can block forever, so this never runs on the UI thread.</summary>
    private async Task OpenDeviceAsync(Action open)
    {
        try
        {
            await Task.Run(open).WaitAsync(DeviceOpenTimeout);
        }
        catch (AudioDeviceException exception)
        {
            _display.ShowError(exception.Message);
        }
        catch (TimeoutException)
        {
            _display.ShowError(Strings.AudioDeviceNotResponding);
        }
    }

    private void UpdateRoutingStatus()
    {
        bool hasCable = _cable.FindGameOutput(_router.ListOutputs()) != null;
        var gameMicrophone = _cable.GameMicrophoneName(GameOutput);

        var outputs = _router.ListOutputs();
        var systemOutput = outputs.FirstOrDefault(device => device.Id == _router.SystemOutputId());

        IsSystemOutputOnCable = systemOutput != null && _cable.IsVirtual(systemOutput);
        IsDriverMissing = !hasCable;
        DriverStatus = hasCable ? Strings.DriverFound : Strings.DriverMissing;
        GameInstruction = string.Format(Strings.GameInstruction, gameMicrophone);

        const string absent = "--";
        var status = string.Format(
            Strings.RoutingStatus,
            Microphone != null ? Strings.Ok : absent,
            GameOutput != null ? Strings.Ok : absent,
            hasCable ? Strings.Ok : Strings.Missing);

        _display.ShowRouting(
            status,
            DescribeRouting(hasCable, gameMicrophone));
    }

    private string DescribeRouting(bool hasCable, string gameMicrophone)
    {
        if (!hasCable)
            return Strings.NoDriverHint;

        return IsSystemOutputOnCable
            ? Strings.SystemOutputOnCableHint
            : string.Format(Strings.GameMicrophoneHint, gameMicrophone);
    }

    private async Task InstallDriverAsync()
    {
        SetInstallingDriver(true);
        try
        {
            await _cable.InstallAsync(new Progress<string>(message => DriverStatus = message));
        }
        catch (DriverInstallCanceledException)
        {
            DriverStatus = Strings.DriverInstallCanceled;
        }
        catch (DriverInstallException exception)
        {
            DriverStatus = exception.Message;
        }
        finally
        {
            SetInstallingDriver(false);
        }
    }

    private void SetInstallingDriver(bool installing)
    {
        _isInstallingDriver = installing;
        Raise(nameof(CanInstallDriver));
    }

    private void SetStopHotkey()
    {
        if (_dialogs.AskHotkey() is not { } hotkey)
            return;

        _settings.StopHotkey = hotkey;
        _repository.Save(_settings);
        _board.ApplyHotkeys();
        Raise(nameof(StopHotkeyText));
    }
}
