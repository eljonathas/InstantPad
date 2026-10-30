using System.IO;
using System.Windows;
using InstantPad.Application.Hotkeys;
using InstantPad.Desktop.Views;
using InstantPad.Domain.Board;
using InstantPad.Domain.Hotkeys;
using Microsoft.Win32;

namespace InstantPad.Desktop.Services;

public interface IDialogService
{
    InterfaceLayout? AskInterface(string title, InterfaceLayout initial, int soundCount);

    string? AskText(string title, string initial);

    Hotkey? AskHotkey();

    bool Confirm(string title, string message);

    string? PickAudioFile();

    string? PickSaveLocation(string suggestedFileName);
}

/// <summary>Opens the dialogs with global hotkeys off, since they would swallow what the user types.</summary>
public sealed class DialogService(HotkeyService hotkeys) : IDialogService
{
    private const string AudioFileFilter = "Áudio|*.mp3;*.wav;*.m4a;*.aac;*.wma;*.flac|Todos os arquivos|*.*";

    private static Window Owner => System.Windows.Application.Current.MainWindow!;

    public InterfaceLayout? AskInterface(string title, InterfaceLayout initial, int soundCount) =>
        WithHotkeysPaused(() =>
        {
            var dialog = new InterfaceDialog(title, initial, soundCount) { Owner = Owner };
            return dialog.ShowDialog() == true ? dialog.Result : null;
        });

    public string? AskText(string title, string initial) =>
        WithHotkeysPaused(() => TextDialog.Ask(Owner, title, initial));

    public Hotkey? AskHotkey() =>
        WithHotkeysPaused(() => HotkeyDialog.Ask(Owner));

    public bool Confirm(string title, string message) =>
        WithHotkeysPaused(() =>
            MessageBox.Show(Owner, message, title, MessageBoxButton.YesNo, MessageBoxImage.Warning) == MessageBoxResult.Yes);

    public string? PickAudioFile()
    {
        var dialog = new OpenFileDialog { Filter = AudioFileFilter };
        return dialog.ShowDialog(Owner) == true ? dialog.FileName : null;
    }

    public string? PickSaveLocation(string suggestedFileName) =>
        WithHotkeysPaused(() =>
        {
            var dialog = new SaveFileDialog
            {
                FileName = suggestedFileName,
                Filter = "Áudio|*" + Path.GetExtension(suggestedFileName),
                InitialDirectory = Environment.GetFolderPath(Environment.SpecialFolder.MyMusic),
            };
            return dialog.ShowDialog(Owner) == true ? dialog.FileName : null;
        });

    private T WithHotkeysPaused<T>(Func<T> showDialog)
    {
        hotkeys.Pause();
        try
        {
            return showDialog();
        }
        finally
        {
            hotkeys.Resume();
        }
    }
}
