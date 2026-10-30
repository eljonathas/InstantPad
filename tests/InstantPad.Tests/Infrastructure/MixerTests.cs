using InstantPad.Infrastructure.Audio;
using Xunit;

namespace InstantPad.Tests.Infrastructure;

public sealed class MixerTests
{
    private static SoundClip Clip(float level, int samples = 4800) =>
        new(Enumerable.Repeat(level, samples).ToArray());

    [Fact]
    public void ALiveOutputAlwaysFillsTheBlock()
    {
        var mixer = new Mixer();

        Assert.Equal(960, mixer.Read(new float[960], 0, 960));
    }

    [Fact]
    public void FinishedVoicesLeaveTheMixer()
    {
        var mixer = new Mixer();
        var tag = new object();
        mixer.Add(new Voice(Clip(0.1f, samples: 100), 1f, tag));

        mixer.Read(new float[200], 0, 200);

        Assert.False(mixer.IsPlaying(tag));
    }

    [Fact]
    public void OverlappingSoundsNeverClip()
    {
        var mixer = new Mixer();
        var tag = new object();
        mixer.Add(new Voice(Clip(0.9f), 1f, tag));
        mixer.Add(new Voice(Clip(0.9f), 1f, tag));

        var buffer = new float[4800];
        mixer.Read(buffer, 0, buffer.Length);

        Assert.All(buffer, sample => Assert.True(Math.Abs(sample) <= 1f));
        Assert.True(mixer.Peak > 0.8f);
    }

    [Fact]
    public void SoundVolumeScalesTheVoices()
    {
        var mixer = new Mixer { SoundVolume = 0.5f };
        mixer.Add(new Voice(Clip(0.4f), 1f, new object()));

        var buffer = new float[100];
        mixer.Read(buffer, 0, buffer.Length);

        Assert.Equal(0.2f, buffer[0], precision: 3);
    }

    [Fact]
    public void StoppingByTagLeavesOtherSoundsPlaying()
    {
        var mixer = new Mixer();
        var stopped = new object();
        var kept = new object();
        mixer.Add(new Voice(Clip(0.1f), 1f, stopped));
        mixer.Add(new Voice(Clip(0.1f), 1f, kept));

        mixer.Remove(stopped);

        Assert.False(mixer.IsPlaying(stopped));
        Assert.True(mixer.IsPlaying(kept));
    }

    [Fact]
    public void AMutedMicStaysCurrentAndSilent()
    {
        var mic = new MicRingBuffer();
        var mixer = new Mixer(mic) { MicEnabled = false };
        mic.Write(Enumerable.Repeat(0.5f, 9600).ToArray(), 9600);

        var buffer = new float[1920];
        mixer.Read(buffer, 0, buffer.Length);
        Assert.All(buffer, sample => Assert.Equal(0f, sample));

        mixer.MicEnabled = true;
        mixer.Read(buffer, 0, buffer.Length);
        Assert.Equal(0.5f, buffer[0], precision: 3);
    }

    [Fact]
    public void TheMicBufferDropsBacklogInsteadOfBuildingDelay()
    {
        var mic = new MicRingBuffer();
        var oneSecond = Enumerable.Repeat(0.5f, AudioFormat.SamplesFor(1000)).ToArray();

        mic.Write(oneSecond, oneSecond.Length);

        var buffer = new float[oneSecond.Length];
        Assert.True(mic.Read(buffer, buffer.Length) <= AudioFormat.SamplesFor(120));
    }

    [Fact]
    public void TheMicBufferWaitsForItsPrebuffer()
    {
        var mic = new MicRingBuffer();
        mic.Write(new float[10], 10);

        Assert.Equal(0, mic.Read(new float[10], 10));
    }
}
