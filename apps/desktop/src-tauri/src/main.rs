// Keeps a console window from opening with the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    instantpad_desktop::run();
}
