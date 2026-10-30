using InstantPad.Domain.Hotkeys;

namespace InstantPad.Domain.Board;

/// <summary>A sound assigned to a pad.</summary>
public sealed class Sound
{
    public string Id { get; set; } = Guid.NewGuid().ToString("N");
    public string Name { get; set; } = "";
    public string Path { get; set; } = "";
    public string Color { get; set; } = PadPalette.Neutral;
    public double Volume { get; set; } = 1;
    public Hotkey? Hotkey { get; set; }

    /// <summary>Null only in libraries saved before interfaces existed.</summary>
    public string? InterfaceId { get; set; }

    /// <summary>Position inside the interface, left to right, top to bottom.</summary>
    public int Slot { get; set; }
}
