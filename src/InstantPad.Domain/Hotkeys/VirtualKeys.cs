namespace InstantPad.Domain.Hotkeys;

/// <summary>Windows virtual-key codes used by the app, and their display names.</summary>
public static class VirtualKeys
{
    public const int Digit0 = 0x30;
    public const int Digit9 = 0x39;
    public const int LetterA = 0x41;
    public const int LetterZ = 0x5A;
    public const int Numpad0 = 0x60;
    public const int Numpad9 = 0x69;
    public const int Multiply = 0x6A;
    public const int Add = 0x6B;
    public const int Subtract = 0x6D;
    public const int Decimal = 0x6E;
    public const int Divide = 0x6F;
    public const int F1 = 0x70;
    public const int F24 = 0x87;
    public const int OemPlus = 0xBB;
    public const int OemMinus = 0xBD;
    public const int OemPeriod = 0xBE;

    public static string NameOf(int key) => key switch
    {
        >= Digit0 and <= Digit9 or >= LetterA and <= LetterZ => ((char)key).ToString(),
        >= Numpad0 and <= Numpad9 => $"Num{key - Numpad0}",
        >= F1 and <= F24 => $"F{key - F1 + 1}",
        Multiply => "Num*",
        Add => "Num+",
        Subtract => "Num-",
        Decimal => "Num,",
        Divide => "Num/",
        OemPlus => "=",
        OemMinus => "-",
        OemPeriod => ".",
        _ => $"Tecla {key}",
    };
}
