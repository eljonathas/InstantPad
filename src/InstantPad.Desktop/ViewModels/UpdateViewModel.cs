using System.Windows.Input;
using InstantPad.Application.Localization;
using InstantPad.Application.Updates;

namespace InstantPad.Desktop.ViewModels;

/// <summary>Updates in the background and shows how it is going on the settings screen.</summary>
public sealed class UpdateViewModel : ObservableObject
{
    private readonly UpdateService _updates;
    private Func<string> _describeStatus = () => "";
    private bool _isReadyToRestart;

    public UpdateViewModel(UpdateService updates)
    {
        _updates = updates;
        RestartCommand = new RelayCommand(updates.RestartIntoNewVersion);
    }

    public ICommand RestartCommand { get; }

    public string Status => _describeStatus();

    public bool IsReadyToRestart { get => _isReadyToRestart; private set => Set(ref _isReadyToRestart, value); }

    public async Task CheckAndInstallAsync()
    {
        if (!_updates.IsReleaseBuild)
        {
            SetStatus(() => Strings.DevelopmentVersion);
            return;
        }

        var current = _updates.CurrentVersion.ToString(3);
        SetStatus(() => string.Format(Strings.Version, current));
        try
        {
            await _updates.CleanUpPreviousVersionAsync();

            if (await _updates.FindUpdateAsync() is not { } release)
                return;

            var version = release.Version.ToString(3);
            var progress = new Progress<int>(percent =>
                SetStatus(() => string.Format(Strings.DownloadingVersion, version, percent)));
            await _updates.InstallAsync(release, progress);

            SetStatus(() => string.Format(Strings.VersionInstalled, version));
            IsReadyToRestart = true;
        }
        catch (UpdateFailedException exception)
        {
            SetStatus(() => exception.Message);
        }
    }

    public void RefreshTexts() => Raise(nameof(Status));

    private void SetStatus(Func<string> describe)
    {
        _describeStatus = describe;
        Raise(nameof(Status));
    }
}
