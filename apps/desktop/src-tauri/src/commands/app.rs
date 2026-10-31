use instantpad_application::events::StartView;
use tauri::State;

use crate::composition::Core;

/// Called once the interface knows its language. Devices load in the background.
#[tauri::command]
#[specta::specta]
pub fn start(core: State<'_, Core>, default_interface_name: String) -> StartView {
    core.board.start(&default_interface_name);

    StartView {
        board: core.board.view(),
        mixer: core.mixer.view(),
        setup: core.setup.view(),
        favorites: core.favorites.list(),
    }
}
