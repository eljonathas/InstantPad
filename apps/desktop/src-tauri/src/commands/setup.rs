use instantpad_domain::hotkeys::{Hotkey, NumericScheme};
use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub async fn load_devices(core: State<'_, Core>) -> Result<(), ()> {
    core.setup.load_devices().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn choose_microphone(core: State<'_, Core>, id: Option<String>) -> Result<(), ()> {
    core.setup.choose_microphone(id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn choose_game_output(core: State<'_, Core>, id: Option<String>) -> Result<(), ()> {
    core.setup.choose_game_output(id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn choose_monitor_output(core: State<'_, Core>, id: Option<String>) -> Result<(), ()> {
    core.setup.choose_monitor_output(id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_numeric_scheme(core: State<'_, Core>, scheme: NumericScheme) {
    core.setup.set_numeric_scheme(scheme);
}

#[tauri::command]
#[specta::specta]
pub fn set_stop_hotkey(core: State<'_, Core>, hotkey: Hotkey) {
    core.setup.set_stop_hotkey(hotkey);
}

#[tauri::command]
#[specta::specta]
pub fn set_one_sound_at_a_time(core: State<'_, Core>, enabled: bool) {
    core.setup.set_one_sound_at_a_time(enabled);
}

#[tauri::command]
#[specta::specta]
pub fn set_language(core: State<'_, Core>, language: Option<String>) {
    core.setup.set_language(language);
}

#[tauri::command]
#[specta::specta]
pub async fn install_driver(core: State<'_, Core>) -> Result<(), ()> {
    core.setup.install_driver().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn open_sound_settings(core: State<'_, Core>) {
    core.setup.open_sound_settings();
}
