using System.Windows;
using System.Windows.Media;

namespace InstantPad.Desktop.Controls;

/// <summary>Vertical LED ladder: green, then amber, then red at the top.</summary>
public sealed class LedMeter : FrameworkElement
{
    private const int Segments = 16;
    private const double Gap = 2;

    private static readonly Brush Green = Frozen(0x35, 0xf2, 0x7a);
    private static readonly Brush Amber = Frozen(0xff, 0xb0, 0x3a);
    private static readonly Brush Red = Frozen(0xff, 0x3b, 0x3b);
    private static readonly Brush Off = Frozen(0x1c, 0x1e, 0x22);

    public static readonly DependencyProperty LevelProperty = DependencyProperty.Register(
        nameof(Level), typeof(double), typeof(LedMeter),
        new FrameworkPropertyMetadata(0.0, FrameworkPropertyMetadataOptions.AffectsRender));

    public double Level
    {
        get => (double)GetValue(LevelProperty);
        set => SetValue(LevelProperty, value);
    }

    protected override void OnRender(DrawingContext dc)
    {
        double height = (ActualHeight - Gap * (Segments - 1)) / Segments;
        int lit = (int)Math.Ceiling(Math.Clamp(Level, 0, 1) * Segments);

        for (int i = 0; i < Segments; i++)
        {
            double y = ActualHeight - (i + 1) * height - i * Gap;
            dc.DrawRoundedRectangle(i < lit ? BrushFor(i) : Off, null, new Rect(0, y, ActualWidth, height), 1, 1);
        }
    }

    private static Brush BrushFor(int segment) =>
        segment >= Segments - 2 ? Red : segment >= Segments - 5 ? Amber : Green;

    private static SolidColorBrush Frozen(byte r, byte g, byte b)
    {
        var brush = new SolidColorBrush(Color.FromRgb(r, g, b));
        brush.Freeze();
        return brush;
    }
}
