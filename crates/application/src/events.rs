use instantpad_domain::hotkeys::{Hotkey, NumericScheme};
use instantpad_domain::sounds::SoundSource;
use instantpad_domain::{board, palette};
use serde::Serialize;
use specta::Type;

use crate::message::Message;
use crate::ports::AudioDevice;
use crate::settings::AppSettings;

// `#[specta(type = u32)]` exports a float as a plain `number`. Specta adds `| null` for NaN,
// which these values never are.

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundView {
    pub id: String,
    pub name: String,
    pub path: String,
    pub color: String,
    #[specta(type = u32)]
    pub volume: f64,
    pub has_hotkey: bool,
    /// A fully lit pad of this color needs dark text.
    pub is_light: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PadView {
    pub slot: u32,
    pub label: String,
    pub shortcut: String,
    pub is_favorite: bool,
    /// A sound is on its way to this pad.
    pub is_loading: bool,
    pub sound: Option<SoundView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceTab {
    pub id: String,
    pub name: String,
    pub rows: u32,
    pub columns: u32,
    pub accent: String,
    pub sound_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoardView {
    pub interfaces: Vec<InterfaceTab>,
    pub active_id: String,
    pub pads: Vec<PadView>,
    /// The empty pad waiting for a sound.
    pub target_slot: Option<u32>,
    /// Text of the stop key, or `None` when there is none.
    pub stop_shortcut: Option<String>,
    /// The color a new interface starts with.
    pub next_accent: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupView {
    pub microphones: Vec<AudioDevice>,
    pub game_outputs: Vec<AudioDevice>,
    pub monitor_outputs: Vec<AudioDevice>,
    pub microphone_id: Option<String>,
    pub game_output_id: Option<String>,
    pub monitor_output_id: Option<String>,
    pub has_cable: bool,
    pub is_system_output_on_cable: bool,
    pub game_microphone_name: String,
    pub language: Option<String>,
    pub numeric_scheme: NumericScheme,
    /// First pad, last pad, next, previous and stop keys of the scheme. Empty when it is off.
    pub numeric_hint_keys: Vec<String>,
    pub stop_hotkey: Option<Hotkey>,
    pub one_sound_at_a_time: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MixerView {
    #[specta(type = u32)]
    pub mic_volume: f64,
    #[specta(type = u32)]
    pub game_volume: f64,
    #[specta(type = u32)]
    pub monitor_volume: f64,
    pub mic_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    pub status: Message,
    pub is_ready_to_restart: bool,
}

/// The LCD at the top of the device.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DisplayEvent {
    Show { message: Message },
    Error { message: Message },
    Routing { status: Message, hint: Message },
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum AppEvent {
    Board(BoardView),
    Favorites(Vec<SoundSource>),
    Setup(SetupView),
    DriverStatus(Message),
    Update(UpdateView),
    Display(DisplayEvent),
}

impl AppEvent {
    pub fn show(message: Message) -> Self {
        Self::Display(DisplayEvent::Show { message })
    }

    pub fn error(message: Message) -> Self {
        Self::Display(DisplayEvent::Error { message })
    }
}

/// What the meters and pads show on one frame.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    #[specta(type = u32)]
    pub mic_level: f32,
    #[specta(type = u32)]
    pub output_level: f32,
    pub playing_slots: Vec<u32>,
}

/// What the rules allow, so the interface offers only that.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub pad_colors: Vec<String>,
    pub accent_colors: Vec<String>,
    pub max_rows: u32,
    pub max_columns: u32,
    pub default_rows: u32,
    pub default_columns: u32,
    /// Where a double click puts each knob.
    pub default_mixer: MixerView,
}

impl Limits {
    pub fn current() -> Self {
        let defaults = AppSettings::default();
        Self {
            pad_colors: palette::PADS.map(str::to_owned).to_vec(),
            accent_colors: palette::ACCENTS.map(str::to_owned).to_vec(),
            max_rows: board::MAX_ROWS,
            max_columns: board::MAX_COLUMNS,
            default_rows: board::DEFAULT_ROWS,
            default_columns: board::DEFAULT_COLUMNS,
            default_mixer: MixerView {
                mic_volume: defaults.mic_volume,
                game_volume: defaults.game_volume,
                monitor_volume: defaults.monitor_volume,
                mic_enabled: defaults.mic_enabled,
            },
        }
    }
}

/// Everything the interface needs to draw its first frame.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartView {
    pub board: BoardView,
    pub mixer: MixerView,
    pub setup: SetupView,
    pub favorites: Vec<SoundSource>,
    pub limits: Limits,
}
