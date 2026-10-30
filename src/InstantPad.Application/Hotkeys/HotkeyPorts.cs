using InstantPad.Domain.Board;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Application.Hotkeys;

/// <summary>Registers system-wide keys with the operating system.</summary>
public interface IHotkeyHost
{
    /// <summary>Returns false when another program already owns the key.</summary>
    bool Register(Hotkey hotkey, Action action);

    void Clear();
}

/// <summary>What the keys can do.</summary>
public interface IHotkeyActions
{
    void PlaySound(Sound sound);

    void PlayPad(int padIndex);

    void NextInterface();

    void PreviousInterface();

    void StopAll();
}
