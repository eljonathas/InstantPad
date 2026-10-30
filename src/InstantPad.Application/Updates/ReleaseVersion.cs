using System.Diagnostics.CodeAnalysis;

namespace InstantPad.Application.Updates;

/// <summary>Reads and compares the vX.Y.Z versions used by the release tags.</summary>
public static class ReleaseVersion
{
    public static bool TryParse(string? tag, [NotNullWhen(true)] out Version? version)
    {
        version = null;
        if (string.IsNullOrWhiteSpace(tag))
            return false;

        if (!Version.TryParse(tag.TrimStart('v', 'V'), out var parsed) || parsed.Build < 0)
            return false;

        version = Normalize(parsed);
        return true;
    }

    public static bool IsNewer(Version candidate, Version current) =>
        Normalize(candidate) > Normalize(current);

    // Assemblies report 1.2.3.0 while tags say 1.2.3, and Version treats those as different.
    private static Version Normalize(Version version) =>
        new(version.Major, version.Minor, Math.Max(version.Build, 0));
}
