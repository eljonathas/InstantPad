use instantpad_domain::trending::{
    TRENDING_REGIONS, is_trending_region, region_of_locale, trending_region,
};

#[test]
fn the_country_of_a_locale_is_found_whatever_the_separator() {
    assert_eq!(region_of_locale("pt-BR").as_deref(), Some("br"));
    assert_eq!(region_of_locale("en_US").as_deref(), Some("us"));
    assert_eq!(region_of_locale("zh-Hant-TW").as_deref(), Some("tw"));
    assert_eq!(region_of_locale("de_DE.UTF-8").as_deref(), Some("de"));
}

#[test]
fn a_locale_without_a_country_has_none() {
    assert_eq!(region_of_locale("pt"), None);
    assert_eq!(region_of_locale("es-419"), None);
    assert_eq!(region_of_locale(""), None);
}

#[test]
fn the_chosen_country_wins_over_the_system() {
    assert_eq!(trending_region(Some("fr"), "pt-BR"), "fr");
}

#[test]
fn with_no_choice_the_trending_follows_the_system() {
    assert_eq!(trending_region(None, "pt-BR"), "br");
    assert_eq!(trending_region(None, "pt"), "us");
}

#[test]
fn a_country_that_cannot_be_chosen_is_ignored() {
    assert_eq!(trending_region(Some("../search"), "en-GB"), "gb");
    assert!(!is_trending_region("xx"));
}

#[test]
fn every_offered_country_is_a_plain_two_letter_code() {
    for region in TRENDING_REGIONS {
        assert_eq!(region.len(), 2);
        assert!(region.chars().all(|letter| letter.is_ascii_lowercase()));
        assert!(is_trending_region(region));
    }
}
