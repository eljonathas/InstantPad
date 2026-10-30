namespace InstantPad.Domain.Hotkeys;

/// <summary>Values match the MOD_* flags of the Windows hotkey API.</summary>
[Flags]
public enum HotkeyModifiers
{
    None = 0,
    Alt = 1,
    Control = 2,
    Shift = 4,
    Windows = 8,
}

public sealed record Hotkey(int VirtualKey, HotkeyModifiers Modifiers = HotkeyModifiers.None)
{
    public override string ToString()
    {
        var parts = new List<string>();
        if (Modifiers.HasFlag(HotkeyModifiers.Control)) parts.Add("Ctrl");
        if (Modifiers.HasFlag(HotkeyModifiers.Alt)) parts.Add("Alt");
        if (Modifiers.HasFlag(HotkeyModifiers.Shift)) parts.Add("Shift");
        if (Modifiers.HasFlag(HotkeyModifiers.Windows)) parts.Add("Win");
        parts.Add(VirtualKeys.NameOf(VirtualKey));
        return string.Join("+", parts);
    }
}
