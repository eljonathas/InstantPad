use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

const PAD_COUNT: u32 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct HotkeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl HotkeyModifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        ..Self::NONE
    };
    pub const ALT: Self = Self {
        alt: true,
        ..Self::NONE
    };
}

/// A key combination. `key` is a `KeyboardEvent.code`, such as `Numpad1` or `KeyA`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct Hotkey {
    pub key: String,
    pub modifiers: HotkeyModifiers,
}

impl Hotkey {
    pub fn new(key: impl Into<String>, modifiers: HotkeyModifiers) -> Self {
        Self {
            key: key.into(),
            modifiers,
        }
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let modifiers = [
            (self.modifiers.ctrl, "Ctrl+"),
            (self.modifiers.alt, "Alt+"),
            (self.modifiers.shift, "Shift+"),
            (self.modifiers.meta, "Win+"),
        ];
        for (_, label) in modifiers.iter().filter(|(active, _)| *active) {
            formatter.write_str(label)?;
        }
        formatter.write_str(&key_label(&self.key))
    }
}

fn key_label(key: &str) -> String {
    match key {
        "NumpadMultiply" => "Num*".to_owned(),
        "NumpadAdd" => "Num+".to_owned(),
        "NumpadSubtract" => "Num-".to_owned(),
        "NumpadDecimal" => "Num,".to_owned(),
        "NumpadDivide" => "Num/".to_owned(),
        "Equal" => "=".to_owned(),
        "Minus" => "-".to_owned(),
        "Period" => ".".to_owned(),
        _ => match key.strip_prefix("Numpad") {
            Some(digit) => format!("Num{digit}"),
            None => key
                .strip_prefix("Digit")
                .or_else(|| key.strip_prefix("Key"))
                .unwrap_or(key)
                .to_owned(),
        },
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NumericScheme {
    #[default]
    Numpad,
    CtrlNumpad,
    AltDigits,
    Off,
}

/// Keys for the first pads of the active interface and for moving between interfaces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NumericLayout {
    pub pads: Vec<Hotkey>,
    pub next_interface: Option<Hotkey>,
    pub previous_interface: Option<Hotkey>,
    pub stop_all: Option<Hotkey>,
    pub toggle_voice: Option<Hotkey>,
    pub next_voice: Option<Hotkey>,
}

impl NumericLayout {
    pub fn for_scheme(scheme: NumericScheme) -> Self {
        match scheme {
            NumericScheme::Numpad => numpad_layout(HotkeyModifiers::NONE),
            NumericScheme::CtrlNumpad => numpad_layout(HotkeyModifiers::CTRL),
            NumericScheme::AltDigits => Self {
                pads: pad_keys("Digit", HotkeyModifiers::ALT),
                next_interface: Some(Hotkey::new("Equal", HotkeyModifiers::ALT)),
                previous_interface: Some(Hotkey::new("Minus", HotkeyModifiers::ALT)),
                stop_all: Some(Hotkey::new("Period", HotkeyModifiers::ALT)),
                toggle_voice: Some(Hotkey::new("KeyV", HotkeyModifiers::ALT)),
                next_voice: Some(Hotkey::new("KeyB", HotkeyModifiers::ALT)),
            },
            NumericScheme::Off => Self::default(),
        }
    }
}

fn numpad_layout(modifiers: HotkeyModifiers) -> NumericLayout {
    NumericLayout {
        pads: pad_keys("Numpad", modifiers),
        next_interface: Some(Hotkey::new("NumpadAdd", modifiers)),
        previous_interface: Some(Hotkey::new("NumpadSubtract", modifiers)),
        stop_all: Some(Hotkey::new("NumpadDecimal", modifiers)),
        toggle_voice: Some(Hotkey::new("NumpadMultiply", modifiers)),
        next_voice: Some(Hotkey::new("NumpadDivide", modifiers)),
    }
}

// Keyboard order: 1 to 9, then 0 as the tenth pad.
fn pad_keys(prefix: &str, modifiers: HotkeyModifiers) -> Vec<Hotkey> {
    (1..=PAD_COUNT)
        .map(|number| Hotkey::new(format!("{prefix}{}", number % PAD_COUNT), modifiers))
        .collect()
}
