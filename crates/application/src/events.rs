use instantpad_domain::hotkeys::{Hotkey, NumericScheme};
use instantpad_domain::sounds::SoundSource;
use serde::Serialize;
use specta::Type;

use crate::message::Message;
use crate::ports::AudioDevice;

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
    pub stop_hotkey: Option<Hotkey>,
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

/// Everything the interface needs to draw its first frame.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartView {
    pub board: BoardView,
    pub mixer: MixerView,
    pub setup: SetupView,
    pub favorites: Vec<SoundSource>,
}
