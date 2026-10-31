use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub fn restart_now(core: State<'_, Core>) {
    // On Windows the installer ends the process before the app gets to save on exit.
    core.shut_down();
    core.updates.restart_now();
}
