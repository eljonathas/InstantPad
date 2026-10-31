use instantpad_domain::board::InterfaceLayout;
use instantpad_domain::hotkeys::Hotkey;
use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub async fn press_pad(core: State<'_, Core>, slot: u32) -> Result<(), ()> {
    core.board.press_pad(slot).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn select_interface(core: State<'_, Core>, id: String) {
    core.board.select_interface(&id);
}

#[tauri::command]
#[specta::specta]
pub fn step_interface(core: State<'_, Core>, offset: i32) {
    core.board.step_interface(offset);
}

#[tauri::command]
#[specta::specta]
pub fn add_interface(core: State<'_, Core>, layout: InterfaceLayout) {
    core.board.add_interface(layout);
}

#[tauri::command]
#[specta::specta]
pub fn configure_interface(core: State<'_, Core>, id: String, layout: InterfaceLayout) {
    core.board.configure_interface(&id, layout);
}

#[tauri::command]
#[specta::specta]
pub fn move_interface(core: State<'_, Core>, id: String, offset: i32) {
    core.board.move_interface(&id, offset);
}

#[tauri::command]
#[specta::specta]
pub fn delete_interface(core: State<'_, Core>, id: String) {
    core.board.delete_interface(&id);
}

#[tauri::command]
#[specta::specta]
pub async fn import_file(core: State<'_, Core>, path: String, slot: u32) -> Result<(), ()> {
    core.board.import_file(&path, slot).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn swap_pads(core: State<'_, Core>, from_slot: u32, to_slot: u32) {
    core.board.swap_pads(from_slot, to_slot);
}

#[tauri::command]
#[specta::specta]
pub fn send_to_interface(core: State<'_, Core>, slot: u32, target_id: String) {
    core.board.send_to_interface(slot, &target_id);
}

#[tauri::command]
#[specta::specta]
pub fn rename_pad(core: State<'_, Core>, slot: u32, name: String) {
    core.board.rename(slot, &name);
}

#[tauri::command]
#[specta::specta]
pub fn set_pad_volume(core: State<'_, Core>, slot: u32, volume: f64) {
    core.board.set_volume(slot, volume);
}

#[tauri::command]
#[specta::specta]
pub fn set_pad_color(core: State<'_, Core>, slot: u32, color: String) {
    core.board.set_color(slot, &color);
}

#[tauri::command]
#[specta::specta]
pub fn set_pad_hotkey(core: State<'_, Core>, slot: u32, hotkey: Hotkey) {
    core.board.set_hotkey(slot, hotkey);
}

#[tauri::command]
#[specta::specta]
pub fn clear_pad_hotkey(core: State<'_, Core>, slot: u32) {
    core.board.clear_hotkey(slot);
}

#[tauri::command]
#[specta::specta]
pub fn remove_pad(core: State<'_, Core>, slot: u32) {
    core.board.remove(slot);
}

#[tauri::command]
#[specta::specta]
pub fn stop_all(core: State<'_, Core>) {
    core.board.stop_all();
}

/// Global keys swallow their presses, so they stay off while a text field or dialog is open.
#[tauri::command]
#[specta::specta]
pub fn pause_hotkeys(core: State<'_, Core>) {
    core.hotkeys.pause();
}

#[tauri::command]
#[specta::specta]
pub fn resume_hotkeys(core: State<'_, Core>) {
    core.hotkeys.resume();
}
