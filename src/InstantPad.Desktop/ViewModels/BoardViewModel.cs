using System.Collections.ObjectModel;
using System.Windows.Input;
using InstantPad.Application.Audio;
using InstantPad.Application.Hotkeys;
using InstantPad.Application.Persistence;
using InstantPad.Application.Settings;
using InstantPad.Application.Sounds;
using InstantPad.Desktop.Services;
using InstantPad.Domain.Board;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Desktop.ViewModels;

/// <summary>The pads on screen, the interface tabs and what happens when they are used.</summary>
public sealed class BoardViewModel : ObservableObject, IHotkeyActions
{
    private readonly PadBoard _board;
    private readonly IBoardRepository _repository;
    private readonly ISoundPlayer _player;
    private readonly ISoundFileStore _files;
    private readonly FavoritesService _favorites;
    private readonly HotkeyService _hotkeys;
    private readonly AppSettings _settings;
    private readonly DisplayViewModel _display;
    private readonly IDialogService _dialogs;
    private readonly IAccentTheme _theme;

    private PadInterface _active;
    private InterfaceTabViewModel? _selectedTab;
    private PadViewModel? _targetPad;
    private int _padRows;
    private int _padColumns;

    public BoardViewModel(
        PadBoard board,
        IBoardRepository repository,
        ISoundPlayer player,
        ISoundFileStore files,
        FavoritesService favorites,
        HotkeyService hotkeys,
        AppSettings settings,
        DisplayViewModel display,
        IDialogService dialogs,
        IAccentTheme theme)
    {
        _board = board;
        _repository = repository;
        _player = player;
        _files = files;
        _favorites = favorites;
        _hotkeys = hotkeys;
        _settings = settings;
        _display = display;
        _dialogs = dialogs;
        _theme = theme;
        _active = board.Interfaces.FirstOrDefault(item => item.Id == settings.ActiveInterfaceId) ?? board.Interfaces[0];

        _favorites.Changed += RefreshPadLabels;

        NextInterfaceCommand = new RelayCommand(NextInterface);
        PreviousInterfaceCommand = new RelayCommand(PreviousInterface);
        AddInterfaceCommand = new RelayCommand(AddInterface);
        ConfigureInterfaceCommand = new RelayCommand(() => ConfigureInterface(_active));
        StopAllCommand = new RelayCommand(StopAll);
    }

    /// <summary>An empty pad was pressed and is waiting for a sound.</summary>
    public event Action? PadTargeted;

    public ObservableCollection<PadViewModel> Pads { get; } = [];

    public ObservableCollection<InterfaceTabViewModel> Tabs { get; } = [];

    public ICommand NextInterfaceCommand { get; }

    public ICommand PreviousInterfaceCommand { get; }

    public ICommand AddInterfaceCommand { get; }

    public ICommand ConfigureInterfaceCommand { get; }

    public ICommand StopAllCommand { get; }

    public PadInterface Active => _active;

    public IEnumerable<PadInterface> Interfaces => _board.Interfaces;

    public IEnumerable<PadInterface> OtherInterfaces => _board.Interfaces.Where(item => item != _active);

    public int PadRows { get => _padRows; private set => Set(ref _padRows, value); }

    public int PadColumns { get => _padColumns; private set => Set(ref _padColumns, value); }

    public Hotkey? StopHotkey => _settings.StopHotkey ?? NumericLayout.StopAll;

    public InterfaceTabViewModel? SelectedTab
    {
        get => _selectedTab;
        set
        {
            // The list clears its selection while the tabs are rebuilt.
            if (value == null || value == _selectedTab)
                return;

            _selectedTab = value;
            Raise(nameof(SelectedTab));
            if (value.Model != _active)
            {
                ShowInterface(value.Model);
                _display.Show(value.Model.Name);
            }
        }
    }

    private NumericLayout NumericLayout => NumericShortcuts.LayoutFor(_settings.NumericScheme);

    public void Start()
    {
        RebuildTabs();
        ShowInterface(_active);
        ApplyHotkeys();
        Save();

        foreach (var sound in _board.Sounds)
            _ = PreloadAsync(sound.Path);
    }

    public void Save() => _repository.Save(_board);

    /// <summary>Called on every UI frame to light the pads that are sounding.</summary>
    public void UpdatePlayingState()
    {
        foreach (var pad in Pads)
            pad.IsPlaying = pad.Sound != null && _player.IsPlaying(pad.Sound);
    }

