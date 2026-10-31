use instantpad_domain::voice::{VoiceEffect, VoiceParam};
use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub fn select_voice_effect(core: State<'_, Core>, effect: VoiceEffect) {
    core.voice.select(effect);
}

#[tauri::command]
#[specta::specta]
pub fn set_voice_param(core: State<'_, Core>, param: VoiceParam, value: f64) {
    core.voice.set_param(param, value);
}

#[tauri::command]
#[specta::specta]
pub fn set_hearing_voice(core: State<'_, Core>, enabled: bool) {
    core.voice.set_hearing(enabled);
}
