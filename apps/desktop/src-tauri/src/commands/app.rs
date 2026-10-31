use instantpad_application::events::{Limits, StartView};
use tauri::{AppHandle, Manager, State};

use crate::composition::Core;

/// Called once the interface knows its language. The rest loads in the background.
#[tauri::command]
#[specta::specta]
pub fn start(app: AppHandle, core: State<'_, Core>, default_interface_name: String) -> StartView {
    core.board.start(&default_interface_name);

    core.background_work.call_once(|| {
        tauri::async_runtime::spawn(async move {
            let core = app.state::<Core>();
            tokio::join!(
                core.board.preload_sounds(),
                core.updates.check_and_download(),
            );
        });
    });

    StartView {
        board: core.board.view(),
        mixer: core.mixer.view(),
        setup: core.setup.view(),
        favorites: core.favorites.list(),
        limits: Limits::current(),
    }
}
