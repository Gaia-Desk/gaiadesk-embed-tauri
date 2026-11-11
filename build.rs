const COMMANDS: &[&str] = &["start_help", "stop", "set_paused", "set_masks", "status"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
