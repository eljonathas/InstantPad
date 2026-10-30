using System.Windows.Input;
using InstantPad.Application.Updates;

namespace InstantPad.Desktop.ViewModels;

/// <summary>Updates in the background and shows how it is going on the setup screen.</summary>
public sealed class UpdateViewModel : ObservableObject
{
    private readonly UpdateService _updates;
    private string _status = "";
    private bool _isReadyToRestart;

    public UpdateViewModel(UpdateService updates)
    {
        _updates = updates;
        RestartCommand = new RelayCommand(updates.RestartIntoNewVersion);
    }

    public ICommand RestartCommand { get; }

    public string Status { get => _status; private set => Set(ref _status, value); }

    public bool IsReadyToRestart { get => _isReadyToRestart; private set => Set(ref _isReadyToRestart, value); }

    public async Task CheckAndInstallAsync()
    {
        if (!_updates.IsReleaseBuild)
        {
            Status = "Versão de desenvolvimento";
            return;
        }

        var current = _updates.CurrentVersion.ToString(3);
        Status = $"Versão {current}";
        try
        {
            await _updates.CleanUpPreviousVersionAsync();

            if (await _updates.FindUpdateAsync() is not { } release)
                return;

            var version = release.Version.ToString(3);
            var progress = new Progress<int>(percent => Status = $"Baixando a versão {version}: {percent}%");
            await _updates.InstallAsync(release, progress);

            Status = $"Versão {version} instalada. Ela abre na próxima vez.";
            IsReadyToRestart = true;
        }
        catch (UpdateFailedException exception)
        {
            Status = exception.Message;
        }
    }
}
