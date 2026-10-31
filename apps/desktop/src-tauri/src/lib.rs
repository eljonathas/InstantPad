//! Mounts the app: plugins, shared state and the commands the interface can call.

mod commands;
mod composition;
mod events;

use specta_typescript::Typescript;
use tauri::{Manager, Wry};
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::events::{BackendEvent, MeterEvent};

pub const BINDINGS_FILE: &str = "bindings.ts";

fn specta_builder() -> Builder<Wry> {
    Builder::<Wry>::new()
        .commands(collect_commands![
            commands::app::start,
            commands::board::press_pad,
            commands::board::select_interface,
            commands::board::step_interface,
            commands::board::add_interface,
            commands::board::configure_interface,
            commands::board::move_interface,
            commands::board::delete_interface,
            commands::board::import_file,
            commands::board::swap_pads,
            commands::board::send_to_interface,
            commands::board::rename_pad,
            commands::board::set_pad_volume,
            commands::board::set_pad_color,
            commands::board::set_pad_hotkey,
            commands::board::clear_pad_hotkey,
            commands::board::remove_pad,
            commands::board::stop_all,
            commands::board::pause_hotkeys,
            commands::board::resume_hotkeys,
            commands::browser::search,
            commands::browser::trending,
            commands::browser::preview_sound,
            commands::browser::play_in_game,
            commands::browser::add_to_pad,
            commands::browser::toggle_favorite,
            commands::browser::save_to_computer,
            commands::browser::suggested_file_name,
            commands::mixer::set_mic_volume,
            commands::mixer::set_game_volume,
            commands::mixer::set_monitor_volume,
            commands::mixer::set_mic_enabled,
            commands::setup::load_devices,
            commands::setup::choose_microphone,
            commands::setup::choose_game_output,
            commands::setup::choose_monitor_output,
            commands::setup::set_numeric_scheme,
            commands::setup::set_stop_hotkey,
            commands::setup::set_language,
            commands::setup::install_driver,
            commands::setup::open_sound_settings,
            commands::updates::restart_now,
        ])
        .events(collect_events![BackendEvent, MeterEvent])
        .disable_serde_phases()
}

/// Writes the TypeScript bindings of every command and event to `path`.
pub fn export_bindings(path: &std::path::Path) {
    specta_builder()
        .export(Typescript::default(), path)
        .expect("export the TypeScript bindings");
}

pub fn run() {
    let builder = specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(composition::build(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run the app");
}
