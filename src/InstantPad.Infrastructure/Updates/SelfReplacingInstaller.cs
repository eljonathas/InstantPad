using System.IO;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text.RegularExpressions;
using InstantPad.Application.Updates;

namespace InstantPad.Infrastructure.Updates;

/// <summary>
/// Windows blocks overwriting a running executable but allows renaming it, so the new
/// version is verified next to it and then swapped in by two renames.
/// </summary>
public sealed partial class SelfReplacingInstaller : IUpdateInstaller
{
    private const int CleanUpAttempts = 4;
    private const int DownloadBufferSize = 81920;
    private static readonly TimeSpan CleanUpDelay = TimeSpan.FromMilliseconds(250);

    private readonly HttpClient _http;
    private readonly string _executablePath;
    private readonly string _downloadPath;
    private readonly string _previousPath;

    public SelfReplacingInstaller(HttpClient http, string executablePath)
    {
        _http = http;
        _executablePath = executablePath;
        _downloadPath = executablePath + ".new";
        _previousPath = executablePath + ".old";
    }

    public async Task InstallAsync(ReleaseInfo release, IProgress<int>? progress, CancellationToken cancellationToken)
    {
        try
        {
            var expectedHash = await DownloadExpectedHashAsync(release.ChecksumUrl, cancellationToken);
            var actualHash = await DownloadExecutableAsync(release.ExecutableUrl, progress, cancellationToken);

            if (!string.Equals(expectedHash, actualHash, StringComparison.OrdinalIgnoreCase))
                throw new UpdateFailedException("O arquivo baixado não confere com a versão publicada. Nada foi alterado.");

            SwapExecutable();
        }
        catch (Exception exception) when (exception is HttpRequestException or TaskCanceledException && !cancellationToken.IsCancellationRequested)
        {
            throw new UpdateFailedException("Não foi possível baixar a atualização. Verifique sua conexão.", exception);
        }
        catch (Exception exception) when (exception is UnauthorizedAccessException or IOException)
        {
            throw new UpdateFailedException(
                $"Sem permissão para atualizar em \"{Path.GetDirectoryName(_executablePath)}\". " +
                "Mova o InstantPad para uma pasta sua, como Documentos, e tente de novo.",
                exception);
        }
        finally
        {
            DeleteIgnoringFailure(_downloadPath);
        }
    }

    public async Task CleanUpPreviousVersionAsync(CancellationToken cancellationToken)
    {
        // The old process may still be closing, and the next start retries anyway.
        for (int attempt = 0; attempt < CleanUpAttempts; attempt++)
        {
            if (DeleteIgnoringFailure(_previousPath))
                return;

            await Task.Delay(CleanUpDelay, cancellationToken);
        }
    }

    private async Task<string> DownloadExpectedHashAsync(Uri checksumUrl, CancellationToken cancellationToken)
    {
        var content = await _http.GetStringAsync(checksumUrl, cancellationToken);
        var match = Sha256Regex().Match(content);
        return match.Success
            ? match.Value
            : throw new UpdateFailedException("A atualização publicada não tem um checksum válido. Nada foi alterado.");
    }

    private async Task<string> DownloadExecutableAsync(Uri executableUrl, IProgress<int>? progress, CancellationToken cancellationToken)
    {
        using var response = await _http.GetAsync(executableUrl, HttpCompletionOption.ResponseHeadersRead, cancellationToken);
        response.EnsureSuccessStatusCode();

        await using var source = await response.Content.ReadAsStreamAsync(cancellationToken);
        await using var target = File.Create(_downloadPath);
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);

        long total = response.Content.Headers.ContentLength ?? 0;
        long received = 0;
        var buffer = new byte[DownloadBufferSize];
        int read;
        while ((read = await source.ReadAsync(buffer, cancellationToken)) > 0)
        {
            hash.AppendData(buffer, 0, read);
            await target.WriteAsync(buffer.AsMemory(0, read), cancellationToken);
            received += read;
            if (total > 0)
                progress?.Report((int)(received * 100 / total));
        }

        return Convert.ToHexString(hash.GetHashAndReset());
    }

    private void SwapExecutable()
    {
        DeleteIgnoringFailure(_previousPath);
        File.Move(_executablePath, _previousPath);
        try
        {
            File.Move(_downloadPath, _executablePath);
        }
        catch (Exception exception) when (exception is UnauthorizedAccessException or IOException)
        {
            File.Move(_previousPath, _executablePath);
            throw;
        }
    }

    private static bool DeleteIgnoringFailure(string path)
    {
        try
        {
            File.Delete(path);
            return true;
        }
        catch (Exception exception) when (exception is UnauthorizedAccessException or IOException)
        {
            return false;
        }
    }

    [GeneratedRegex(@"\b[0-9a-fA-F]{64}\b")]
    private static partial Regex Sha256Regex();
}
