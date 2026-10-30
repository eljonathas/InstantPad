using InstantPad.Application.Updates;
using Xunit;

namespace InstantPad.Tests.Updates;

public sealed class ReleaseVersionTests
{
    [Theory]
    [InlineData("v1.2.3")]
    [InlineData("1.2.3")]
    public void TryParse_ReadsReleaseTags(string tag)
    {
        Assert.True(ReleaseVersion.TryParse(tag, out var version));
        Assert.Equal(new Version(1, 2, 3), version);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("nightly")]
    [InlineData("v1.2")]
    [InlineData("v1.2.3-beta")]
    public void TryParse_RejectsAnythingElse(string? tag)
    {
        Assert.False(ReleaseVersion.TryParse(tag, out _));
    }

    [Theory]
    [InlineData("1.2.4", "1.2.3", true)]
    [InlineData("1.3.0", "1.2.9", true)]
    [InlineData("2.0.0", "1.9.9", true)]
    [InlineData("1.10.0", "1.9.0", true)]
    [InlineData("1.2.3", "1.2.3", false)]
    [InlineData("1.2.3", "1.2.4", false)]
    public void IsNewer_ComparesNumerically(string candidate, string current, bool expected)
    {
        Assert.Equal(expected, ReleaseVersion.IsNewer(new Version(candidate), new Version(current)));
    }

    [Fact]
    public void IsNewer_TreatsTheAssemblyFourthPartAsEqual()
    {
        Assert.False(ReleaseVersion.IsNewer(new Version(1, 2, 3), new Version(1, 2, 3, 0)));
        Assert.False(ReleaseVersion.IsNewer(new Version(1, 2, 3, 0), new Version(1, 2, 3)));
    }
}
