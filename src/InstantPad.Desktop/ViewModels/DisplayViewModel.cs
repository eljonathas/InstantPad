using InstantPad.Application.Localization;

namespace InstantPad.Desktop.ViewModels;

/// <summary>The LCD at the top of the device.</summary>
public sealed class DisplayViewModel : ObservableObject
{
    private string _main = Strings.Ready;
    private string _status = "";
    private string _hint = "";

    public string Main { get => _main; private set => Set(ref _main, value); }

    public string Status { get => _status; private set => Set(ref _status, value); }

    public string Hint { get => _hint; private set => Set(ref _hint, value); }

    public void Show(string message) => Main = message;

    public void ShowError(string message)
    {
        Main = Strings.Error;
        Status = message;
    }

    public void ShowRouting(string status, string hint)
    {
        Status = status;
        Hint = hint;
    }
}
