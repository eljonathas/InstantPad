// Keeps a console window from opening with the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = instantpad_desktop::run_installer_step(std::env::args()) {
        std::process::exit(code);
    }
    instantpad_desktop::run();
}
