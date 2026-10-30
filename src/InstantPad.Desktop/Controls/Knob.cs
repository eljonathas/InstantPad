using System.Windows;
using System.Windows.Input;
using System.Windows.Media;

namespace InstantPad.Desktop.Controls;

/// <summary>Rotary knob: drag up and down or use the wheel, double click to reset.</summary>
public sealed class Knob : FrameworkElement
{
    private const double SweepDegrees = 270;
    private const double DragPixelsForFullRange = 180;

    public static readonly DependencyProperty ValueProperty = DependencyProperty.Register(
        nameof(Value), typeof(double), typeof(Knob),
        new FrameworkPropertyMetadata(0.0, FrameworkPropertyMetadataOptions.AffectsRender | FrameworkPropertyMetadataOptions.BindsTwoWayByDefault));

    public static readonly DependencyProperty AccentProperty = DependencyProperty.Register(
        nameof(Accent), typeof(Brush), typeof(Knob),
        new FrameworkPropertyMetadata(Brushes.Orange, FrameworkPropertyMetadataOptions.AffectsRender));

    public double Value
    {
        get => (double)GetValue(ValueProperty);
        set => SetValue(ValueProperty, Math.Clamp(value, Minimum, Maximum));
    }

    public Brush Accent
    {
        get => (Brush)GetValue(AccentProperty);
        set => SetValue(AccentProperty, value);
    }

    public double Minimum { get; set; }
    public double Maximum { get; set; } = 1;
    public double DefaultValue { get; set; } = 1;

    private double _dragStartY;
    private double _dragStartValue;


    public Knob() => Cursor = Cursors.SizeNS;

    protected override void OnMouseLeftButtonDown(MouseButtonEventArgs e)
    {
        if (e.ClickCount == 2)
        {
            Value = DefaultValue;
            return;
        }
        _dragStartY = e.GetPosition(this).Y;
        _dragStartValue = Value;
        CaptureMouse();
        e.Handled = true;
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (!IsMouseCaptured)
            return;
        double delta = (_dragStartY - e.GetPosition(this).Y) / DragPixelsForFullRange;
        Value = _dragStartValue + delta * (Maximum - Minimum);
    }

    protected override void OnMouseLeftButtonUp(MouseButtonEventArgs e) => ReleaseMouseCapture();

    protected override void OnMouseWheel(MouseWheelEventArgs e)
    {
        Value += Math.Sign(e.Delta) * (Maximum - Minimum) / 25;
        e.Handled = true;
    }

    protected override void OnRender(DrawingContext dc)
    {
        double size = Math.Min(ActualWidth, ActualHeight);
        var center = new Point(ActualWidth / 2, ActualHeight / 2);
        double radius = size / 2 - 9;
        double fraction = Maximum > Minimum ? (Value - Minimum) / (Maximum - Minimum) : 0;

        DrawTicks(dc, center, radius + 8, fraction);

        dc.DrawEllipse(new SolidColorBrush(Color.FromArgb(120, 0, 0, 0)), null,
            new Point(center.X, center.Y + 3), radius + 2, radius + 2);

        var skirt = new LinearGradientBrush(Color.FromRgb(0x4a, 0x4d, 0x55), Color.FromRgb(0x14, 0x15, 0x18), 90);
        dc.DrawEllipse(skirt, new Pen(Brushes.Black, 1), center, radius, radius);

        var cap = new RadialGradientBrush(Color.FromRgb(0x3c, 0x3f, 0x46), Color.FromRgb(0x1a, 0x1b, 0x1f))
        {
            GradientOrigin = new Point(0.35, 0.25),
        };
        dc.DrawEllipse(cap, new Pen(new SolidColorBrush(Color.FromRgb(0x08, 0x08, 0x0a)), 1), center, radius * 0.78, radius * 0.78);

        double angle = DegreesFor(fraction);
        var pointer = new Pen(Accent, 3) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round };
        dc.DrawLine(pointer, PointOn(center, radius * 0.34, angle), PointOn(center, radius * 0.74, angle));
    }

    private void DrawTicks(DrawingContext dc, Point center, double radius, double fraction)
    {
        const int ticks = 21;
        var lit = new Pen(Accent, 2);
        var unlit = new Pen(new SolidColorBrush(Color.FromRgb(0x4a, 0x4d, 0x55)), 1.5);
        for (int i = 0; i < ticks; i++)
        {
            double at = i / (double)(ticks - 1);
            double angle = DegreesFor(at);
            dc.DrawLine(at <= fraction ? lit : unlit, PointOn(center, radius - 3, angle), PointOn(center, radius, angle));
        }
    }

    // 0 sits at seven o'clock, the end of the sweep at five o'clock.
    private static double DegreesFor(double fraction) => 135 + SweepDegrees * fraction;

    private static Point PointOn(Point center, double radius, double degrees)
    {
        double radians = degrees * Math.PI / 180;
        return new Point(center.X + Math.Cos(radians) * radius, center.Y + Math.Sin(radians) * radius);
    }
}
