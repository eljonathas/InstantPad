using InstantPad.Infrastructure.Catalog;
using Xunit;

namespace InstantPad.Tests.Infrastructure;

public sealed class MyInstantsPageParserTests
{
    private const string BaseUrl = "https://www.myinstants.com";

    private const string Html = """
        <div class="instant">
        <div class="circle small-button-background" style="background-color:#ffff33;"></div>
        <button class="small-button" onclick="play('/media/sounds/movie_1.mp3', 'loader-23010', 'bruh')" title="Play BRUH sound" type="button"></button>
        <div id="loader-23010" class="loader"></div>
        <div class="small-button-shadow"></div>
        <a href="/en/instant/bruh/" class="instant-link link-secondary">BRUH &amp; cia</a>
        </div>
        <div class="instant">
        <button class="small-button" onclick="play('/media/sounds/sem-cor.mp3', 'loader-2', 'sem-cor')" title="Play" type="button"></button>
        <a href="/en/instant/sem-cor/" class="instant-link link-secondary">Sem cor</a>
        </div>
        <a href="?page=2">next</a>
        """;

    [Fact]
    public void ReadsNameUrlAndColor()
    {
        var sound = MyInstantsPageParser.Parse(Html, BaseUrl, page: 1).Sounds[0];

        Assert.Equal("BRUH & cia", sound.Name);
        Assert.Equal($"{BaseUrl}/media/sounds/movie_1.mp3", sound.SoundUrl);
        Assert.Equal("#ffff33", sound.Color);
    }

    [Fact]
    public void ASoundWithoutColorStillComesThrough()
    {
        var page = MyInstantsPageParser.Parse(Html, BaseUrl, page: 1);

        Assert.Equal(2, page.Sounds.Count);
        Assert.Equal("Sem cor", page.Sounds[1].Name);
    }

    [Fact]
    public void RepeatedSoundsAppearOnce()
    {
        var page = MyInstantsPageParser.Parse(Html + Html, BaseUrl, page: 1);

        Assert.Equal(2, page.Sounds.Count);
    }

    [Theory]
    [InlineData(1, true)]
    [InlineData(2, false)]
    public void KnowsWhetherThereIsAnotherPage(int page, bool hasMore)
    {
        Assert.Equal(hasMore, MyInstantsPageParser.Parse(Html, BaseUrl, page).HasMore);
    }

    [Fact]
    public void APageWithoutSoundsIsEmpty()
    {
        Assert.Empty(MyInstantsPageParser.Parse("<html></html>", BaseUrl, page: 1).Sounds);
    }
}
