using System.Net.Http;
using InstantPad.Application.Audio;
using InstantPad.Infrastructure.VirtualCable;
using Xunit;

namespace InstantPad.Tests.Application;

public sealed class AudioRoutingTests : IDisposable
{
    private static readonly AudioDevice RealMic = new("mic", "Microfone (fifine Microphone)");
    private static readonly AudioDevice CableOutput = new("cable-out", "CABLE Output (VB-Audio Virtual Cable)");
    private static readonly AudioDevice CableInput = new("cable-in", "CABLE Input (VB-Audio Virtual Cable)");
    private static readonly AudioDevice Cable16Channels = new("cable-16", "CABLE In 16ch (VB-Audio Virtual Cable)");
    private static readonly AudioDevice Headset = new("headset", "Alto-falantes (BW06-BT)");

    private static readonly AudioDevice[] Microphones = [RealMic, CableOutput];
    private static readonly AudioDevice[] Outputs = [Cable16Channels, CableInput, Headset];
    private static readonly SavedRouting NothingSaved = new(null, null, null);

    private readonly HttpClient _http = new();
    private readonly AudioRouting _routing;

    public AudioRoutingTests() => _routing = new AudioRouting(new VbCable(_http));

    public void Dispose() => _http.Dispose();

    [Fact]
    public void AHeadsetSavedAsGameOutputGivesWayToTheCable()
    {
        var saved = new SavedRouting(null, Headset.Id, Headset.Id);

        var choice = _routing.Choose(Microphones, Outputs, saved, new DefaultDevices(RealMic.Id, Headset.Id));

        Assert.Equal(CableInput, choice.GameOutput);
        Assert.Equal(Headset, choice.MonitorOutput);
    }

    [Fact]
    public void TheCableIsNeverTheMicrophoneEvenWhenWindowsDefaultsToIt()
    {
        var choice = _routing.Choose(Microphones, Outputs, NothingSaved, new DefaultDevices(CableOutput.Id, Headset.Id));

        Assert.Equal(RealMic, choice.Microphone);
    }

    [Fact]
    public void TheCableIsNeverWhereYouListenEvenWhenWindowsDefaultsToIt()
    {
        var choice = _routing.Choose(Microphones, Outputs, NothingSaved, new DefaultDevices(RealMic.Id, CableInput.Id));

        Assert.Equal(Headset, choice.MonitorOutput);
    }

    [Fact]
    public void WithoutACableThereIsNoGameOutput()
    {
        var choice = _routing.Choose([RealMic], [Headset], NothingSaved, new DefaultDevices(RealMic.Id, Headset.Id));

        Assert.Null(choice.GameOutput);
    }

    [Fact]
    public void ASavedCableIsKept()
    {
        var saved = new SavedRouting(null, Cable16Channels.Id, null);

        var choice = _routing.Choose(Microphones, Outputs, saved, new DefaultDevices(null, null));

        Assert.Equal(Cable16Channels, choice.GameOutput);
    }

    [Fact]
    public void EachRoleOnlyListsDevicesThatCanPlayIt()
    {
        Assert.Equal([RealMic], _routing.MicrophonesFrom(Microphones));
        Assert.Equal([Cable16Channels, CableInput], _routing.GameOutputsFrom(Outputs));
        Assert.Equal([Headset], _routing.MonitorOutputsFrom(Outputs));
    }
}
