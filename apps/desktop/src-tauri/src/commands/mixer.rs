use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub fn set_mic_volume(core: State<'_, Core>, volume: f64) {
    core.mixer.set_mic_volume(volume);
}

#[tauri::command]
#[specta::specta]
pub fn set_game_volume(core: State<'_, Core>, volume: f64) {
    core.mixer.set_game_volume(volume);
}

#[tauri::command]
#[specta::specta]
pub fn set_monitor_volume(core: State<'_, Core>, volume: f64) {
    core.mixer.set_monitor_volume(volume);
}

#[tauri::command]
#[specta::specta]
pub fn set_mic_enabled(core: State<'_, Core>, enabled: bool) {
    core.mixer.set_mic_enabled(enabled);
}
