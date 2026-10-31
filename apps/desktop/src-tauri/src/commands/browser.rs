use instantpad_application::library::SoundPage;
use instantpad_application::message::Failure;
use instantpad_domain::sounds::SoundSource;
use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub async fn search(core: State<'_, Core>, query: String, page: u32) -> Result<SoundPage, Failure> {
    core.library.search(&query, page).await
}

#[tauri::command]
#[specta::specta]
pub async fn trending(core: State<'_, Core>, page: u32) -> Result<SoundPage, Failure> {
    core.library.trending(page).await
}

#[tauri::command]
#[specta::specta]
pub async fn preview_sound(core: State<'_, Core>, source: SoundSource) -> Result<(), ()> {
    core.rows.preview(source).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn play_in_game(core: State<'_, Core>, source: SoundSource) -> Result<(), ()> {
    core.rows.play_in_game(source).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn add_to_pad(
    core: State<'_, Core>,
    source: SoundSource,
    slot: Option<u32>,
) -> Result<(), ()> {
    core.rows.add_to_pad(source, slot).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_favorite(core: State<'_, Core>, source: SoundSource) -> Result<(), ()> {
    core.rows.toggle_favorite(source).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn save_to_computer(
    core: State<'_, Core>,
    source: SoundSource,
    destination: String,
) -> Result<(), ()> {
    core.rows.save_to_computer(source, &destination).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn suggested_file_name(source: SoundSource) -> String {
    source.suggested_file_name()
}
