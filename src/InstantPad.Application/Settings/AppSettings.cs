using InstantPad.Domain.Hotkeys;

namespace InstantPad.Application.Settings;

public sealed class AppSettings
{
    private const double MaxMicVolume = 1;

    private double _micVolume = MaxMicVolume;

    public string? MicrophoneId { get; set; }
    public string? VirtualOutputId { get; set; }
    public string? MonitorOutputId { get; set; }

    /// <summary>USB mics already peak near full scale, so any boost distorts the voice.</summary>
    public double MicVolume
    {
        get => _micVolume;
        set => _micVolume = Math.Clamp(value, 0, MaxMicVolume);
    }

    public double GameVolume { get; set; } = 0.8;
    public double MonitorVolume { get; set; } = 0.6;
    public bool MicEnabled { get; set; } = true;
    public string? ActiveInterfaceId { get; set; }

    /// <summary>Language tag such as "en" or "pt". Null follows the system language.</summary>
    public string? Language { get; set; }

    public NumericScheme NumericScheme { get; set; } = NumericScheme.Numpad;

    /// <summary>Overrides the stop key of the numeric scheme when set.</summary>
    public Hotkey? StopHotkey { get; set; }
}
