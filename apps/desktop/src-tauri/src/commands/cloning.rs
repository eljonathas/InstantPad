use tauri::State;

use crate::composition::Core;

#[tauri::command(async)]
#[specta::specta]
pub fn open_cloning(core: State<'_, Core>) {
    core.cloning.open();
}

#[tauri::command(async)]
#[specta::specta]
pub fn start_clone_sample(core: State<'_, Core>) {
    core.cloning.start_sample();
}

#[tauri::command]
#[specta::specta]
pub async fn finish_clone_sample(core: State<'_, Core>) -> Result<(), ()> {
    core.cloning.finish_sample().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn import_clone_sample(core: State<'_, Core>, path: String) -> Result<(), ()> {
    core.cloning.import_sample(&path).await;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn save_cloned_voice(core: State<'_, Core>, name: String) {
    core.cloning.save(&name);
}

#[tauri::command(async)]
#[specta::specta]
pub fn close_cloning(core: State<'_, Core>) {
    core.cloning.close();
}
