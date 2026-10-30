using System.Diagnostics;
using System.IO;
using System.Windows;
using InstantPad.Application.Hotkeys;
using InstantPad.Application.Localization;
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

    void OpenSoundSettings();
}

/// <summary>Opens the dialogs with global hotkeys off, since they would swallow what the user types.</summary>
public sealed class DialogService(HotkeyService hotkeys) : IDialogService
{
    private const string SoundSettingsUri = "ms-settings:sound";

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
        var dialog = new OpenFileDialog { Filter = Strings.AudioFileFilter };
        return dialog.ShowDialog(Owner) == true ? dialog.FileName : null;
    }

    public string? PickSaveLocation(string suggestedFileName) =>
        WithHotkeysPaused(() =>
        {
            var dialog = new SaveFileDialog
            {
                FileName = suggestedFileName,
                Filter = $"{Strings.AudioFileType}|*{Path.GetExtension(suggestedFileName)}",
                InitialDirectory = Environment.GetFolderPath(Environment.SpecialFolder.MyMusic),
            };
            return dialog.ShowDialog(Owner) == true ? dialog.FileName : null;
        });

    public void OpenSoundSettings() =>
        Process.Start(new ProcessStartInfo(SoundSettingsUri) { UseShellExecute = true });

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
