use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub fn restart_now(core: State<'_, Core>) {
    core.updates.restart_now();
}
