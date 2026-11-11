include!("src/keys.rs");

const COMMANDS: &[&str] = &[
    "start_help",
    "stop",
    "set_paused",
    "set_masks",
    "status",
    "get_info",
    "screen_permission",
    "request_screen_permission",
    "open_screen_permission_settings",
    "rate",
    "attach_diagnostics",
    "send_diagnostics",
    "discard_diagnostics",
    "listen",
    "stop_listening",
    "decline_request",
];

fn main() {
    // The Tauri CLI hands us the app's `plugins."gaiadesk-support"` (tauri.conf.json, platform
    // files and --config merged): a secret key there would ship in the bundle, so no build.
    let config = tauri_plugin::plugin_config::<serde_json::Value>("gaiadesk-support");
    let key = config
        .as_ref()
        .and_then(|c| c.get("publishableKey")?.as_str());
    if key.is_some_and(is_secret_key) {
        panic!("tauri.conf.json plugins.\"gaiadesk-support\".publishableKey: {SECRET_KEY_IN_APP}");
    }
    tauri_plugin::Builder::new(COMMANDS).build();
}
