using System.Net;
using System.Text.RegularExpressions;
using InstantPad.Application.Sounds;
using InstantPad.Domain.Board;

namespace InstantPad.Infrastructure.Catalog;

/// <summary>
/// MyInstants has no public API, so this reads the same HTML the site serves.
/// If the site changes its markup, this is the only place to fix.
/// </summary>
public static partial class MyInstantsPageParser
{
    public static CatalogPage Parse(string html, string baseUrl, int page)
    {
        var sounds = new List<CatalogSound>();
        var seen = new HashSet<string>();

        foreach (Match match in SoundRegex().Matches(html))
        {
            var url = match.Groups["url"].Value;
            if (!seen.Add(url))
                continue;

            var name = WebUtility.HtmlDecode(match.Groups["name"].Value).Trim();
            var color = match.Groups["color"].Success ? match.Groups["color"].Value : PadPalette.Neutral;
            sounds.Add(new CatalogSound(name, baseUrl + url, color));
        }

        return new CatalogPage(sounds, HasMore: html.Contains($"page={page + 1}"));
    }

    [GeneratedRegex("""
        (?:background-color:\s*(?<color>\#[0-9a-fA-F]{3,8})[^>]*>\s*</div>\s*)?
        <button[^>]*onclick="play\('(?<url>/media/sounds/[^']+)',\s*'[^']*',\s*'[^']*'\)"[^>]*>
        [\s\S]{0,400}?
        class="instant-link[^"]*"[^>]*>(?<name>[^<]*)</a>
        """, RegexOptions.IgnorePatternWhitespace)]
    private static partial Regex SoundRegex();
}
