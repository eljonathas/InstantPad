namespace InstantPad.Infrastructure.Audio;

/// <summary>The format everything is converted to before mixing.</summary>
public static class AudioFormat
{
    public const int SampleRate = 48000;
    public const int Channels = 2;

    public static int SamplesFor(int milliseconds) => SampleRate * Channels * milliseconds / 1000;
}
