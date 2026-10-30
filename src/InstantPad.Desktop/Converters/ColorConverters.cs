using System.Globalization;
using System.Windows.Data;
using System.Windows.Media;
using InstantPad.Domain.Board;

namespace InstantPad.Desktop.Converters;

public static class WpfColor
{
    /// <summary>Null for a color the site or a hand-edited file got wrong.</summary>
    public static Color? Parse(string? hex)
    {
        try
        {
            return string.IsNullOrEmpty(hex) ? null : (Color)ColorConverter.ConvertFromString(hex);
        }
        catch (FormatException)
        {
            return null;
        }
    }
}

public sealed class HexToBrushConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        WpfColor.Parse(value as string) is { } color ? new SolidColorBrush(color) : Brushes.Transparent;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

/// <summary>Picks readable text for a fully lit pad.</summary>
public sealed class ContrastBrushConverter : IValueConverter
{
    private const double LightColorLuminance = 0.55;

    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        HexColor.Luminance(value as string) > LightColorLuminance ? Brushes.Black : Brushes.White;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}
