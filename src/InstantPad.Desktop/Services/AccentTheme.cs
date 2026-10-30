using System.Windows.Media;
using InstantPad.Desktop.Converters;

namespace InstantPad.Desktop.Services;

public interface IAccentTheme
{
    void Apply(string hexColor);
}

/// <summary>Repaints everything that uses the Accent brush: logo, knobs and LEDs.</summary>
public sealed class AccentTheme : IAccentTheme
{
    private const string AccentResource = "Accent";

    public void Apply(string hexColor) =>
        System.Windows.Application.Current.Resources[AccentResource] =
            new SolidColorBrush(WpfColor.Parse(hexColor) ?? Colors.Orange);
}
