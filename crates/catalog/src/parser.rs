//! MyInstants has no public API, so this reads the same HTML the site serves.
//! If the site changes its markup, this is the only place to fix.

use std::collections::HashSet;
use std::sync::LazyLock;

use instantpad_application::ports::{CatalogPage, CatalogSound};
use instantpad_domain::palette::NEUTRAL;
use regex::Regex;

static SOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)
        (?:background-color:\s*(?<color>\#[0-9a-fA-F]{3,8})[^>]*>\s*</div>\s*)?
        <button[^>]*onclick="play\('(?<url>/media/sounds/[^']+)',\s*'[^']*',\s*'[^']*'\)"[^>]*>
        [\s\S]{0,400}?
        class="instant-link[^"]*"[^>]*>(?<name>[^<]*)</a>
        "#,
    )
    .expect("the sound pattern is valid")
});

pub fn parse(html: &str, base_url: &str, page: u32) -> CatalogPage {
    let mut seen = HashSet::new();
    let sounds = SOUND
        .captures_iter(html)
        .filter(|found| seen.insert(found["url"].to_owned()))
        .map(|found| CatalogSound {
            name: html_escape::decode_html_entities(&found["name"])
                .trim()
                .to_owned(),
            sound_url: format!("{base_url}{}", &found["url"]),
            color: found
                .name("color")
                .map_or(NEUTRAL, |color| color.as_str())
                .to_owned(),
        })
        .collect();

    CatalogPage {
        sounds,
        has_more: html.contains(&format!("page={}", page + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_URL: &str = "https://www.myinstants.com";
    const HTML: &str = include_str!("../tests/fixtures/results.html");
    const EMPTY: &str = include_str!("../tests/fixtures/empty.html");

    #[test]
    fn reads_name_url_and_color() {
        let sound = &parse(HTML, BASE_URL, 1).sounds[0];

        assert_eq!(sound.name, "BRUH & cia");
        assert_eq!(
            sound.sound_url,
            "https://www.myinstants.com/media/sounds/movie_1.mp3"
        );
        assert_eq!(sound.color, "#ffff33");
    }

    #[test]
    fn a_sound_without_color_still_comes_through() {
        let page = parse(HTML, BASE_URL, 1);

        assert_eq!(page.sounds.len(), 2);
        assert_eq!(page.sounds[1].name, "Sem cor");
        assert_eq!(page.sounds[1].color, NEUTRAL);
    }

    #[test]
    fn repeated_sounds_appear_once() {
        let page = parse(&format!("{HTML}{HTML}"), BASE_URL, 1);

        assert_eq!(page.sounds.len(), 2);
    }

    #[test]
    fn knows_whether_there_is_another_page() {
        assert!(parse(HTML, BASE_URL, 1).has_more);
        assert!(!parse(HTML, BASE_URL, 2).has_more);
    }

    #[test]
    fn a_page_without_sounds_is_empty() {
        assert!(parse(EMPTY, BASE_URL, 1).sounds.is_empty());
    }
}
