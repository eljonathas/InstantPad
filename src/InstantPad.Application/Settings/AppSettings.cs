using InstantPad.Domain.Hotkeys;

namespace InstantPad.Application.Settings;

public sealed class AppSettings
{
    public string? MicrophoneId { get; set; }
    public string? VirtualOutputId { get; set; }
    public string? MonitorOutputId { get; set; }
    public double MicVolume { get; set; } = 1;
    public double GameVolume { get; set; } = 0.8;
    public double MonitorVolume { get; set; } = 0.6;
    public bool MicEnabled { get; set; } = true;
    public string? ActiveInterfaceId { get; set; }
    public NumericScheme NumericScheme { get; set; } = NumericScheme.Numpad;

    /// <summary>Overrides the stop key of the numeric scheme when set.</summary>
    public Hotkey? StopHotkey { get; set; }
}
