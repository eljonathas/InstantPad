using System.Windows.Input;
using InstantPad.Domain.Board;

namespace InstantPad.Desktop.ViewModels;

/// <summary>One physical pad of the interface on screen.</summary>
public sealed class PadViewModel : ObservableObject
{
    private Sound? _sound;
    private bool _isPlaying;
    private bool _isTarget;
    private bool _isFavorite;
    private string _shortcut = "";

    public PadViewModel(PadInterface owner, int slot, Sound? sound, Func<PadViewModel, Task> press)
    {
        Owner = owner;
        Slot = slot;
        _sound = sound;
        PressCommand = new AsyncCommand(() => press(this));
    }

    public PadInterface Owner { get; }

    public int Slot { get; }

    public ICommand PressCommand { get; }

    public string Label => LabelFor(Slot);

    public bool HasSound => Sound != null;

    public string? Name => Sound?.Name;

    public string? Color => Sound?.Color;

    public Sound? Sound
    {
        get => _sound;
        set
        {
            _sound = value;
            Refresh();
        }
    }

    public bool IsPlaying { get => _isPlaying; set => Set(ref _isPlaying, value); }

    public bool IsTarget { get => _isTarget; set => Set(ref _isTarget, value); }

    public bool IsFavorite { get => _isFavorite; set => Set(ref _isFavorite, value); }

    public string Shortcut { get => _shortcut; set => Set(ref _shortcut, value); }

    public static string LabelFor(int slot) => $"{slot + 1:00}";

    /// <summary>The sound was edited: redraw what comes from it.</summary>
    public void Refresh()
    {
        Raise(nameof(Sound));
        Raise(nameof(HasSound));
        Raise(nameof(Name));
        Raise(nameof(Color));
    }
}

public sealed class InterfaceTabViewModel(PadInterface model) : ObservableObject
{
    public PadInterface Model { get; } = model;

    public string Name => Model.Name;
}
