//! Loads the REAL prebuilt library (downloaded and SHA-256 checked by
//! gaiadesk-embed-fetch into target/gaiadesk-live, or GAIADESK_EMBED_LIBRARY)
//! and checks what it does before touching the network or the screen.

use std::path::PathBuf;
use std::ptr;

use serde_json::Value;
use tauri_plugin_gaiadesk_support::ffi::{GdEmbedCallbacks, Lib};
use tauri_plugin_gaiadesk_support::options::{self, Brand, HelpOptions};

fn lib() -> &'static Lib {
    // Once per process: the tests run in parallel and would download into one directory.
    static LIB: std::sync::OnceLock<&'static Lib> = std::sync::OnceLock::new();
    LIB.get_or_init(load)
}

fn load() -> &'static Lib {
    let path = match std::env::var_os("GAIADESK_EMBED_LIBRARY") {
        Some(p) => PathBuf::from(p),
        None => {
            let asset =
                gaiadesk_embed_fetch::asset_for(std::env::consts::OS, std::env::consts::ARCH)
                    .expect("a prebuilt library for this platform");
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/gaiadesk-live");
            gaiadesk_embed_fetch::fetch_for(asset, &dir).expect("download")
        }
    };
    Lib::load(&path).expect("load")
}

const NO_CB: fn() -> GdEmbedCallbacks = || GdEmbedCallbacks {
    on_event: None,
    user_data: ptr::null_mut(),
};

#[test]
fn build_info_is_a_release_build_for_this_platform() {
    let l = lib();
    let info: Value = serde_json::from_str(&l.build_info()).unwrap();
    assert_eq!(info["test_source"], Value::Bool(false), "{info}");
    assert_eq!(info["os"].as_str(), Some(std::env::consts::OS), "{info}");
    assert_eq!(info["version"].as_str(), Some(l.version().as_str()));
    assert!(!l.version().is_empty());
}

#[test]
fn refusals_come_back_as_codes() {
    let l = lib();
    let token = format!("gdemb_{}", "a".repeat(64));
    let no_consent = format!(r#"{{"embed_token":"{token}","consent_granted":false}}"#);
    assert_eq!(
        l.start(&no_consent, NO_CB()).unwrap_err().code,
        "consent_required"
    );
    assert_eq!(
        l.start(
            r#"{"embed_token":"gdemb_x","consent_granted":true}"#,
            NO_CB()
        )
        .unwrap_err()
        .code,
        "bad_token"
    );
    let bad_server = format!(
        r#"{{"embed_token":"{token}","consent_granted":true,"server":"http://example.com"}}"#
    );
    assert_eq!(
        l.start(&bad_server, NO_CB()).unwrap_err().code,
        "bad_server"
    );
}

#[test]
fn branding_keys_are_sent_only_to_a_library_that_knows_them() {
    let l = lib();
    let brand = Brand {
        company: "Acme".into(),
        color: Some("#0f766e".into()),
        logo_png_base64: None,
    };
    // A bad token, so nothing starts either way: what matters is which refusal.
    let cfg = options::start_config(
        "gdemb_x",
        None,
        &HelpOptions::default(),
        &brand,
        &[],
        l.supports_branding(),
    );
    assert_eq!(
        l.start(&cfg, NO_CB()).unwrap_err().code,
        "bad_token",
        "the plugin's config must pass the library's key check"
    );
    if !l.supports_branding() {
        // 0.1.0 refuses unknown keys: this is why the plugin strips them.
        let forced =
            options::start_config("gdemb_x", None, &HelpOptions::default(), &brand, &[], true);
        let token = format!("gdemb_{}", "a".repeat(64));
        let forced = forced.replace("gdemb_x", &token);
        assert_eq!(l.start(&forced, NO_CB()).unwrap_err().code, "bad_config");
    }
}
