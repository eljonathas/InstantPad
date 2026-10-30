namespace InstantPad.Application.Updates;

public interface IUpdateInstaller
{
    /// <summary>Replaces the installed executable, reporting progress from 0 to 100.</summary>
    /// <exception cref="UpdateFailedException">The update failed and the installed version is intact.</exception>
    Task InstallAsync(ReleaseInfo release, IProgress<int>? progress, CancellationToken cancellationToken);

    /// <summary>Removes what the previous update left behind.</summary>
    Task CleanUpPreviousVersionAsync(CancellationToken cancellationToken);
}
