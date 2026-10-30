using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Desktop.Views;

internal static class DialogWindow
{
    public static Window Create(Window owner, string title) => new()
    {
        Owner = owner,
        Title = title,
        Width = 380,
        SizeToContent = SizeToContent.Height,
        WindowStartupLocation = WindowStartupLocation.CenterOwner,
        ResizeMode = ResizeMode.NoResize,
        ShowInTaskbar = false,
        FontFamily = (FontFamily)owner.FindResource("Lexend"),
        Background = new SolidColorBrush(Color.FromRgb(0x17, 0x18, 0x1c)),
    };
}

public static class TextDialog
{
    public static string? Ask(Window owner, string title, string initial)
    {
        var dialog = DialogWindow.Create(owner, title);
        var box = new TextBox { Text = initial, Margin = new Thickness(14) };
        box.SelectAll();

        string? result = null;
        box.KeyDown += (_, pressed) =>
        {
            if (pressed.Key == Key.Enter)
                result = box.Text;

            if (pressed.Key is Key.Enter or Key.Escape)
                dialog.Close();
        };

        dialog.Content = box;
        dialog.Loaded += (_, _) => box.Focus();
        dialog.ShowDialog();
        return result;
    }
}

/// <summary>Waits for a key combination. Esc cancels.</summary>
public static class HotkeyDialog
{
    public static Hotkey? Ask(Window owner)
    {
        var dialog = DialogWindow.Create(owner, "Definir atalho");
        dialog.Content = new TextBlock
        {
            Text = "Pressione a tecla ou combinação.\nEsc cancela.",
            Foreground = Brushes.White,
            FontSize = 15,
            TextAlignment = TextAlignment.Center,
            Margin = new Thickness(20, 24, 20, 24),
        };

        Hotkey? result = null;
        dialog.PreviewKeyDown += (_, pressed) =>
        {
            pressed.Handled = true;
            var key = pressed.Key == Key.System ? pressed.SystemKey : pressed.Key;

            if (key == Key.Escape)
            {
                dialog.Close();
            }
            else if (!IsModifierKey(key))
            {
                result = new Hotkey(KeyInterop.VirtualKeyFromKey(key), (HotkeyModifiers)(int)Keyboard.Modifiers);
                dialog.Close();
            }
        };

        dialog.ShowDialog();
        return result;
    }

    private static bool IsModifierKey(Key key) =>
        key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt
            or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin;
}
