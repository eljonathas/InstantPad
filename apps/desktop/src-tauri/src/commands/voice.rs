use std::path::PathBuf;

use instantpad_domain::cloning::VoiceRecipe;
use instantpad_domain::voice::{VoiceEffect, VoiceEngine, VoiceParam};
use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub fn select_voice_effect(core: State<'_, Core>, effect: VoiceEffect) {
    core.voice.select(effect);
}

#[tauri::command]
#[specta::specta]
pub fn select_cloned_voice(core: State<'_, Core>, id: String) {
    core.voice.select_clone(&id);
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

#[tauri::command]
#[specta::specta]
pub async fn install_voice_ai(core: State<'_, Core>) -> Result<(), ()> {
    core.voice.install_ai().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn relearn_voice(core: State<'_, Core>) {
    core.voice.relearn();
}

#[tauri::command]
#[specta::specta]
pub fn delete_cloned_voice(core: State<'_, Core>, id: String) {
    core.voice.delete_clone(&id);
}

#[tauri::command]
#[specta::specta]
pub fn open_designer(core: State<'_, Core>) {
    core.voice.open_designer();
}

#[tauri::command]
#[specta::specta]
pub fn close_designer(core: State<'_, Core>) {
    core.voice.close_designer();
}

#[tauri::command]
#[specta::specta]
pub fn preview_designed_voice(core: State<'_, Core>, recipe: VoiceRecipe) {
    core.voice.preview_design(recipe);
}

#[tauri::command]
#[specta::specta]
pub fn end_voice_preview(core: State<'_, Core>) {
    core.voice.end_preview();
}

#[tauri::command]
#[specta::specta]
pub fn save_designed_voice(core: State<'_, Core>, name: String, recipe: VoiceRecipe) {
    core.voice.save_design(&name, recipe);
}

/// Adds the voice the training kit exported to `folder` to an AI voice.
#[tauri::command]
#[specta::specta]
pub async fn add_trained_model(
    core: State<'_, Core>,
    effect: VoiceEffect,
    clone_id: Option<String>,
    folder: String,
) -> Result<(), ()> {
    core.voice
        .add_trained_model(effect, clone_id, PathBuf::from(folder))
        .await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn remove_trained_model(core: State<'_, Core>, effect: VoiceEffect, clone_id: Option<String>) {
    core.voice.remove_trained_model(effect, clone_id);
}

#[tauri::command]
#[specta::specta]
pub fn set_voice_engine(
    core: State<'_, Core>,
    effect: VoiceEffect,
    clone_id: Option<String>,
    engine: VoiceEngine,
) {
    core.voice.set_engine(effect, clone_id, engine);
}
