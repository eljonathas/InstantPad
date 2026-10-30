using InstantPad.Application.Updates;
using Xunit;

namespace InstantPad.Tests.Updates;

public sealed class UpdateServiceTests
{
    private static readonly Version Current = new(1, 2, 3, 0);

    private readonly FakeSource _source = new();
    private readonly FakeInstaller _installer = new();
    private readonly FakeRestarter _restarter = new();

    private UpdateService Service => new(_source, _installer, _restarter, Current);

    private static ReleaseInfo Release(string version) =>
        new(new Version(version), new Uri("https://example.test/a"), new Uri("https://example.test/b"));

    [Fact]
    public async Task FindsANewerRelease()
    {
        _source.Latest = Release("1.2.4");

        Assert.Equal(_source.Latest, await Service.FindUpdateAsync());
    }

    [Theory]
    [InlineData("1.2.3")]
    [InlineData("1.2.2")]
    public async Task IgnoresReleasesThatAreNotNewer(string version)
    {
        _source.Latest = Release(version);

        Assert.Null(await Service.FindUpdateAsync());
    }

    [Fact]
    public async Task NoReleaseMeansNoUpdate()
    {
        Assert.Null(await Service.FindUpdateAsync());
    }

    [Fact]
    public async Task InstallingNeverRestartsTheApp()
    {
        await Service.InstallAsync(Release("2.0.0"));

        Assert.True(_installer.Installed);
        Assert.False(_restarter.Restarted);
    }

    [Fact]
    public void RestartsOnlyWhenAsked()
    {
        Service.RestartIntoNewVersion();

        Assert.True(_restarter.Restarted);
    }

    [Theory]
    [InlineData("0.0.0.0", false)]
    [InlineData("1.0.0.0", true)]
    public void DevelopmentBuildsAreNotReleaseBuilds(string version, bool isRelease)
    {
        var service = new UpdateService(_source, _installer, _restarter, new Version(version));

        Assert.Equal(isRelease, service.IsReleaseBuild);
    }

    [Fact]
    public async Task CleanUpReachesTheInstaller()
    {
        await Service.CleanUpPreviousVersionAsync();

        Assert.True(_installer.CleanedUp);
    }

    private sealed class FakeSource : IUpdateSource
    {
        public ReleaseInfo? Latest { get; set; }

        public Task<ReleaseInfo?> GetLatestAsync(CancellationToken cancellationToken) => Task.FromResult(Latest);
    }

    private sealed class FakeInstaller : IUpdateInstaller
    {
        public Exception? Failure { get; set; }
        public bool Installed { get; private set; }
        public bool CleanedUp { get; private set; }

        public Task InstallAsync(ReleaseInfo release, IProgress<int>? progress, CancellationToken cancellationToken)
        {
            if (Failure != null)
                throw Failure;

            Installed = true;
            return Task.CompletedTask;
        }

        public Task CleanUpPreviousVersionAsync(CancellationToken cancellationToken)
        {
            CleanedUp = true;
            return Task.CompletedTask;
        }
    }

    private sealed class FakeRestarter : IApplicationRestarter
    {
        public bool Restarted { get; private set; }

        public void Restart() => Restarted = true;
    }
}
