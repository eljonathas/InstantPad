//! Which country's trending sounds the browser shows.

/// The countries offered in the settings, each with its own trending page in the catalog.
pub const TRENDING_REGIONS: [&str; 55] = [
    "ar", "au", "be", "bo", "br", "ca", "ch", "cl", "cn", "co", "cz", "de", "dk", "ec", "eg", "es",
    "fi", "fr", "gb", "gr", "hu", "id", "ie", "il", "in", "it", "jp", "kr", "ma", "mx", "my", "ng",
    "nl", "no", "nz", "pe", "ph", "pk", "pl", "pt", "py", "ro", "ru", "sa", "se", "sg", "th", "tr",
    "tw", "ua", "us", "uy", "ve", "vn", "za",
];

/// For a system whose language names no country.
const FALLBACK_REGION: &str = "us";

/// The country of a locale such as `pt-BR`, `en_US` or `zh-Hant-TW`, in lower case.
pub fn region_of_locale(locale: &str) -> Option<String> {
    // The language comes first; the country is the first later part of two letters.
    locale
        .split(['-', '_', '.'])
        .skip(1)
        .find(|part| part.len() == 2 && part.chars().all(|letter| letter.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase)
}

/// The country whose trending sounds show: the one chosen in the settings, or else the
/// system's.
pub fn trending_region(chosen: Option<&str>, system_locale: &str) -> String {
    // A settings file edited by hand may hold anything.
    chosen
        .filter(|region| is_trending_region(region))
        .map(str::to_owned)
        .or_else(|| region_of_locale(system_locale))
        .unwrap_or_else(|| FALLBACK_REGION.to_owned())
}

/// Only the countries offered in the settings can be chosen.
pub fn is_trending_region(region: &str) -> bool {
    TRENDING_REGIONS.contains(&region)
}
