using System.IO;
using InstantPad.Application.Audio;
using InstantPad.Application.Localization;
using InstantPad.Application.Sounds;

namespace InstantPad.Infrastructure.Persistence;

public sealed class SoundFileStore(AppDataFolder folder) : ISoundFileStore
{
    public string PathForDownload(string soundUrl) =>
        InsideStore(Path.GetFileName(new Uri(soundUrl).LocalPath));

    public string Import(string sourcePath)
    {
        var destination = InsideStore(Path.GetFileName(sourcePath));
        try
        {
            if (!string.Equals(sourcePath, destination, StringComparison.OrdinalIgnoreCase))
                File.Copy(sourcePath, destination, overwrite: true);
            return destination;
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            throw new SoundUnavailableException(string.Format(Strings.SoundCopyFailed, exception.Message), exception);
        }
    }

    public void Export(string storedPath, string destination)
    {
        try
        {
            File.Copy(storedPath, destination, overwrite: true);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            throw new SoundUnavailableException(string.Format(Strings.SoundSaveFailed, exception.Message), exception);
        }
    }

    public bool Exists(string path) => File.Exists(path);

    private string InsideStore(string fileName)
    {
        foreach (var invalid in Path.GetInvalidFileNameChars())
            fileName = fileName.Replace(invalid, '_');

        return Path.Combine(folder.Sounds, fileName);
    }
}
