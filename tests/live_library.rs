//! Loads the REAL prebuilt library (downloaded and SHA-256 checked by
//! gaiadesk-embed-fetch into target/gaiadesk-live, or GAIADESK_EMBED_LIBRARY)
//! and checks what it does before touching the network (beyond a local
//! stand-in server) or the screen.

use std::path::PathBuf;
use std::ptr;

use serde_json::Value;
use tauri_plugin_gaiadesk_support::consent;
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

#[test]
fn the_consent_step_is_there_from_0_4_and_refuses_with_codes() {
    let l = lib();
    if !l.supports_consent() {
        // An older library: the plugin answers `unsupported` without calling it.
        assert_eq!(l.preview("{}").unwrap_err().code, "unsupported");
        return;
    }
    assert_eq!(
        l.record_consent_ex(r#"{"decision":"accepted"}"#)
            .unwrap_err()
            .code,
        "bad_config"
    );
    let input = r#"{"decision":"declined","choices":{"guided":false,"share_logs":false,"capture":"window"},
        "wording":{"title":"Share with Acme?"},"locale":"en","company":"Acme","capture":"window","app_version":null}"#;
    let d = l.record_consent_ex(input).unwrap();
    assert_eq!(d["decision"], "declined");
    assert_eq!(d["receipt"]["surface"], "embedded");
    assert!(d.get("consent_token").is_none());
    // SAFETY: no callback, so no user data.
    let bad = unsafe { l.consent(r#"{"nope":1}"#, None, ptr::null_mut()) };
    assert_eq!(bad.unwrap_err().code, "bad_config");
    if cfg!(target_os = "macos") {
        // Test threads are never the main thread.
        let off = unsafe { l.consent("{}", None, ptr::null_mut()) };
        assert_eq!(off.unwrap_err().code, "wrong_thread");
    }
}

// ---- The consent step end to end (GD-31): the plugin's `record_consent`
// path (`consent::record_input` → the library) for accept, decline and
// timeout, the receipt it returns being exactly what is posted to the
// session, and that receipt riding `start` back into the library. The JS
// half (the real <gaiadesk-consent> with consentBridge) is
// guest-js/test/consent-e2e.test.mjs.

/// One request a local stand-in for the server got: request line, headers (lowercase), body.
type Request = (String, Vec<(String, String)>, String);

/// A one-route HTTP server on 127.0.0.1 answering 201 to everything.
fn stand_in_server() -> (String, std::sync::mpsc::Receiver<Request>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut headers = Vec::new();
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                let Some((k, v)) = h.trim_end().split_once(':') else {
                    break;
                };
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
            let len = headers
                .iter()
                .find(|(k, _)| k == "content-length")
                .map_or(0, |(_, v)| v.parse().unwrap());
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let reply = r#"{"object":"support_consent"}"#;
            let _ = write!(
                s,
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            let _ = tx.send((
                line.trim_end().to_string(),
                headers,
                String::from_utf8(body).unwrap(),
            ));
        }
    });
    (origin, rx)
}

/// api/openapi.yaml's `ConsentReceipt` (additionalProperties: false), by hand.
fn assert_receipt(r: &Value, decision: &str, surface: &str) {
    let o = r.as_object().expect("an object");
    let mut keys: Vec<&str> = o.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "app_name",
            "app_version",
            "at",
            "choices",
            "decision",
            "id",
            "locale",
            "os",
            "os_user",
            "sdk",
            "surface",
            "wording_sha256",
            "wording_version"
        ],
        "{r}"
    );
    let s = |k: &str| {
        r[k].as_str()
            .unwrap_or_else(|| panic!("{k} is a string: {r}"))
    };
    let id = s("id");
    assert!(
        id.len() == 29
            && id.starts_with("cr_")
            && id[3..].bytes().all(|b| b.is_ascii_alphanumeric()),
        "{id}"
    );
    assert_eq!(s("decision"), decision);
    assert!(s("at").len() <= 40 && s("at").ends_with('Z'), "{}", s("at"));
    assert_eq!(s("surface"), surface);
    assert_eq!(s("locale"), "en");
    assert!((1..=100).contains(&s("wording_version").len()));
    let sha = s("wording_sha256");
    assert!(
        sha.len() == 64
            && sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{sha}"
    );
    assert_eq!(
        r["choices"],
        serde_json::json!({ "guided": false, "share_logs": true, "capture": "window" })
    );
    assert!(
        r["os_user"].is_null()
            || r["os_user"]
                .as_str()
                .is_some_and(|u| u.len() <= 64 && !u.contains('/'))
    );
    assert_eq!(s("os"), std::env::consts::OS);
    assert!(r["app_name"].is_null() || r["app_name"].is_string());
    assert_eq!(
        r["app_version"], "2.4.1",
        "the app's version, filled by the plugin"
    );
    assert!(s("sdk").starts_with("libgaiadesk_embed/"), "{}", s("sdk"));
}

