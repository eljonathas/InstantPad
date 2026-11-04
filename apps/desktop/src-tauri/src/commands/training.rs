use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub async fn open_training(core: State<'_, Core>, clone_id: String) -> Result<(), ()> {
    core.training.open(&clone_id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn accept_training(core: State<'_, Core>) -> Result<(), ()> {
    core.training.accept().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn start_training_take(core: State<'_, Core>) {
    core.training.start_take();
}

#[tauri::command]
#[specta::specta]
pub async fn finish_training_take(core: State<'_, Core>) -> Result<(), ()> {
    core.training.finish_take().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn import_training_take(core: State<'_, Core>, path: String) -> Result<(), ()> {
    core.training.import_take(&path).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn remove_training_take(core: State<'_, Core>, id: String) {
    core.training.remove_take(&id);
}

/// Resolves when the training ends, is stopped or fails, which can take a day.
#[tauri::command]
#[specta::specta]
pub async fn train_voice(core: State<'_, Core>) -> Result<(), ()> {
    core.training.train().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn stop_training(core: State<'_, Core>) {
    core.training.stop();
}

#[tauri::command]
#[specta::specta]
pub fn discard_training(core: State<'_, Core>) {
    core.training.discard();
}

#[tauri::command]
#[specta::specta]
pub fn close_training(core: State<'_, Core>) {
    core.training.close();
}

#[tauri::command]
#[specta::specta]
pub fn remove_training_tools(core: State<'_, Core>) {
    core.training.remove_tools();
}
