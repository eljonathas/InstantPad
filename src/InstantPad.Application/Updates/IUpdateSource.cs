namespace InstantPad.Application.Updates;

public interface IUpdateSource
{
    /// <summary>Returns null when no release can be read right now. An update check never breaks the app.</summary>
    Task<ReleaseInfo?> GetLatestAsync(CancellationToken cancellationToken);
}
