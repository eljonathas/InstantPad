namespace InstantPad.Application.Updates;

public sealed class UpdateService(
    IUpdateSource source,
    IUpdateInstaller installer,
    IApplicationRestarter restarter,
    Version currentVersion)
{
    public Version CurrentVersion => currentVersion;

    /// <summary>Builds made outside the release pipeline report 0.0.0 and must not replace themselves.</summary>
    public bool IsReleaseBuild => currentVersion > new Version(0, 0, 0, 0);

    /// <summary>Returns the release to install, or null when the app is up to date.</summary>
    public async Task<ReleaseInfo?> FindUpdateAsync(CancellationToken cancellationToken = default)
    {
        var latest = await source.GetLatestAsync(cancellationToken);
        return latest != null && ReleaseVersion.IsNewer(latest.Version, currentVersion) ? latest : null;
    }

    /// <summary>
    /// Swaps the file on disk while the app keeps running, so an update never interrupts a game.
    /// The new version opens the next time the app starts.
    /// </summary>
    /// <exception cref="UpdateFailedException">The update failed and the installed version is intact.</exception>
    public Task InstallAsync(ReleaseInfo release, IProgress<int>? progress = null, CancellationToken cancellationToken = default) =>
        installer.InstallAsync(release, progress, cancellationToken);

    public void RestartIntoNewVersion() => restarter.Restart();

    public Task CleanUpPreviousVersionAsync(CancellationToken cancellationToken = default) =>
        installer.CleanUpPreviousVersionAsync(cancellationToken);
}
