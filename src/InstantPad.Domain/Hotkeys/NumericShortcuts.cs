namespace InstantPad.Domain.Hotkeys;

public enum NumericScheme
{
    Numpad,
    CtrlNumpad,
    AltDigits,
    Off,
}

/// <summary>Keys for the first pads of the active interface and for moving between interfaces.</summary>
public sealed record NumericLayout(
    IReadOnlyList<Hotkey> Pads,
    Hotkey? NextInterface,
    Hotkey? PreviousInterface,
    Hotkey? StopAll);

public static class NumericShortcuts
{
    private const int PadCount = 10;

    public static NumericLayout LayoutFor(NumericScheme scheme) => scheme switch
    {
        NumericScheme.Numpad => NumpadLayout(HotkeyModifiers.None),
        NumericScheme.CtrlNumpad => NumpadLayout(HotkeyModifiers.Control),
        NumericScheme.AltDigits => new NumericLayout(
            PadKeys(VirtualKeys.Digit0, HotkeyModifiers.Alt),
            new Hotkey(VirtualKeys.OemPlus, HotkeyModifiers.Alt),
            new Hotkey(VirtualKeys.OemMinus, HotkeyModifiers.Alt),
            new Hotkey(VirtualKeys.OemPeriod, HotkeyModifiers.Alt)),
        _ => new NumericLayout([], null, null, null),
    };

    private static NumericLayout NumpadLayout(HotkeyModifiers modifiers) => new(
        PadKeys(VirtualKeys.Numpad0, modifiers),
        new Hotkey(VirtualKeys.Add, modifiers),
        new Hotkey(VirtualKeys.Subtract, modifiers),
        new Hotkey(VirtualKeys.Decimal, modifiers));

    // Keyboard order: 1 to 9, then 0 as the tenth pad.
    private static List<Hotkey> PadKeys(int zeroKey, HotkeyModifiers modifiers) =>
        Enumerable.Range(1, PadCount)
            .Select(number => new Hotkey(zeroKey + number % PadCount, modifiers))
            .ToList();
}
