namespace InstantPad.Application.Audio;

public sealed record SavedRouting(string? MicrophoneId, string? GameOutputId, string? MonitorOutputId);

public sealed record DefaultDevices(string? MicrophoneId, string? OutputId);

public sealed record RoutingChoice(AudioDevice? Microphone, AudioDevice? GameOutput, AudioDevice? MonitorOutput);

/// <summary>
/// Which device can play which role. Each role excludes the mistakes that wreck the sound:
/// a mic reading the cable feeds the mix back into itself, and a real device as the game
/// output sends the mic and every sound to your headphones a second time.
/// </summary>
public sealed class AudioRouting(IVirtualCable cable)
{
    public IReadOnlyList<AudioDevice> MicrophonesFrom(IEnumerable<AudioDevice> devices) =>
        devices.Where(device => !cable.IsVirtual(device)).ToList();

    public IReadOnlyList<AudioDevice> GameOutputsFrom(IEnumerable<AudioDevice> devices) =>
        devices.Where(cable.IsVirtual).ToList();

    public IReadOnlyList<AudioDevice> MonitorOutputsFrom(IEnumerable<AudioDevice> devices) =>
        devices.Where(device => !cable.IsVirtual(device)).ToList();

    public RoutingChoice Choose(
        IReadOnlyList<AudioDevice> microphones,
        IReadOnlyList<AudioDevice> outputs,
        SavedRouting saved,
        DefaultDevices defaults)
    {
        var usableMicrophones = MicrophonesFrom(microphones);
        var gameOutputs = GameOutputsFrom(outputs);
        var monitorOutputs = MonitorOutputsFrom(outputs);

        return new RoutingChoice(
            WithId(usableMicrophones, saved.MicrophoneId) ?? WithId(usableMicrophones, defaults.MicrophoneId) ?? usableMicrophones.FirstOrDefault(),
            WithId(gameOutputs, saved.GameOutputId) ?? cable.FindGameOutput(gameOutputs),
            WithId(monitorOutputs, saved.MonitorOutputId) ?? WithId(monitorOutputs, defaults.OutputId) ?? monitorOutputs.FirstOrDefault());
    }

    private static AudioDevice? WithId(IEnumerable<AudioDevice> devices, string? id) =>
        id == null ? null : devices.FirstOrDefault(device => device.Id == id);
}