    public void ApplyHotkeys()
    {
        _hotkeys.Apply(HotkeyService.Resolve(_board.Sounds, NumericLayout, _settings.StopHotkey, this));
        RefreshPadLabels();
    }

    // ---- Hotkey actions ------------------------------------------------

    public void PlaySound(Sound sound) => _ = TriggerAsync(sound);

    public void PlayPad(int padIndex)
    {
        if (padIndex < Pads.Count && Pads[padIndex].Sound is { } sound)
            PlaySound(sound);
        else
            _display.Show($"Pad {PadViewModel.LabelFor(padIndex)} vazio");
    }

    public void NextInterface() => StepInterface(1);

    public void PreviousInterface() => StepInterface(-1);

    public void StopAll()
    {
        _player.StopAll();
        _display.Show("Parado");
    }

    // ---- Interfaces ----------------------------------------------------

    public void ConfigureInterface(PadInterface target)
    {
        var layout = _dialogs.AskInterface("Configurar interface", target.ToLayout(), _board.SoundsOf(target).Count);
        if (layout == null)
            return;

        _board.Reconfigure(target, layout);
        Save();
        RebuildTabs();
        if (target == _active)
            ShowInterface(target);
    }

    public void MoveInterface(PadInterface target, int offset)
    {
        _board.MoveInterface(target, offset);
        Save();
        RebuildTabs();
    }

    public bool CanMoveInterface(PadInterface target, int offset)
    {
        int index = _board.Interfaces.ToList().IndexOf(target) + offset;
        return index >= 0 && index < _board.Interfaces.Count;
    }

    public bool CanDeleteInterface => _board.Interfaces.Count > 1;

    public void DeleteInterface(PadInterface target)
    {
        var sounds = _board.SoundsOf(target);
        var question = $"Excluir \"{target.Name}\" e tirar os {sounds.Count} sons dos pads? Os favoritos continuam salvos.";
        if (sounds.Count > 0 && !_dialogs.Confirm("Excluir interface", question))
            return;

        foreach (var sound in sounds)
            _player.Stop(sound);

        var next = _board.DeleteInterface(target);
        Save();
        RebuildTabs();
        if (target == _active)
            ShowInterface(next);
        ApplyHotkeys();
    }

    private void AddInterface()
    {
        int count = _board.Interfaces.Count;
        var initial = new InterfaceLayout($"Interface {count + 1}", 4, 6, PadPalette.AccentFor(count));
        var layout = _dialogs.AskInterface("Nova interface", initial, soundCount: 0);
        if (layout == null)
            return;

        var created = _board.AddInterface(layout);
        Save();
        RebuildTabs();
        ShowInterface(created);
        _display.Show(created.Name);
    }

    private void StepInterface(int offset)
    {
        ShowInterface(_board.Neighbor(_active, offset));
        _display.Show(_active.Name);
    }

    private void ShowInterface(PadInterface target)
    {
        _active = target;
        _settings.ActiveInterfaceId = target.Id;
        PadRows = target.Rows;
        PadColumns = target.Columns;
        _theme.Apply(target.Accent);

        ClearTarget();
        Pads.Clear();
        foreach (var slot in Enumerable.Range(0, target.Capacity))
            Pads.Add(new PadViewModel(target, slot, _board.SoundAt(target, slot), PressAsync));
        RefreshPadLabels();

        SelectTab(target);
    }

    private void RebuildTabs()
    {
        Tabs.Clear();
        foreach (var item in _board.Interfaces)
            Tabs.Add(new InterfaceTabViewModel(item));

        _selectedTab = null;
        SelectTab(_active);
    }

    private void SelectTab(PadInterface target)
    {
        var tab = Tabs.FirstOrDefault(item => item.Model == target);
        if (tab != _selectedTab)
        {
            _selectedTab = tab;
            Raise(nameof(SelectedTab));
        }
    }

    // ---- Pads ----------------------------------------------------------

    public PadViewModel? PadForNewSound() => _targetPad ?? Pads.FirstOrDefault(pad => !pad.HasSound);

    public async Task PlaceAsync(string name, string path, string color, PadViewModel pad)
    {
        _player.Forget(path);
        await _player.PreloadAsync(path);

        pad.Sound = _board.Place(pad.Owner, pad.Slot, name, path, color);
        ClearTarget();
        Save();
        RefreshPadLabels();
        _display.Show($"{pad.Label} {name}");
    }

