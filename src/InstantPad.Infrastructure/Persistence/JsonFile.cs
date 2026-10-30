using System.IO;
using System.Text.Json;

namespace InstantPad.Infrastructure.Persistence;

public static class JsonFile
{
    private static readonly JsonSerializerOptions Options = new()
    {
        WriteIndented = true,
        IgnoreReadOnlyProperties = true,
    };

    public static T Load<T>(string path) where T : new()
    {
        try
        {
            if (File.Exists(path))
                return JsonSerializer.Deserialize<T>(File.ReadAllText(path), Options) ?? new T();
        }
        catch (JsonException)
        {
            // Keep the damaged file for recovery instead of overwriting it on the next save.
            File.Copy(path, path + ".bak", overwrite: true);
        }
        return new T();
    }

    /// <summary>Writes through a temporary file, so a crash never leaves half a document.</summary>
    public static void Save<T>(string path, T value)
    {
        var partial = path + ".part";
        File.WriteAllText(partial, JsonSerializer.Serialize(value, Options));
        File.Move(partial, path, overwrite: true);
    }
}