/// What `<gaiadesk-consent>` hands the bridge (`record_consent`'s `input`).
fn component_input(decision: &str) -> serde_json::Map<String, Value> {
    let v = serde_json::json!({
        "decision": decision,
        "choices": { "guided": false, "share_logs": true, "capture": "window" },
        "wording": { "title": "Share with Acme support?", "accept": "Share", "decline": "Not now" },
        "wording_version": "acme-e2e-1", "locale": "en", "company": "Acme", "capture": "window",
        "app_version": null,
    });
    v.as_object().unwrap().clone()
}

fn plugin_defaults(server: Option<String>) -> consent::Defaults {
    consent::Defaults {
        app_version: "2.4.1".into(),
        server,
        ..Default::default()
    }
}

#[test]
fn consent_accept_decline_and_timeout_post_the_receipt_they_return() {
    let l = lib();
    if !l.supports_consent() {
        return;
    }
    let (server, requests) = stand_in_server();
    let token = format!("gdemb_{}", "a".repeat(64));
    for decision in ["accepted", "declined", "timeout"] {
        let mut input = component_input(decision);
        // consentBridge with a session (createConsentBridge({ embedToken, sessionId })).
        input.insert("embed_token".into(), token.clone().into());
        input.insert("session_id".into(), "ss_e2etest".into());
        let json = consent::record_input(input, &plugin_defaults(Some(server.clone())));
        let d = l.record_consent_ex(&json).unwrap();
        assert_eq!(d["decision"], decision);
        assert_eq!(d["choices"], d["receipt"]["choices"]);
        assert_receipt(&d["receipt"], decision, "embedded");
        assert_eq!(d["receipt"]["wording_version"], "acme-e2e-1");
        assert_eq!(
            d["consent_token"]
                .as_str()
                .is_some_and(|t| t.starts_with("gdc_")),
            decision == "accepted",
            "a token only for a yes: {d}"
        );
        let (line, headers, body) = requests
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("the {decision} receipt was posted"));
        assert_eq!(
            line,
            "POST /v1/support/sessions/ss_e2etest/consent HTTP/1.1"
        );
        let h = |k: &str| {
            headers
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(h("authorization"), Some(format!("Bearer {token}").as_str()));
        assert_eq!(h("idempotency-key"), d["receipt"]["id"].as_str());
        let posted: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            posted, d["receipt"],
            "the receipt returned is the one stored"
        );
    }
}

#[test]
fn the_returned_receipt_rides_start_and_the_library_checks_it() {
    let l = lib();
    if !l.supports_consent() {
        return;
    }
    let record = |decision: &str| {
        l.record_consent_ex(&consent::record_input(
            component_input(decision),
            &plugin_defaults(None),
        ))
        .unwrap()
    };
    let yes = record("accepted");
    // startHelp({ guided: true, consentToken, consentReceipt }) → the library's start config.
    let start = |d: &Value| {
        let o: HelpOptions = serde_json::from_value(serde_json::json!({
            "guided": true, "consentToken": d["consent_token"], "consentReceipt": d["receipt"],
        }))
        .unwrap();
        let brand = Brand {
            company: "Acme".into(),
            ..Default::default()
        };
        let token = format!("gdemb_{}", "a".repeat(64));
        options::start_config(&token, None, &o, &brand, &[], l.supports_branding())
    };
    let cfg: Value = serde_json::from_str(&start(&yes)).unwrap();
    assert_eq!(
        cfg["consent_receipt"], yes["receipt"],
        "the receipt, untouched"
    );
    assert_eq!(cfg["consent_token"], yes["consent_token"]);
    assert_eq!(
        (&cfg["capture"], &cfg["guided"]),
        (&serde_json::json!("window"), &serde_json::json!(false)),
        "what the customer chose: view only"
    );
    // The library reads that receipt at start: a decline, a timeout or a forged one is refused
    // before anything connects. (An accepted one would start a session, so it is not started here.)
    for d in [record("declined"), record("timeout")] {
        let e = l.start(&start(&d), NO_CB()).unwrap_err();
        assert_eq!(e.code, "consent_required", "{e}");
    }
    let mut forged = yes.clone();
    forged["receipt"]["id"] = "cr_forged".into();
    assert_eq!(
        l.start(&start(&forged), NO_CB()).unwrap_err().code,
        "consent_required"
    );
}
