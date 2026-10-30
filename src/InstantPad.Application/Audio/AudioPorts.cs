namespace InstantPad.Application.Audio;

public sealed record AudioDevice(string Id, string Name)
{
    public override string ToString() => Name;
}

/// <summary>Plays sounds. Tags identify a playback so it can be stopped or queried later.</summary>
public interface ISoundPlayer
{
    /// <summary>Decodes ahead of time, so the first press of a pad has no delay.</summary>
    Task PreloadAsync(string path);

    /// <summary>Plays for the game and for you. Pressing again restarts the sound.</summary>
    Task PlayAsync(string path, float volume, object tag);

    /// <summary>Plays only for you, to try a sound out.</summary>
    Task PreviewAsync(string path, float volume, object tag);

    /// <summary>Drops the decoded audio, for when the file on disk has changed.</summary>
    void Forget(string path);

    void Stop(object tag);

    void StopAll();

    bool IsPlaying(object tag);
}

/// <summary>Chooses which physical devices the audio flows through.</summary>
public interface IAudioRouter
{
    IReadOnlyList<AudioDevice> ListMicrophones();

    IReadOnlyList<AudioDevice> ListOutputs();

    string? DefaultMicrophoneId();

    string? DefaultOutputId();

    /// <summary>Where Windows sends the sound of every other app.</summary>
    string? SystemOutputId();

    void SetMicrophone(string? deviceId);

    void SetGameOutput(string? deviceId);

    void SetMonitorOutput(string? deviceId);
}

/// <summary>Live mix controls and levels.</summary>
public interface IAudioMix
{
    double MicVolume { set; }

    double GameVolume { set; }

    double MonitorVolume { set; }

    bool MicEnabled { set; }

    float MicLevel { get; }

    float GameOutputLevel { get; }

    /// <summary>Raised on a background thread when a device stops or disappears.</summary>
    event Action<string>? DeviceFailed;
}

/// <summary>The virtual cable that carries the mix into the game.</summary>
public interface IVirtualCable
{
    AudioDevice? FindGameOutput(IEnumerable<AudioDevice> outputs);

    bool IsVirtual(AudioDevice device);

    /// <summary>The device the player must choose as microphone in the game.</summary>
    string GameMicrophoneName(AudioDevice? gameOutput);

    /// <summary>Downloads the driver and opens its setup, which asks the user for elevation.</summary>
    Task InstallAsync(IProgress<string> progress, CancellationToken token = default);
}
