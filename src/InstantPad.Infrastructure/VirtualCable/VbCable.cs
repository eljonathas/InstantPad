using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Net.Http;
using System.Text.RegularExpressions;
using InstantPad.Application.Audio;

namespace InstantPad.Infrastructure.VirtualCable;

/// <summary>
/// Windows only loads Microsoft-signed audio drivers, so the virtual device is VB-CABLE.
/// It is downloaded from the vendor on request and installed by its own signed setup.
/// </summary>
public sealed partial class VbCable(HttpClient http) : IVirtualCable
{
    private const string ProductPage = "https://vb-audio.com/Cable/";
    private const string FallbackDriverPack = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";
    private const string SetupFile = "VBCABLE_Setup_x64.exe";
    private const int ErrorCancelled = 1223;

    private const string DefaultGameMicrophone = "CABLE Output";
    private const string VoiceMeeterName = "VoiceMeeter";
    private const string HiFiCableName = "Hi-Fi";

    // Render side of each known virtual cable: what this app plays into.
    private static readonly string[] RenderNames =
        ["CABLE Input", "CABLE In ", "VoiceMeeter Input", "VoiceMeeter AUX Input", "Hi-Fi Cable Input"];

    public AudioDevice? FindGameOutput(IEnumerable<AudioDevice> outputs)
    {
        var devices = outputs.ToList();
        return RenderNames
            .Select(name => devices.Find(device => HasName(device, name)))
            .FirstOrDefault(device => device != null);
    }

    public bool IsVirtual(AudioDevice device) => RenderNames.Any(name => HasName(device, name));

    public string GameMicrophoneName(AudioDevice? gameOutput)
    {
        if (gameOutput != null && HasName(gameOutput, VoiceMeeterName))
            return "VoiceMeeter Output";
        if (gameOutput != null && HasName(gameOutput, HiFiCableName))
            return "Hi-Fi Cable Output";
        return DefaultGameMicrophone;
    }

    public async Task InstallAsync(IProgress<string> progress, CancellationToken token = default)
    {
        try
        {
            progress.Report("Procurando a versão mais recente do VB-CABLE...");
            var driverPack = await FindDriverPackAsync(token);

            progress.Report("Baixando o driver do site oficial...");
            var setup = await DownloadSetupAsync(driverPack, token);

            progress.Report("Conferindo a assinatura digital...");
            VendorSignature.Verify(setup);

            progress.Report("Abrindo o instalador. Clique em \"Install Driver\" e reinicie o PC ao terminar.");
            Process.Start(new ProcessStartInfo(setup) { UseShellExecute = true, Verb = "runas" });
        }
        catch (Win32Exception exception) when (exception.NativeErrorCode == ErrorCancelled)
        {
            throw new DriverInstallCanceledException();
        }
        catch (Exception exception) when (exception is HttpRequestException or TaskCanceledException or IOException or InvalidDataException)
        {
            throw new DriverInstallException($"Falhou: {exception.Message}", exception);
        }
    }

    private static bool HasName(AudioDevice device, string name) =>
        device.Name.Contains(name, StringComparison.OrdinalIgnoreCase);

    private async Task<string> FindDriverPackAsync(CancellationToken token)
    {
        try
        {
            var html = await http.GetStringAsync(ProductPage, token);
            var newest = DriverPackRegex().Matches(html).MaxBy(match => int.Parse(match.Groups[1].Value));
            return newest?.Value ?? FallbackDriverPack;
        }
        catch (HttpRequestException)
        {
            // The direct link still works when only the product page is down.
            return FallbackDriverPack;
        }
    }

    private async Task<string> DownloadSetupAsync(string driverPack, CancellationToken token)
    {
        var folder = Path.Combine(Path.GetTempPath(), "InstantPad-VBCABLE-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(folder);

        var zipPath = Path.Combine(folder, "driver.zip");
        await using (var source = await http.GetStreamAsync(driverPack, token))
        await using (var target = File.Create(zipPath))
            await source.CopyToAsync(target, token);

        var extracted = Path.Combine(folder, "pack");
        ZipFile.ExtractToDirectory(zipPath, extracted);

        var setup = Path.Combine(extracted, SetupFile);
        return File.Exists(setup)
            ? setup
            : throw new FileNotFoundException($"{SetupFile} não veio no pacote baixado.");
    }

    [GeneratedRegex(@"https://download\.vb-audio\.com/Download_CABLE/VBCABLE_Driver_Pack(\d+)\.zip")]
    private static partial Regex DriverPackRegex();
}
