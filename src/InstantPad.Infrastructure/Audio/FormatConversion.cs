using NAudio.Wave;
using NAudio.Wave.SampleProviders;

namespace InstantPad.Infrastructure.Audio;

public static class FormatConversion
{
    /// <summary>Wraps any source so it reads as stereo at the engine sample rate.</summary>
    public static ISampleProvider ToEngineFormat(ISampleProvider source)
    {
        var stereo = source.WaveFormat.Channels switch
        {
            1 => new MonoToStereoSampleProvider(source),
            2 => source,
            _ => new FirstTwoChannelsSampleProvider(source),
        };

        return stereo.WaveFormat.SampleRate == AudioFormat.SampleRate
            ? stereo
            : new WdlResamplingSampleProvider(stereo, AudioFormat.SampleRate);
    }
}

/// <summary>Reduces a multichannel stream to its first two channels.</summary>
public sealed class FirstTwoChannelsSampleProvider : ISampleProvider
{
    private const int OutputChannels = 2;

    private readonly ISampleProvider _source;
    private readonly int _sourceChannels;
    private float[] _buffer = [];

    public FirstTwoChannelsSampleProvider(ISampleProvider source)
    {
        _source = source;
        _sourceChannels = source.WaveFormat.Channels;
        WaveFormat = WaveFormat.CreateIeeeFloatWaveFormat(source.WaveFormat.SampleRate, OutputChannels);
    }

    public WaveFormat WaveFormat { get; }

    public int Read(float[] buffer, int offset, int count)
    {
        int frames = count / OutputChannels;
        int needed = frames * _sourceChannels;
        if (_buffer.Length < needed)
            _buffer = new float[needed];

        int framesRead = _source.Read(_buffer, 0, needed) / _sourceChannels;
        for (int frame = 0; frame < framesRead; frame++)
        {
            buffer[offset + frame * OutputChannels] = _buffer[frame * _sourceChannels];
            buffer[offset + frame * OutputChannels + 1] = _buffer[frame * _sourceChannels + 1];
        }

        return framesRead * OutputChannels;
    }
}