    public async Task ImportFileAsync(string sourcePath, PadViewModel pad)
    {
        try
        {
            var path = _files.Import(sourcePath);
            var name = System.IO.Path.GetFileNameWithoutExtension(sourcePath);
            await PlaceAsync(name, path, PadPalette.PadColorFor(pad.Slot), pad);
        }
        catch (SoundUnavailableException exception)
        {
            _display.ShowError(exception.Message);
        }
    }

    public async Task PickFileAsync(PadViewModel pad)
    {
        if (_dialogs.PickAudioFile() is { } path)
            await ImportFileAsync(path, pad);
    }

    public void SwapPads(int fromSlot, PadViewModel to)
    {
        _board.Swap(_active, fromSlot, to.Slot);
        Save();
        ShowInterface(_active);
    }

    public void SendToInterface(PadViewModel pad, PadInterface target)
    {
        if (!_board.SendTo(pad.Sound!, target))
        {
            _display.ShowError($"A interface {target.Name} está cheia.");
            return;
        }

        pad.Sound = null;
        Save();
        RefreshPadLabels();
        _display.Show($"Enviado para {target.Name}");
    }

    public void Rename(PadViewModel pad)
    {
        var name = _dialogs.AskText("Nome do pad", pad.Sound!.Name);
        if (string.IsNullOrWhiteSpace(name))
            return;

        pad.Sound.Name = name.Trim();
        AfterEdit(pad);
    }

    public void SetVolume(PadViewModel pad, double volume)
    {
        pad.Sound!.Volume = volume;
        AfterEdit(pad);
    }

    public void SetColor(PadViewModel pad, string color)
    {
        pad.Sound!.Color = color;
        AfterEdit(pad);
    }

    public void SetHotkey(PadViewModel pad)
    {
        if (_dialogs.AskHotkey() is not { } hotkey)
            return;

        foreach (var other in _board.Sounds.Where(sound => hotkey.Equals(sound.Hotkey)))
            other.Hotkey = null;

        pad.Sound!.Hotkey = hotkey;
        AfterEdit(pad);
        ApplyHotkeys();
    }

    public void ClearHotkey(PadViewModel pad)
    {
        pad.Sound!.Hotkey = null;
        AfterEdit(pad);
        ApplyHotkeys();
    }

    public void Remove(PadViewModel pad)
    {
        _player.Stop(pad.Sound!);
        _board.Remove(pad.Sound!);
        pad.Sound = null;
        Save();
        ApplyHotkeys();
    }

    private async Task PressAsync(PadViewModel pad)
    {
        if (pad.Sound != null)
        {
            await TriggerAsync(pad.Sound);
            return;
        }

        ClearTarget();
        _targetPad = pad;
        pad.IsTarget = true;
        PadTargeted?.Invoke();
        _display.Show($"Pad {pad.Label} selecionado: escolha um som");
    }

    private async Task TriggerAsync(Sound sound)
    {
        try
        {
            await _player.PlayAsync(sound.Path, (float)sound.Volume, sound);
            _display.Show($"{PadViewModel.LabelFor(sound.Slot)} {sound.Name}");
        }
        catch (SoundUnavailableException exception)
        {
            _display.ShowError($"{sound.Name}: {exception.Message}");
        }
    }

    private async Task PreloadAsync(string path)
    {
        try
        {
            await _player.PreloadAsync(path);
        }
        catch (SoundUnavailableException)
        {
            // Only an optimization: the error shows up if the user presses the pad.
        }
    }

    private void AfterEdit(PadViewModel pad)
    {
        pad.Refresh();
        Save();
    }

    private void ClearTarget()
    {
        if (_targetPad != null)
            _targetPad.IsTarget = false;

        _targetPad = null;
    }

    private void RefreshPadLabels()
    {
        var numericKeys = NumericLayout.Pads;
        foreach (var pad in Pads)
        {
            pad.Shortcut = pad.Sound?.Hotkey?.ToString()
                ?? (pad.Slot < numericKeys.Count ? numericKeys[pad.Slot].ToString() : "");
            pad.IsFavorite = pad.Sound != null && _favorites.IsFavorite(pad.Sound.Path);
        }
    }
}
