using NAudio.Wave;

namespace InstantPad.Infrastructure.Audio;

/// <summary>One playback of a clip on one mixer.</summary>
public sealed class Voice(SoundClip clip, float volume, object tag)
{
    private readonly float[] _samples = clip.Samples;
    private int _position;

    public object Tag { get; } = tag;

    public bool Finished => _position >= _samples.Length;

    public void MixInto(float[] buffer, int offset, int count)
    {
        int available = Math.Min(count, _samples.Length - _position);
        for (int index = 0; index < available; index++)
            buffer[offset + index] += _samples[_position + index] * volume;

        _position += available;
    }
}

/// <summary>Sums the voices and an optional live input, applying gain and a soft limiter.</summary>
public sealed class Mixer(MicRingBuffer? mic = null) : ISampleProvider
{
    private const float LimiterKnee = 0.8f;

    private readonly object _lock = new();
    private readonly List<Voice> _voices = [];
    private float[] _micBuffer = [];

    public WaveFormat WaveFormat { get; } =
        WaveFormat.CreateIeeeFloatWaveFormat(AudioFormat.SampleRate, AudioFormat.Channels);

    /// <summary>Gain applied to sounds, 0 to 1.</summary>
    public float SoundVolume { get; set; } = 1f;

    /// <summary>Gain applied to the live input, 0 to 1.</summary>
    public float MicVolume { get; set; } = 1f;

    public bool MicEnabled { get; set; } = true;

    /// <summary>Peak of the most recent block, for the meters.</summary>
    public float Peak { get; private set; }

    public void Add(Voice voice)
    {
        lock (_lock)
            _voices.Add(voice);
    }

    public void Clear()
    {
        lock (_lock)
            _voices.Clear();
    }

    public void Remove(object tag)
    {
        lock (_lock)
            _voices.RemoveAll(voice => Equals(voice.Tag, tag));
    }

    public bool IsPlaying(object tag)
    {
        lock (_lock)
            return _voices.Exists(voice => Equals(voice.Tag, tag));
    }

    public int Read(float[] buffer, int offset, int count)
    {
        Array.Clear(buffer, offset, count);

        MixVoices(buffer, offset, count);
        ApplySoundVolume(buffer, offset, count);
        MixMic(buffer, offset, count);
        Peak = Limit(buffer, offset, count);

        // A live output must never report end of stream.
        return count;
    }

    private void MixVoices(float[] buffer, int offset, int count)
    {
        lock (_lock)
        {
            foreach (var voice in _voices)
                voice.MixInto(buffer, offset, count);

            _voices.RemoveAll(voice => voice.Finished);
        }
    }

    private void ApplySoundVolume(float[] buffer, int offset, int count)
    {
        float volume = SoundVolume;
        if (volume == 1f)
            return;

        for (int index = 0; index < count; index++)
            buffer[offset + index] *= volume;
    }

    private void MixMic(float[] buffer, int offset, int count)
    {
        if (mic == null)
            return;

        if (_micBuffer.Length < count)
            _micBuffer = new float[count];

        // Always drained, so the mic stays current while it is muted.
        int read = mic.Read(_micBuffer, count);
        if (!MicEnabled)
            return;

        float volume = MicVolume;
        for (int index = 0; index < read; index++)
            buffer[offset + index] += _micBuffer[index] * volume;
    }

    private static float Limit(float[] buffer, int offset, int count)
    {
        float peak = 0f;
        for (int index = 0; index < count; index++)
        {
            float limited = SoftClip(buffer[offset + index]);
            buffer[offset + index] = limited;
            peak = Math.Max(peak, Math.Abs(limited));
        }
        return peak;
    }

    // Transparent below the knee, then eases towards 1 so overlapping sounds never hard clip.
    private static float SoftClip(float sample)
    {
        float magnitude = Math.Abs(sample);
        if (magnitude <= LimiterKnee)
            return sample;

        float over = (magnitude - LimiterKnee) / (1f - LimiterKnee);
        float shaped = LimiterKnee + (1f - LimiterKnee) * (over / (1f + over));
        return Math.Sign(sample) * shaped;
    }
}
