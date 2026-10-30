using System.Collections.Concurrent;
using System.IO;
using System.Runtime.InteropServices;
using InstantPad.Application.Audio;
using InstantPad.Application.Localization;

namespace InstantPad.Infrastructure.Audio;

/// <summary>Plays sound files through the engine, keeping them decoded in memory.</summary>
public sealed class ClipPlayer(AudioEngine engine) : ISoundPlayer
{
    private readonly ConcurrentDictionary<string, Task<SoundClip>> _clips = new();

    public Task PreloadAsync(string path) => LoadAsync(path);

    public async Task PlayAsync(string path, float volume, object tag)
    {
        var clip = await LoadAsync(path);
        engine.Stop(tag);
        engine.PlayForGame(clip, volume, tag);
        engine.PlayForMonitor(clip, volume, tag);
    }

    public async Task PreviewAsync(string path, float volume, object tag)
    {
        var clip = await LoadAsync(path);
        engine.Stop(tag);
        engine.PlayForMonitor(clip, volume, tag);
    }

    public void Forget(string path) => _clips.TryRemove(path, out _);

    public void Stop(object tag) => engine.Stop(tag);

    public void StopAll() => engine.StopAll();

    public bool IsPlaying(object tag) => engine.IsPlaying(tag);

    private async Task<SoundClip> LoadAsync(string path)
    {
        var load = _clips.GetOrAdd(path, _ => Task.Run(() => SoundClip.Load(path)));
        try
        {
            return await load;
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or InvalidDataException or COMException)
        {
            // A failed load must be retried next time, not cached forever.
            Forget(path);
            throw new SoundUnavailableException(string.Format(Strings.SoundOpenFailed, exception.Message), exception);
        }
    }
}
