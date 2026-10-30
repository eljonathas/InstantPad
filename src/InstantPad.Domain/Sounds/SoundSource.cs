using InstantPad.Domain.Board;

namespace InstantPad.Domain.Sounds;

/// <summary>A sound that is not on a pad yet: a catalog result or a favorite.</summary>
public sealed class SoundSource
{
    public string Name { get; set; } = "";
    public string Color { get; set; } = PadPalette.Neutral;

    /// <summary>Where the audio file lives, or will live once downloaded. Also identifies the sound.</summary>
    public string Path { get; set; } = "";

    /// <summary>Null for files the user imported.</summary>
    public string? SoundUrl { get; set; }

    /// <summary>A readable file name for saving the sound outside the app.</summary>
    public string SuggestedFileName
    {
        get
        {
            var name = string.Concat(Name.Select(letter => System.IO.Path.GetInvalidFileNameChars().Contains(letter) ? '_' : letter));
            return name.Trim() + System.IO.Path.GetExtension(Path);
        }
    }

    public bool IsSameSoundAs(string path) => string.Equals(Path, path, StringComparison.OrdinalIgnoreCase);
}
