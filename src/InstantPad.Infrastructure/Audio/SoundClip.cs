using System.IO;
using InstantPad.Application.Localization;
using NAudio.Wave;

namespace InstantPad.Infrastructure.Audio;

/// <summary>A sound fully decoded to the engine format.</summary>
public sealed class SoundClip(float[] samples)
{
    private const int ReadBlockSamples = AudioFormat.SampleRate * AudioFormat.Channels;

    public float[] Samples { get; } = samples;

    public static SoundClip Load(string path)
    {
        using var reader = OpenReader(path);
        var source = FormatConversion.ToEngineFormat(reader.ToSampleProvider());

        var block = new float[ReadBlockSamples];
        var samples = new List<float>(block.Length);
        int read;
        while ((read = source.Read(block, 0, block.Length)) > 0)
            samples.AddRange(new ArraySegment<float>(block, 0, read));

        if (samples.Count == 0)
            throw new InvalidDataException(Strings.SoundHasNoAudio);

        return new SoundClip(samples.ToArray());
    }

    private static WaveStream OpenReader(string path) =>
        Path.GetExtension(path).Equals(".wav", StringComparison.OrdinalIgnoreCase)
            ? new WaveFileReader(path)
            : new MediaFoundationReader(path);
}
