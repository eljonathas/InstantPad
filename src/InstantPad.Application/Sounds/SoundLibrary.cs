using InstantPad.Application.Audio;
using InstantPad.Application.Localization;
using InstantPad.Domain.Sounds;

namespace InstantPad.Application.Sounds;

/// <summary>Turns catalog results into sounds with a local file behind them.</summary>
public sealed class SoundLibrary(ISoundCatalog catalog, ISoundFileStore files)
{
    public SoundSource ToSource(CatalogSound sound) => new()
    {
        Name = sound.Name,
        Color = sound.Color,
        Path = files.PathForDownload(sound.SoundUrl),
        SoundUrl = sound.SoundUrl,
    };

    public async Task EnsureDownloadedAsync(SoundSource source, CancellationToken token = default)
    {
        if (files.Exists(source.Path))
            return;
        if (source.SoundUrl == null)
            throw new SoundUnavailableException(Strings.SoundFileMissing);

        await catalog.DownloadAsync(source.SoundUrl, source.Path, token);
    }

    /// <summary>Saves the audio file on the user's computer, outside the app.</summary>
    public async Task SaveToComputerAsync(SoundSource source, string destination, CancellationToken token = default)
    {
        await EnsureDownloadedAsync(source, token);
        files.Export(source.Path, destination);
    }
}
