using System.IO;
using System.Net;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using InstantPad.Application.Updates;
using InstantPad.Infrastructure.Updates;
using Xunit;

namespace InstantPad.Tests.Updates;

public sealed class SelfReplacingInstallerTests : IDisposable
{
    private const string ExecutableUrl = "https://example.test/InstantPad.exe";
    private const string ChecksumUrl = "https://example.test/InstantPad.exe.sha256";

    private static readonly byte[] CurrentBytes = Encoding.UTF8.GetBytes("current version");
    private static readonly byte[] UpdateBytes = Encoding.UTF8.GetBytes("new version");
    private static readonly ReleaseInfo Release = new(new Version(2, 0, 0), new Uri(ExecutableUrl), new Uri(ChecksumUrl));

    private readonly string _folder = Directory.CreateTempSubdirectory("instantpad-update-").FullName;
    private readonly string _executable;

    public SelfReplacingInstallerTests()
    {
        _executable = Path.Combine(_folder, "InstantPad.exe");
        File.WriteAllBytes(_executable, CurrentBytes);
    }

    public void Dispose() => Directory.Delete(_folder, recursive: true);

    private static string Sha256(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();

    private SelfReplacingInstaller Installer(StubHttpHandler handler) => new(new HttpClient(handler), _executable);

    private SelfReplacingInstaller HealthyInstaller() => Installer(new StubHttpHandler()
        .Respond(ChecksumUrl, $"{Sha256(UpdateBytes)} *InstantPad.exe\n")
        .Respond(ExecutableUrl, UpdateBytes));

    private void AssertCurrentVersionIntact()
    {
        Assert.Equal(CurrentBytes, File.ReadAllBytes(_executable));
        Assert.False(File.Exists(_executable + ".new"));
    }

    [Fact]
    public async Task InstallReplacesTheExecutableAndKeepsThePreviousOne()
    {
        await HealthyInstaller().InstallAsync(Release, null, CancellationToken.None);

        Assert.Equal(UpdateBytes, File.ReadAllBytes(_executable));
        Assert.Equal(CurrentBytes, File.ReadAllBytes(_executable + ".old"));
        Assert.False(File.Exists(_executable + ".new"));
    }

    [Fact]
    public async Task InstallWorksWhileTheExecutableIsOpen()
    {
        // Same sharing Windows applies to the image of a running process.
        await using var running = new FileStream(_executable, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete);

        await HealthyInstaller().InstallAsync(Release, null, CancellationToken.None);

        Assert.Equal(UpdateBytes, File.ReadAllBytes(_executable));
    }

    [Fact]
    public async Task InstallReportsProgressUpToOneHundred()
    {
        var reported = new List<int>();

        await HealthyInstaller().InstallAsync(Release, new InlineProgress(reported), CancellationToken.None);

        Assert.Equal(100, reported[^1]);
    }

    [Fact]
    public async Task DivergentHashIsRefused()
    {
        var installer = Installer(new StubHttpHandler()
            .Respond(ChecksumUrl, Sha256(CurrentBytes))
            .Respond(ExecutableUrl, UpdateBytes));

        await Assert.ThrowsAsync<UpdateFailedException>(() => installer.InstallAsync(Release, null, CancellationToken.None));

        AssertCurrentVersionIntact();
        Assert.False(File.Exists(_executable + ".old"));
    }

    [Theory]
    [InlineData("")]
    [InlineData("not a hash")]
    public async Task InvalidChecksumFileIsRefused(string checksum)
    {
        var installer = Installer(new StubHttpHandler()
            .Respond(ChecksumUrl, checksum)
            .Respond(ExecutableUrl, UpdateBytes));

        await Assert.ThrowsAsync<UpdateFailedException>(() => installer.InstallAsync(Release, null, CancellationToken.None));

        AssertCurrentVersionIntact();
    }

    [Fact]
    public async Task MissingDownloadFailsWithoutTouchingTheExecutable()
    {
        var installer = Installer(new StubHttpHandler().Respond(ChecksumUrl, Sha256(UpdateBytes)));

        await Assert.ThrowsAsync<UpdateFailedException>(() => installer.InstallAsync(Release, null, CancellationToken.None));

        AssertCurrentVersionIntact();
    }

    [Fact]
    public async Task NetworkFailureFailsWithoutTouchingTheExecutable()
    {
        var installer = Installer(new StubHttpHandler().Fail(ChecksumUrl));

        await Assert.ThrowsAsync<UpdateFailedException>(() => installer.InstallAsync(Release, null, CancellationToken.None));

        AssertCurrentVersionIntact();
    }

    [Fact]
    public async Task FolderThatCannotBeWrittenFailsWithAClearMessage()
    {
        // A folder in the way of the download raises the same error as a folder without permission.
        Directory.CreateDirectory(_executable + ".new");

        var exception = await Assert.ThrowsAsync<UpdateFailedException>(
            () => HealthyInstaller().InstallAsync(Release, null, CancellationToken.None));

        Assert.Contains(_folder, exception.Message);
        Assert.Equal(CurrentBytes, File.ReadAllBytes(_executable));
    }

    [Fact]
    public async Task LockedLeftoverFailsWithoutLosingTheExecutable()
    {
        await using var locked = File.Open(_executable + ".old", FileMode.Create, FileAccess.Write, FileShare.None);

        await Assert.ThrowsAsync<UpdateFailedException>(
            () => HealthyInstaller().InstallAsync(Release, null, CancellationToken.None));

        AssertCurrentVersionIntact();
    }

    [Fact]
    public async Task CancellationIsNotReportedAsAFailure()
    {
        using var cancelled = new CancellationTokenSource();
        await cancelled.CancelAsync();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => HealthyInstaller().InstallAsync(Release, null, cancelled.Token));

        AssertCurrentVersionIntact();
    }

    [Fact]
    public async Task CleanUpRemovesThePreviousVersion()
    {
        File.WriteAllBytes(_executable + ".old", CurrentBytes);

        await HealthyInstaller().CleanUpPreviousVersionAsync(CancellationToken.None);

        Assert.False(File.Exists(_executable + ".old"));
    }

    [Fact]
    public async Task CleanUpWithNothingToRemoveDoesNothing()
    {
        await HealthyInstaller().CleanUpPreviousVersionAsync(CancellationToken.None);

        Assert.Equal(CurrentBytes, File.ReadAllBytes(_executable));
    }

    [Fact]
    public async Task CleanUpGivesUpQuietlyWhenThePreviousVersionIsStillOpen()
    {
        await using var stillClosing = File.Open(_executable + ".old", FileMode.Create, FileAccess.Write, FileShare.None);

        await HealthyInstaller().CleanUpPreviousVersionAsync(CancellationToken.None);

        Assert.True(File.Exists(_executable + ".old"));
    }

    // Progress<T> posts to a sync context, which would race the assertion.
    private sealed class InlineProgress(List<int> values) : IProgress<int>
    {
        public void Report(int value) => values.Add(value);
    }
}
