use instantpad_domain::hotkeys::{Hotkey, HotkeyModifiers, NumericLayout, NumericScheme};
use instantpad_domain::palette::luminance;
use instantpad_domain::sounds::{SoundSource, safe_file_name};

fn label(hotkey: &Option<Hotkey>) -> String {
    hotkey.as_ref().unwrap().to_string()
}

#[test]
fn numeric_schemes_cover_ten_pads_and_navigation() {
    let cases = [
        (
            NumericScheme::Numpad,
            ["Num1", "Num0", "Num+", "Num-", "Num,"],
        ),
        (
            NumericScheme::CtrlNumpad,
            [
                "Ctrl+Num1",
                "Ctrl+Num0",
                "Ctrl+Num+",
                "Ctrl+Num-",
                "Ctrl+Num,",
            ],
        ),
        (
            NumericScheme::AltDigits,
            ["Alt+1", "Alt+0", "Alt+=", "Alt+-", "Alt+."],
        ),
    ];

    for (scheme, [first, last, next, previous, stop]) in cases {
        let layout = NumericLayout::for_scheme(scheme);

        assert_eq!(layout.pads.len(), 10);
        assert_eq!(layout.pads[0].to_string(), first);
        assert_eq!(layout.pads[9].to_string(), last);
        assert_eq!(label(&layout.next_interface), next);
        assert_eq!(label(&layout.previous_interface), previous);
        assert_eq!(label(&layout.stop_all), stop);
    }
}

#[test]
fn the_off_scheme_has_no_keys() {
    let layout = NumericLayout::for_scheme(NumericScheme::Off);

    assert!(layout.pads.is_empty());
    assert!(layout.stop_all.is_none());
}

#[test]
fn hotkeys_read_like_on_the_keyboard() {
    let ctrl_alt = HotkeyModifiers {
        ctrl: true,
        alt: true,
        ..HotkeyModifiers::NONE
    };
    let shift = HotkeyModifiers {
        shift: true,
        ..HotkeyModifiers::NONE
    };

    assert_eq!(Hotkey::new("F12", ctrl_alt).to_string(), "Ctrl+Alt+F12");
    assert_eq!(Hotkey::new("KeyA", shift).to_string(), "Shift+A");
    assert_eq!(
        Hotkey::new("Numpad0", HotkeyModifiers::NONE).to_string(),
        "Num0"
    );
    assert_eq!(
        Hotkey::new("Home", HotkeyModifiers::NONE).to_string(),
        "Home"
    );
}

#[test]
fn luminance_reads_the_hex_forms_the_site_uses() {
    let cases = [
        ("#000000", 0.0),
        ("#ffffff", 1.0),
        ("#fff", 1.0),
        ("#ff000000", 0.0),
        ("invalida", 0.0),
        ("", 0.0),
    ];

    for (hex, expected) in cases {
        assert!((luminance(hex) - expected).abs() < 0.001, "{hex}");
    }
}

#[test]
fn favorites_match_paths_ignoring_case() {
    let source = SoundSource {
        name: "a".into(),
        color: "#fff".into(),
        path: "C:/Sons/A.mp3".into(),
        sound_url: None,
    };

    assert!(source.is_same_sound_as("c:/sons/a.MP3"));
    assert!(!source.is_same_sound_as("c:/sons/b.mp3"));
}

#[test]
fn file_names_lose_what_windows_refuses() {
    assert_eq!(safe_file_name("som: a/b?.mp3"), "som_ a_b_.mp3");
}

#[test]
fn suggested_names_come_from_the_sound_name() {
    let source = SoundSource {
        name: " vine: boom? ".into(),
        color: "#fff".into(),
        path: "C:/Sons/x1.mp3".into(),
        sound_url: None,
    };

    assert_eq!(source.suggested_file_name(), "vine_ boom_.mp3");
}

#[test]
fn the_numpad_schemes_have_keys_for_the_voice() {
    let layout = NumericLayout::for_scheme(NumericScheme::Numpad);

    assert_eq!(label(&layout.toggle_voice), "Num*");
    assert_eq!(label(&layout.next_voice), "Num/");
}
