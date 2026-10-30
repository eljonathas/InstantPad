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

    public bool IsSameSoundAs(string path) => string.Equals(Path, path, StringComparison.OrdinalIgnoreCase);
}
