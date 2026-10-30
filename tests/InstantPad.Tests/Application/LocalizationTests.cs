using System.Collections;
using System.Globalization;
using System.Text.RegularExpressions;
using InstantPad.Application.Localization;
using Xunit;

namespace InstantPad.Tests.Application;

public sealed partial class LocalizationTests
{
    private static Dictionary<string, string> Texts(string language)
    {
        var set = Strings.ResourceManager.GetResourceSet(CultureInfo.GetCultureInfo(language), createIfNotExists: true, tryParents: false)!;
        return set.Cast<DictionaryEntry>().ToDictionary(entry => (string)entry.Key, entry => (string)entry.Value!);
    }

    private static List<string> Placeholders(string text) =>
        PlaceholderRegex().Matches(text).Select(match => match.Value).Distinct().Order().ToList();

    [Fact]
    public void EveryLanguageHasTheSameTexts()
    {
        Assert.Equal(Texts("").Keys.Order(), Texts("pt").Keys.Order());
    }

    [Fact]
    public void TranslationsKeepTheirPlaceholders()
    {
        var portuguese = Texts("pt");

        Assert.All(Texts(""), english => Assert.Equal(Placeholders(english.Value), Placeholders(portuguese[english.Key])));
    }

    [Fact]
    public void TranslationsAreNotEmpty()
    {
        Assert.All(Texts("pt"), text => Assert.False(string.IsNullOrWhiteSpace(text.Value), text.Key));
    }

    [GeneratedRegex(@"\{\d+\}")]
    private static partial Regex PlaceholderRegex();
}
