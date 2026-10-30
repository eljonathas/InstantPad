using System.IO;

namespace InstantPad.Infrastructure.Persistence;

/// <summary>Where the app keeps its data. INSTANTPAD_DATA points it elsewhere for testing.</summary>
public sealed class AppDataFolder
{
    private const string OverrideVariable = "INSTANTPAD_DATA";

    public AppDataFolder()
        : this(Environment.GetEnvironmentVariable(OverrideVariable) is { Length: > 0 } custom
            ? custom
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "InstantPad"))
    {
    }

    public AppDataFolder(string root)
    {
        Root = root;
        Directory.CreateDirectory(Sounds);
    }

    public string Root { get; }

    public string Sounds => Path.Combine(Root, "sounds");

    public string File(string name) => Path.Combine(Root, name);
}
