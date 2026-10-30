using System.Globalization;

namespace InstantPad.Domain.Board;

public static class HexColor
{
    /// <summary>Perceived brightness from 0 to 1. Colors that cannot be read count as black.</summary>
    public static double Luminance(string? hex)
    {
        if (!TryParse(hex, out var red, out var green, out var blue))
            return 0;

        return (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
    }

    private static bool TryParse(string? hex, out byte red, out byte green, out byte blue)
    {
        red = green = blue = 0;
        var digits = hex?.TrimStart('#') ?? "";

        // #RGB is shorthand for #RRGGBB, and the alpha of #AARRGGBB does not affect brightness.
        if (digits.Length == 3)
            digits = string.Concat(digits.Select(digit => $"{digit}{digit}"));
        else if (digits.Length == 8)
            digits = digits[2..];

        return digits.Length == 6
            && TryParseByte(digits[..2], out red)
            && TryParseByte(digits[2..4], out green)
            && TryParseByte(digits[4..], out blue);
    }

    private static bool TryParseByte(string text, out byte value) =>
        byte.TryParse(text, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out value);
}
