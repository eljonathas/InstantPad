//! Mounts the app: plugins, shared state and the commands the interface can call.

mod commands;
mod composition;
mod events;
mod hotkeys;
mod updater;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent, Wry};
use tauri_specta::{Builder, Event, collect_commands, collect_events};

use crate::composition::Core;
use crate::events::{BackendEvent, MeterEvent};
use crate::hotkeys::GlobalHotkeys;

const METER_INTERVAL: Duration = Duration::from_millis(33);

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
            commands::setup::set_one_sound_at_a_time,
            commands::setup::set_press_again_to_stop,
            commands::setup::set_language,
            commands::setup::install_driver,
            commands::setup::open_sound_settings,
            commands::updates::restart_now,
            commands::voice::select_voice_effect,
            commands::voice::set_voice_param,
            commands::voice::set_hearing_voice,
        ])
        .events(collect_events![BackendEvent, MeterEvent])
        .disable_serde_phases()
}

/// Levels and lit pads reach the interface on their own thread, far from the audio callbacks.
fn start_meters(app: AppHandle) {
    thread::spawn(move || {
        loop {
            let frame = app.state::<Core>().meter_frame();
            // Only fails while the window is closing, when nobody is listening.
            let _ = MeterEvent(frame).emit(&app);
            thread::sleep(METER_INTERVAL);
        }
    });
}

/// Opening the app again brings the running window to the front.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn run() {
    let builder = specta_builder();

    tauri::Builder::default()
        // First, so a second copy leaves before it touches the devices or the hotkeys.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(hotkeys::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let hotkeys = Arc::new(GlobalHotkeys::new(app.handle().clone()));
            app.manage(hotkeys.clone());
            app.manage(composition::build(app.handle(), hotkeys)?);
            start_meters(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("build the app")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                let core = app.state::<Core>();
                core.shut_down();
                core.updates.apply_on_exit();
            }
        });
}

// Not on Windows: a test binary that links Tauri needs an app manifest to load there, and the
// bindings come out the same on every system.
#[cfg(all(test, not(windows)))]
mod tests {
    use std::path::{Path, PathBuf};
    use std::{env, fs};

    use specta_typescript::Typescript;

    use super::specta_builder;

    fn export(path: &Path) {
        specta_builder()
            .export(Typescript::default(), path)
            .expect("export the TypeScript bindings");
    }

    /// Fails when the committed bindings drifted from the commands and events.
    /// Run with `UPDATE_BINDINGS=1` to rewrite them.
    #[test]
    fn the_typescript_bindings_are_current() {
        let committed =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/shared/ipc/bindings.ts");

        if env::var_os("UPDATE_BINDINGS").is_some() {
            export(&committed);
            return;
        }

        let generated = env::temp_dir().join(format!("instantpad-{}.ts", std::process::id()));
        export(&generated);
        let current = fs::read_to_string(&generated).expect("read the generated bindings");
        fs::remove_file(&generated).expect("remove the generated bindings");

        assert_eq!(
            fs::read_to_string(&committed).expect("read the committed bindings"),
            current,
            "run `pnpm gen:ipc` in apps/desktop",
        );
    }
}
