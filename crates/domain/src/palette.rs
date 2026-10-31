pub const NEUTRAL: &str = "#3a3f4b";

pub const PADS: [&str; 8] = [
    "#ff3b5c", "#ff8a1f", "#ffd21f", "#35f27a", "#1fd6ff", "#4f7dff", "#b45cff", "#ff5cc8",
];

pub const ACCENTS: [&str; 8] = [
    "#ffb03a", "#35f27a", "#1fd6ff", "#4f7dff", "#b45cff", "#ff5cc8", "#ff3b5c", "#e8e8ea",
];

const NEARLY_BLACK_LUMINANCE: f64 = 0.12;
const LIGHT_COLOR_LUMINANCE: f64 = 0.55;

pub fn pad_color_for(slot: u32) -> &'static str {
    PADS[slot as usize % PADS.len()]
}

pub fn accent_for(index: usize) -> &'static str {
    ACCENTS[index % ACCENTS.len()]
}

/// A nearly black pad would look switched off, so it takes a palette color instead.
pub fn visible_color(color: &str, slot: u32) -> String {
    if luminance(color) < NEARLY_BLACK_LUMINANCE {
        pad_color_for(slot).to_owned()
    } else {
        color.to_owned()
    }
}

/// Whether a fully lit pad of this color needs dark text.
pub fn is_light(color: &str) -> bool {
    luminance(color) > LIGHT_COLOR_LUMINANCE
}

/// Perceived brightness from 0 to 1. Colors that cannot be read count as black.
pub fn luminance(hex: &str) -> f64 {
    let Some((red, green, blue)) = parse(hex) else {
        return 0.0;
    };
    (0.2126 * f64::from(red) + 0.7152 * f64::from(green) + 0.0722 * f64::from(blue)) / 255.0
}

fn parse(hex: &str) -> Option<(u8, u8, u8)> {
    let digits = hex.trim_start_matches('#');

    // #RGB is shorthand for #RRGGBB, and the alpha of #AARRGGBB does not affect brightness.
    let digits: String = match digits.len() {
        3 => digits.chars().flat_map(|digit| [digit, digit]).collect(),
        6 => digits.to_owned(),
        8 => digits.get(2..)?.to_owned(),
        _ => return None,
    };

    Some((
        u8::from_str_radix(digits.get(..2)?, 16).ok()?,
        u8::from_str_radix(digits.get(2..4)?, 16).ok()?,
        u8::from_str_radix(digits.get(4..)?, 16).ok()?,
    ))
}
