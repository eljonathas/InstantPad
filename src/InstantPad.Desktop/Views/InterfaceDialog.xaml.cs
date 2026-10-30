using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using InstantPad.Desktop.Converters;
using InstantPad.Domain.Board;

namespace InstantPad.Desktop.Views;

/// <summary>Edits name, grid and color. A grid too small for the sounds cannot be saved.</summary>
public partial class InterfaceDialog : Window
{
    private readonly int _soundCount;
    private string _accent;

    public InterfaceDialog(string title, InterfaceLayout initial, int soundCount)
    {
        _soundCount = soundCount;
        _accent = initial.Accent;

        InitializeComponent();
        Title = title;

        NameBox.Text = initial.Name;
        NameBox.SelectAll();
        RowsBox.ItemsSource = Enumerable.Range(1, PadInterface.MaxRows).ToList();
        RowsBox.SelectedItem = initial.Rows;
        ColumnsBox.ItemsSource = Enumerable.Range(1, PadInterface.MaxColumns).ToList();
        ColumnsBox.SelectedItem = initial.Columns;

        foreach (var color in PadPalette.Accents)
            Swatches.Children.Add(CreateSwatch(color));
        PaintSwatches();

        Validate();
        Loaded += (_, _) => NameBox.Focus();
    }

    public InterfaceLayout? Result { get; private set; }

    private int Rows => RowsBox.SelectedItem as int? ?? 1;

    private int Columns => ColumnsBox.SelectedItem as int? ?? 1;

    private Button CreateSwatch(string color)
    {
        var swatch = new Button
        {
            Width = 30,
            Height = 30,
            Margin = new Thickness(0, 0, 6, 0),
            Tag = color,
            Background = new SolidColorBrush(WpfColor.Parse(color) ?? Colors.Gray),
            BorderThickness = new Thickness(3),
            ToolTip = color,
        };
        swatch.Click += (_, _) =>
        {
            _accent = color;
            PaintSwatches();
        };
        return swatch;
    }

    private void PaintSwatches()
    {
        foreach (Button swatch in Swatches.Children)
            swatch.BorderBrush = Equals(swatch.Tag, _accent) ? Brushes.White : Brushes.Transparent;
    }

    private void Field_Changed(object sender, RoutedEventArgs e) => Validate();

    private void Validate()
    {
        // Fires while the XAML is still loading.
        if (Summary == null || SaveButton == null)
            return;

        int capacity = Rows * Columns;
        bool fits = capacity >= _soundCount;

        Summary.Text = fits
            ? $"{capacity} pads"
            : $"{capacity} pads não comportam os {_soundCount} sons desta interface.";
        Summary.Foreground = fits ? Brushes.White : Brushes.OrangeRed;
        SaveButton.IsEnabled = fits && NameBox.Text.Trim().Length > 0;
    }

    private void Save_Click(object sender, RoutedEventArgs e)
    {
        Result = new InterfaceLayout(NameBox.Text.Trim(), Rows, Columns, _accent);
        DialogResult = true;
    }
}
