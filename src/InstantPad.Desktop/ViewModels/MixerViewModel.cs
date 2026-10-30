using InstantPad.Application.Audio;
using InstantPad.Application.Settings;

namespace InstantPad.Desktop.ViewModels;

/// <summary>The knobs, the meters and the mic switch.</summary>
public sealed class MixerViewModel : ObservableObject
{
    private const double MeterDecay = 0.85;

    private readonly IAudioMix _mix;
    private readonly AppSettings _settings;
    private double _micPeak;
    private double _outputPeak;
    private double _micLevel;
    private double _outputLevel;

    public MixerViewModel(IAudioMix mix, AppSettings settings)
    {
        _mix = mix;
        _settings = settings;

        mix.MicVolume = settings.MicVolume;
        mix.GameVolume = settings.GameVolume;
        mix.MonitorVolume = settings.MonitorVolume;
        mix.MicEnabled = settings.MicEnabled;
    }

    public double MicVolume
    {
        get => _settings.MicVolume;
        set
        {
            _settings.MicVolume = value;
            _mix.MicVolume = value;
            Raise(nameof(MicVolume));
        }
    }

    public double GameVolume
    {
        get => _settings.GameVolume;
        set
        {
            _settings.GameVolume = value;
            _mix.GameVolume = value;
            Raise(nameof(GameVolume));
        }
    }

    public double MonitorVolume
    {
        get => _settings.MonitorVolume;
        set
        {
            _settings.MonitorVolume = value;
            _mix.MonitorVolume = value;
            Raise(nameof(MonitorVolume));
        }
    }

    public bool MicEnabled
    {
        get => _settings.MicEnabled;
        set
        {
            _settings.MicEnabled = value;
            _mix.MicEnabled = value;
            Raise(nameof(MicEnabled));
            Raise(nameof(MicLabel));
        }
    }

    public string MicLabel => MicEnabled ? "Mic aberto" : "Mic mudo";

    public double MicLevel { get => _micLevel; private set => Set(ref _micLevel, value); }

    public double OutputLevel { get => _outputLevel; private set => Set(ref _outputLevel, value); }

    /// <summary>Called on every UI frame. Peaks fall slowly, like the needle of a real meter.</summary>
    public void UpdateMeters()
    {
        _micPeak = Math.Max(_mix.MicLevel, _micPeak * MeterDecay);
        _outputPeak = Math.Max(_mix.GameOutputLevel, _outputPeak * MeterDecay);

        // The square root spreads quiet levels over more LEDs.
        MicLevel = Math.Sqrt(_micPeak);
        OutputLevel = Math.Sqrt(_outputPeak);
    }
}
