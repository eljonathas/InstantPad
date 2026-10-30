using System.Windows.Input;
using InstantPad.Application.Audio;
using InstantPad.Application.Persistence;
using InstantPad.Application.Settings;
using InstantPad.Desktop.Services;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Desktop.ViewModels;

public sealed record NumericOption(NumericScheme Scheme, string Label)
{
    public override string ToString() => Label;
}

/// <summary>The setup screen: driver, audio routing and numeric keys.</summary>
public sealed class SetupViewModel : ObservableObject
{
    private const string CableOutputName = "CABLE Output";

    private readonly IAudioRouter _router;
    private readonly IVirtualCable _cable;
    private readonly ISettingsRepository _repository;
    private readonly AppSettings _settings;
    private readonly BoardViewModel _board;
    private readonly DisplayViewModel _display;
    private readonly IDialogService _dialogs;

    private IReadOnlyList<AudioDevice> _microphones = [];
    private IReadOnlyList<AudioDevice> _gameOutputs = [];
    private IReadOnlyList<AudioDevice> _monitorOutputs = [];
    private AudioDevice? _microphone;
    private AudioDevice? _gameOutput;
    private AudioDevice? _monitorOutput;
    private NumericOption _numericOption;
    private string _driverStatus = "";
    private string _gameInstruction = "";
    private bool _isDriverMissing;
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
        _numericOption = NumericOptions.First(option => option.Scheme == settings.NumericScheme);

        RefreshDevicesCommand = new RelayCommand(LoadDevices);
        InstallDriverCommand = new AsyncCommand(InstallDriverAsync);
        SetStopHotkeyCommand = new RelayCommand(SetStopHotkey);
    }

    public IReadOnlyList<NumericOption> NumericOptions { get; } =
    [
        new(NumericScheme.Numpad, "Teclado numérico (padrão)"),
        new(NumericScheme.CtrlNumpad, "Ctrl + teclado numérico"),
        new(NumericScheme.AltDigits, "Alt + 1 a 0"),
        new(NumericScheme.Off, "Desligado"),
    ];

    public ICommand RefreshDevicesCommand { get; }

    public ICommand InstallDriverCommand { get; }

    public ICommand SetStopHotkeyCommand { get; }

    public IReadOnlyList<AudioDevice> Microphones { get => _microphones; private set => Set(ref _microphones, value); }

    public IReadOnlyList<AudioDevice> GameOutputs { get => _gameOutputs; private set => Set(ref _gameOutputs, value); }

    public IReadOnlyList<AudioDevice> MonitorOutputs { get => _monitorOutputs; private set => Set(ref _monitorOutputs, value); }

    public string DriverStatus { get => _driverStatus; private set => Set(ref _driverStatus, value); }

    public string GameInstruction { get => _gameInstruction; private set => Set(ref _gameInstruction, value); }

    public bool IsDriverMissing { get => _isDriverMissing; private set => Set(ref _isDriverMissing, value); }

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

    public NumericOption NumericOption
    {
        get => _numericOption;
        set
        {
            if (!Set(ref _numericOption, value))
                return;

            _settings.NumericScheme = value.Scheme;
            _repository.Save(_settings);
            _board.ApplyHotkeys();
            Raise(nameof(NumericHint));
            Raise(nameof(StopHotkeyText));
        }
    }

    public string NumericHint => DescribeNumericKeys(_numericOption.Scheme);

    public string StopHotkeyText => _board.StopHotkey?.ToString() ?? "Definir atalho";

    public void LoadDevices()
    {
        LoadDeviceLists();

        Applying(() => _router.SetMicrophone(Microphone?.Id));
        Applying(() => _router.SetGameOutput(GameOutput?.Id));
        Applying(() => _router.SetMonitorOutput(MonitorOutput?.Id));
        UpdateRoutingStatus();
    }

    private static string DescribeNumericKeys(NumericScheme scheme)
    {
        if (scheme == NumericScheme.Off)
            return "Só valem os atalhos que você definir em cada pad.";

        var layout = NumericShortcuts.LayoutFor(scheme);
        var note = scheme == NumericScheme.Numpad
            ? " Deixe o NumLock ligado. Com o app aberto, essas teclas ficam reservadas para ele."
            : "";

        return $"{layout.Pads[0]} a {layout.Pads[^1]} tocam os 10 primeiros pads da interface ativa. " +
               $"{layout.NextInterface} e {layout.PreviousInterface} trocam de interface. {layout.StopAll} para tudo.{note}";
    }

    private void LoadDeviceLists()
    {
        _isLoadingDevices = true;
        try
        {
            var microphones = _router.ListMicrophones();
            var outputs = _router.ListOutputs();
            var defaultMicrophone = _router.DefaultMicrophoneId();
            var defaultOutput = _router.DefaultOutputId();

            Microphones = microphones.Where(device => !device.Name.Contains(CableOutputName, StringComparison.OrdinalIgnoreCase)).ToList();
            GameOutputs = outputs;
            MonitorOutputs = outputs.Where(device => !_cable.IsVirtual(device)).ToList();

            Microphone =
                microphones.FirstOrDefault(device => device.Id == _settings.MicrophoneId) ??
                microphones.FirstOrDefault(device => device.Id == defaultMicrophone);
            GameOutput =
                outputs.FirstOrDefault(device => device.Id == _settings.VirtualOutputId) ??
                _cable.FindGameOutput(outputs);
            MonitorOutput =
                outputs.FirstOrDefault(device => device.Id == _settings.MonitorOutputId) ??
                MonitorOutputs.FirstOrDefault(device => device.Id == defaultOutput) ??
                MonitorOutputs.FirstOrDefault();
        }
        finally
        {
            _isLoadingDevices = false;
        }
    }

    private void ChooseDevice(ref AudioDevice? field, AudioDevice? value, Action<string?> apply, [System.Runtime.CompilerServices.CallerMemberName] string? propertyName = null)
    {
        if (!Set(ref field, value, propertyName) || _isLoadingDevices)
            return;

        Applying(() => apply(value?.Id));
        UpdateRoutingStatus();
        _repository.Save(_settings);
    }

    private void Applying(Action apply)
    {
        try
        {
            apply();
        }
        catch (AudioDeviceException exception)
        {
            _display.ShowError(exception.Message);
        }
    }

    private void UpdateRoutingStatus()
    {
        bool hasCable = _cable.FindGameOutput(_router.ListOutputs()) != null;
        var gameMicrophone = _cable.GameMicrophoneName(GameOutput);

        IsDriverMissing = !hasCable;
        DriverStatus = hasCable
            ? "Driver encontrado."
            : "Nenhum cabo virtual encontrado. Sem ele os sons não chegam ao jogo.";
        GameInstruction = $"Abra as configurações de voz do jogo (ou Discord) e selecione \"{gameMicrophone}\" como microfone.";

        _display.ShowRouting(
            $"Mic {Mark(Microphone != null, "ok", "--")}   Saída {Mark(GameOutput != null, "ok", "--")}   Driver {Mark(hasCable, "ok", "ausente")}",
            hasCable ? $"No jogo, escolha o microfone: {gameMicrophone}" : "Sem driver virtual: abra Ajustes e instale");
    }

    private static string Mark(bool ok, string yes, string no) => ok ? yes : no;

    private async Task InstallDriverAsync()
    {
        SetInstallingDriver(true);
        try
        {
            await _cable.InstallAsync(new Progress<string>(message => DriverStatus = message));
        }
        catch (DriverInstallCanceledException)
        {
            DriverStatus = "Instalação cancelada.";
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
