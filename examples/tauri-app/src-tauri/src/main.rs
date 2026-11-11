// Hides the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_gaiadesk_support::init())
        .run(tauri::generate_context!())
        .expect("error while running Acme Desk");
}
