using System.Runtime.InteropServices;
using InstantPad.Application.Audio;
using NAudio.CoreAudioApi;
using NAudio.Wave;
using NAudio.Wave.SampleProviders;

namespace InstantPad.Infrastructure.Audio;

/// <summary>
/// Routing:
///   mic + sounds -> virtual cable (what the game hears)
///   sounds       -> monitor device (what you hear)
/// </summary>
public sealed class AudioEngine : IAudioRouter, IAudioMix, IDisposable
{
    private const int OutputLatencyMs = 30;
    private const int CaptureBufferMs = 20;
    private const int CaptureBacklogMs = 500;
    private const int CaptureBlockMs = 100;

    private readonly MicRingBuffer _micBuffer = new();
    private readonly Mixer _gameMixer;
    private readonly Mixer _monitorMixer = new();
    private readonly float[] _captureBlock = new float[AudioFormat.SamplesFor(CaptureBlockMs)];

    private WasapiCapture? _capture;
    private BufferedWaveProvider? _captureBuffer;
    private ISampleProvider? _captureSource;
    private WasapiOut? _gameOutput;
    private WasapiOut? _monitorOutput;

    public AudioEngine() => _gameMixer = new Mixer(_micBuffer);

    public event Action<string>? DeviceFailed;

    public double MicVolume { set => _gameMixer.MicVolume = (float)value; }

    public double GameVolume { set => _gameMixer.SoundVolume = (float)value; }

    public double MonitorVolume { set => _monitorMixer.SoundVolume = (float)value; }

    public bool MicEnabled { set => _gameMixer.MicEnabled = value; }

    public float MicLevel { get; private set; }

    public float GameOutputLevel => _gameMixer.Peak;

    public IReadOnlyList<AudioDevice> ListMicrophones() => ListDevices(DataFlow.Capture);

    public IReadOnlyList<AudioDevice> ListOutputs() => ListDevices(DataFlow.Render);

    public string? DefaultMicrophoneId() => DefaultDeviceId(DataFlow.Capture);

    public string? DefaultOutputId() => DefaultDeviceId(DataFlow.Render);

    public void SetMicrophone(string? deviceId)
    {
        StopCapture();
        if (deviceId != null)
            OpeningDevice(() => StartCapture(deviceId));
    }

    public void SetGameOutput(string? deviceId) =>
        OpeningDevice(() => _gameOutput = RestartOutput(_gameOutput, deviceId, _gameMixer));

    public void SetMonitorOutput(string? deviceId) =>
        OpeningDevice(() => _monitorOutput = RestartOutput(_monitorOutput, deviceId, _monitorMixer));

    public void PlayForGame(SoundClip clip, float volume, object tag)
    {
        // A mixer with no device is never read, so its voices would never finish.
        if (_gameOutput != null)
            _gameMixer.Add(new Voice(clip, volume, tag));
    }

    public void PlayForMonitor(SoundClip clip, float volume, object tag)
    {
        if (_monitorOutput != null)
            _monitorMixer.Add(new Voice(clip, volume, tag));
    }

    public void Stop(object tag)
    {
        _gameMixer.Remove(tag);
        _monitorMixer.Remove(tag);
    }

    public void StopAll()
    {
        _gameMixer.Clear();
        _monitorMixer.Clear();
    }

    public bool IsPlaying(object tag) => _gameMixer.IsPlaying(tag) || _monitorMixer.IsPlaying(tag);

    public void Dispose()
    {
        StopCapture();
        DisposeOutput(_gameOutput);
        DisposeOutput(_monitorOutput);
    }

    private static List<AudioDevice> ListDevices(DataFlow flow)
    {
        using var enumerator = new MMDeviceEnumerator();
        return enumerator.EnumerateAudioEndPoints(flow, DeviceState.Active)
            .Select(device => new AudioDevice(device.ID, device.FriendlyName))
            .OrderBy(device => device.Name)
            .ToList();
    }

    private static string? DefaultDeviceId(DataFlow flow)
    {
        using var enumerator = new MMDeviceEnumerator();
        return enumerator.HasDefaultAudioEndpoint(flow, Role.Communications)
            ? enumerator.GetDefaultAudioEndpoint(flow, Role.Communications).ID
            : null;
    }

    private static void OpeningDevice(Action open)
    {
        try
        {
            open();
        }
        catch (Exception exception) when (exception is COMException or InvalidOperationException or ArgumentException)
        {
            throw new AudioDeviceException($"Dispositivo indisponível: {exception.Message}", exception);
        }
    }

    private void StartCapture(string deviceId)
    {
        using var enumerator = new MMDeviceEnumerator();
        var capture = new WasapiCapture(enumerator.GetDevice(deviceId), useEventSync: true, CaptureBufferMs);

        _captureBuffer = new BufferedWaveProvider(capture.WaveFormat)
        {
            DiscardOnBufferOverflow = true,
            ReadFully = false,
            BufferDuration = TimeSpan.FromMilliseconds(CaptureBacklogMs),
        };
        _captureSource = FormatConversion.ToEngineFormat(_captureBuffer.ToSampleProvider());
        _capture = capture;

        capture.DataAvailable += OnCaptureData;
        capture.RecordingStopped += (_, stopped) =>
        {
            if (stopped.Exception != null)
                DeviceFailed?.Invoke($"Microfone parou: {stopped.Exception.Message}");
        };
        capture.StartRecording();
    }

    private void OnCaptureData(object? sender, WaveInEventArgs captured)
    {
        var source = _captureSource;
        var buffer = _captureBuffer;
        if (source == null || buffer == null || captured.BytesRecorded == 0)
            return;

        buffer.AddSamples(captured.Buffer, 0, captured.BytesRecorded);

        float peak = 0f;
        int read;
        while ((read = source.Read(_captureBlock, 0, _captureBlock.Length)) > 0)
        {
            for (int index = 0; index < read; index++)
                peak = Math.Max(peak, Math.Abs(_captureBlock[index]));

            _micBuffer.Write(_captureBlock, read);
        }
        MicLevel = peak;
    }

    private void StopCapture()
    {
        var capture = _capture;
        _capture = null;
        _captureSource = null;
        _captureBuffer = null;

        if (capture != null)
        {
            capture.DataAvailable -= OnCaptureData;
            capture.StopRecording();
            capture.Dispose();
        }

        _micBuffer.Reset();
        MicLevel = 0f;
    }

    private WasapiOut? RestartOutput(WasapiOut? current, string? deviceId, Mixer mixer)
    {
        DisposeOutput(current);
        mixer.Clear();
        if (deviceId == null)
            return null;

        using var enumerator = new MMDeviceEnumerator();
        var device = enumerator.GetDevice(deviceId);

        ISampleProvider source = mixer;
        int deviceRate = device.AudioClient.MixFormat.SampleRate;
        if (deviceRate != AudioFormat.SampleRate)
            source = new WdlResamplingSampleProvider(source, deviceRate);

        var output = new WasapiOut(device, AudioClientShareMode.Shared, useEventSync: true, OutputLatencyMs);
        output.PlaybackStopped += (_, stopped) =>
        {
            if (stopped.Exception != null)
                DeviceFailed?.Invoke($"Saída de áudio parou: {stopped.Exception.Message}");
        };
        output.Init(source.ToWaveProvider());
        output.Play();
        return output;
    }

    private static void DisposeOutput(WasapiOut? output)
    {
        output?.Stop();
        output?.Dispose();
    }
}
