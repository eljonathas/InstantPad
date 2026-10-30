using InstantPad.Application.Audio;
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
            throw new SoundUnavailableException("O arquivo do som não existe mais.");

        await catalog.DownloadAsync(source.SoundUrl, source.Path, token);
    }
}
