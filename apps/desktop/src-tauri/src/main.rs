//! `DeskHop.exe`: optional Tauri shell. Tray, autostart, updater, pipe client.
//!
//! Nothing changes when it is closed. No engine logic here.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run DeskHop");
}
