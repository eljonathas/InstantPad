namespace InstantPad.Application.Updates;

/// <summary>A published version and where to download it from.</summary>
public sealed record ReleaseInfo(Version Version, Uri ExecutableUrl, Uri ChecksumUrl);
