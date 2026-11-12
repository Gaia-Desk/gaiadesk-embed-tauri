//! The consent step (GD-31, libgaiadesk_embed 0.4.0): `consent` (the
//! library's dialog), `preview`, `record_consent` (the embedded web
//! component's or your own UI's answer, made a receipt) and `wait_info`.
//! Options and decisions keep the library's snake_case JSON, so the same
//! object works here and in `@gaiadesk/consent`.

use std::collections::BTreeMap;
use std::ffi::{c_char, c_void, CStr};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::commands::{blocking, brand_of, library_rects, native_window};
use crate::error::{Error, Result};
use crate::events::PREFIX;
use crate::options::{api_base, Brand, Capture, CssRect, HelpOptions};
use crate::{http, Support};

/// The consent dialog's look (§1 `theme`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentTheme {
    /// `light`, `dark` or `auto`.
    pub appearance: Option<String>,
    /// `#RRGGBB` each.
    pub background: Option<String>,
    pub surface: Option<String>,
    pub text: Option<String>,
    pub accent: Option<String>,
    /// px, 0..24.
    pub radius: Option<u32>,
    pub font_family: Option<String>,
    /// `filled`, `outline` or `pill`.
    pub button_style: Option<String>,
}

/// What the customer may choose (§1 `choices`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentChoices {
    pub offer_guided: Option<bool>,
    pub offer_logs: Option<bool>,
    pub offer_capture: Option<bool>,
}

/// The consent options (§1): every key optional, the library's own names.
/// Unlike the library's, `masks` are CSS pixels of the calling page, and
/// `company`, `parent_window`, the brand, `app_version` and (with an
/// `embed_token`) `server` default to the plugin's.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentOptions {
    pub company: Option<String>,
    pub capture: Option<Capture>,
    pub guided: Option<bool>,
    pub masked: Option<bool>,
    pub brand_color: Option<String>,
    pub logo_png_base64: Option<String>,
    pub parent_window: Option<u64>,
    pub theme: Option<ConsentTheme>,
    pub locale: Option<String>,
    pub wording: Option<BTreeMap<String, String>>,
    pub wording_version: Option<String>,
    pub choices: Option<ConsentChoices>,
    pub privacy_url: Option<String>,
    pub terms_url: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub preview: Option<bool>,
    pub server: Option<String>,
    pub embed_token: Option<String>,
    pub app_version: Option<String>,
    pub session_id: Option<String>,
    pub window_id: Option<u64>,
    pub masks: Option<Vec<CssRect>>,
}

/// What is shared and whether guided input is on: the receipt's choices
/// when there is one (the library refuses anything else), else `o`'s. Pure.
pub fn granted(o: &HelpOptions) -> (&str, bool) {
    let c = o.consent_receipt.as_ref().and_then(|r| r.get("choices"));
    let capture = c
        .and_then(|c| c.get("capture")?.as_str())
        .filter(|c| ["window", "app", "screen"].contains(c))
        .unwrap_or(o.capture.as_str());
    let guided = o.guided && c.map_or(true, |c| c.get("guided") == Some(&Value::Bool(true)));
    (capture, guided)
}

/// What the plugin fills in where the app left a key out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defaults {
    pub brand: Brand,
    pub parent_window: Option<u64>,
    pub app_version: String,
    /// The plugin config's `server`.
    pub server: Option<String>,
}

/// The library's server origin for a plugin `server` (the API's, without `/v1`). Pure.
pub fn server_origin(server: Option<&str>) -> String {
    api_base(server).trim_end_matches("/v1").to_string()
}

/// `gd_embed_consent`'s JSON: `o` with the defaults and the masks in library
/// units (`masks`). Null keys are left out (the library refuses them). Pure.
pub fn library_options(
    o: &ConsentOptions,
    d: &Defaults,
    masks: Option<Vec<crate::ffi::GdRect>>,
) -> String {
    let mut o = o.clone();
    o.company = o.company.or_else(|| Some(d.brand.company.clone()));
    o.brand_color = o.brand_color.or_else(|| d.brand.color.clone());
    o.logo_png_base64 = o
        .logo_png_base64
        .or_else(|| d.brand.logo_png_base64.clone());
    o.parent_window = o.parent_window.or(d.parent_window);
    o.app_version = o
        .app_version
        .or_else(|| Some(d.app_version.clone()).filter(|v| !v.is_empty()));
    if o.embed_token.is_some() && o.server.is_none() {
        o.server = Some(server_origin(d.server.as_deref()));
    }
    o.masks = masks.filter(|m| !m.is_empty());
    let mut v = serde_json::to_value(o).expect("options");
    drop_nulls(&mut v);
    v.to_string()
}

/// `gd_embed_record_consent_ex`'s JSON: the component's input, with this
/// app's version when it has none and the session's server when it names
/// one. Pure.
pub fn record_input(mut input: Map<String, Value>, d: &Defaults) -> String {
    if matches!(input.get("app_version"), None | Some(Value::Null)) && !d.app_version.is_empty() {
        input.insert("app_version".into(), d.app_version.clone().into());
    }
    if input.get("embed_token").is_some_and(Value::is_string)
        && matches!(input.get("server"), None | Some(Value::Null))
    {
        input.insert("server".into(), server_origin(d.server.as_deref()).into());
    }
    let mut v = Value::Object(input);
    drop_nulls(&mut v);
    v.to_string()
}

fn drop_nulls(v: &mut Value) {
    if let Value::Object(m) = v {
        m.retain(|_, x| !x.is_null());
        m.values_mut().for_each(drop_nulls);
    }
}

/// §4's request, as the web component's bridge sends it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PreviewRequest {
    pub capture: Option<Capture>,
    pub parent_window: Option<u64>,
    pub window_id: Option<u64>,
    /// CSS pixels of the calling page.
    pub masks: Option<Vec<CssRect>>,
}

/// One `consent_*` library event → its Tauri event name and payload (the
/// event without `type`). Pure.
pub fn event(json: &str) -> Option<(String, Value)> {
    let mut v: Value = serde_json::from_str(json).ok()?;
    let name = v.as_object_mut()?.remove("type")?;
    let name = name.as_str().filter(|n| n.starts_with("consent_"))?;
    Some((format!("{PREFIX}{name}"), v))
}

fn defaults<R: Runtime>(app: &AppHandle<R>, window: Option<&WebviewWindow<R>>) -> Result<Defaults> {
    let sup = app.state::<Support>();
    Ok(Defaults {
        brand: brand_of(app, &sup, &HelpOptions::default())?,
        parent_window: window.and_then(native_window),
        app_version: app.package_info().version.to_string(),
        server: sup.config.server.clone(),
    })
}

unsafe extern "C" fn on_event<R: Runtime>(user: *mut c_void, json: *const c_char) {
    let _ = std::panic::catch_unwind(|| {
        // SAFETY: the AppHandle `consent` keeps alive for the (blocking) call.
        let app = unsafe { &*(user as *const AppHandle<R>) };
        let json = unsafe { CStr::from_ptr(json) }.to_string_lossy();
        if let Some((name, payload)) = event(&json) {
            let _ = app.emit(&name, payload);
        }
    });
}

/// The library's consent dialog; resolves with §2's decision. Events:
/// `consent_shown`, `consent_accepted`, `consent_declined`.
#[tauri::command]
pub(crate) async fn consent<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    options: Option<ConsentOptions>,
) -> Result<Value> {
    let o = options.unwrap_or_default();
    blocking(move || {
        let lib = app.state::<Support>().lib(&app)?;
        let masks = o.masks.as_ref().map(|m| library_rects(&window, m));
        let json = library_options(&o, &defaults(&app, Some(&window))?, masks);
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = app.clone();
        // A modal dialog: on macOS it must be the main thread.
        app.run_on_main_thread(move || {
            let user = Box::into_raw(Box::new(handle));
            // SAFETY: `user` is the AppHandle on_event reads, freed only after the call.
            let r = unsafe { lib.consent(&json, Some(on_event::<R>), user as *mut c_void) };
            // SAFETY: the call has returned; the library holds no callback.
            drop(unsafe { Box::from_raw(user) });
            let _ = tx.send(r);
        })
        .map_err(|e| Error::new("wrong_thread", e.to_string()))?;
        rx.recv().unwrap_or_else(|_| {
            Err(Error::new(
                "wrong_thread",
                "the main thread did not run the dialog",
            ))
        })
    })
    .await
}

/// One frame of what support would see (§4), or `capture_unavailable`.
#[tauri::command]
pub(crate) async fn preview<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    options: Option<PreviewRequest>,
) -> Result<Value> {
    let mut o = options.unwrap_or_default();
    blocking(move || {
        let lib = app.state::<Support>().lib(&app)?;
        o.parent_window = o.parent_window.or_else(|| native_window(&window));
        let rects = o.masks.take().map(|m| library_rects(&window, &m));
        let mut v = serde_json::to_value(&o).expect("request");
        if let Some(r) = rects.filter(|r| !r.is_empty()) {
            v["masks"] = serde_json::to_value(r).expect("rects");
        }
        drop_nulls(&mut v);
        lib.preview(&v.to_string())
    })
    .await
}

/// The embedded (or your own) consent UI's answer, made a receipt by the
/// library: §2's decision (`surface: "embedded"`). With `embed_token` and
/// `session_id` the library also posts it (a decline ends the request).
#[tauri::command]
pub(crate) async fn record_consent<R: Runtime>(
    app: AppHandle<R>,
    input: Map<String, Value>,
) -> Result<Value> {
    blocking(move || {
        let lib = app.state::<Support>().lib(&app)?;
        lib.record_consent_ex(&record_input(input, &defaults(&app, None)?))
    })
    .await
}

/// Who is waiting (§5): the support team and its typical wait.
#[tauri::command]
pub(crate) async fn wait_info<R: Runtime>(
    app: AppHandle<R>,
    embed_token: String,
    session_id: String,
) -> Result<Value> {
    blocking(move || {
        if !session_id.starts_with("ss_")
            || !session_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(Error::new(
                "bad_options",
                "sessionId must be a support session id (ss_…)",
            ));
        }
        let api = app.state::<Support>().api();
        http::get_json(
            &format!("{api}/support/sessions/{session_id}/wait"),
            &embed_token,
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::GdRect;
    use serde_json::json;

    fn defaults() -> Defaults {
        Defaults {
            brand: Brand {
                company: "Acme".into(),
                color: Some("#0f766e".into()),
                logo_png_base64: None,
            },
            parent_window: Some(7),
            app_version: "2.4.1".into(),
            server: None,
        }
    }

    fn help(js: Value) -> HelpOptions {
        serde_json::from_value(js).unwrap()
    }

    #[test]
    fn a_receipt_rides_the_start_and_its_choices_win() {
        let receipt = json!({ "id": "cr_x", "decision": "accepted", "choices": { "guided": false, "share_logs": true, "capture": "screen" } });
        let o = help(
            json!({ "guided": true, "capture": "window", "consentToken": "gdc_1", "consentReceipt": receipt }),
        );
        let c: Value = serde_json::from_str(&crate::options::start_config(
            "gdemb_x",
            None,
            &o,
            &Brand::default(),
            &[],
            false,
        ))
        .unwrap();
        assert_eq!(
            (&c["capture"], &c["guided"]),
            (&json!("screen"), &json!(false)),
            "the customer chose view only, the screen"
        );
        assert_eq!(
            (&c["consent_token"], &c["consent_receipt"]),
            (&json!("gdc_1"), &receipt)
        );
        let yes = help(
            json!({ "guided": true, "consentReceipt": { "choices": { "guided": true, "capture": "window" } } }),
        );
        assert_eq!(granted(&yes), ("window", true));
        let unasked =
            help(json!({ "guided": false, "consentReceipt": { "choices": { "guided": true } } }));
        assert_eq!(
            granted(&unasked),
            ("window", false),
            "a choice never turns on what the app did not ask for"
        );
        assert_eq!(
            granted(&help(json!({ "guided": true, "capture": "app" }))),
            ("app", true)
        );
    }

    #[test]
    fn options_take_the_plugins_defaults_and_drop_nulls() {
        let o: ConsentOptions = serde_json::from_value(json!({
            "guided": true, "theme": { "accent": "#5b4cf0" }, "choices": { "offer_logs": true },
            "masks": [{ "x": 1, "y": 2, "w": 3, "h": 4 }]
        }))
        .unwrap();
        let rects = vec![GdRect {
            x: 2.0,
            y: 4.0,
            w: 6.0,
            h: 8.0,
        }];
        let v: Value =
            serde_json::from_str(&library_options(&o, &defaults(), Some(rects))).unwrap();
        assert_eq!(
            v,
            json!({ "company": "Acme", "guided": true, "brand_color": "#0f766e", "parent_window": 7,
                    "theme": { "accent": "#5b4cf0" }, "choices": { "offer_logs": true }, "app_version": "2.4.1",
                    "masks": [{ "x": 2.0, "y": 4.0, "w": 6.0, "h": 8.0 }] })
        );
    }

    #[test]
    fn the_apps_own_values_win_and_a_token_brings_the_server() {
        let o: ConsentOptions = serde_json::from_value(json!({
            "company": "Beta", "parent_window": 9, "app_version": "1.0", "capture": "screen",
            "embed_token": "gdemb_x", "session_id": "ss_1"
        }))
        .unwrap();
        let v: Value =
            serde_json::from_str(&library_options(&o, &defaults(), Some(vec![]))).unwrap();
        assert_eq!(
            (&v["company"], &v["parent_window"], &v["app_version"]),
            (&json!("Beta"), &json!(9), &json!("1.0"))
        );
        assert_eq!(v["capture"], json!("screen"));
        assert_eq!(v["server"], json!("https://api.gaiadesk.net"));
        assert!(v.get("masks").is_none(), "no masks: no key");
        let d = Defaults {
            server: Some("http://127.0.0.1:9000".into()),
            ..defaults()
        };
        let v: Value = serde_json::from_str(&library_options(&o, &d, None)).unwrap();
        assert_eq!(v["server"], json!("http://127.0.0.1:9000"));
    }

    #[test]
    fn options_are_strict() {
        assert!(
            serde_json::from_value::<ConsentOptions>(json!({ "consentGranted": true })).is_err()
        );
        assert!(
            serde_json::from_value::<ConsentOptions>(json!({ "theme": { "colour": "#fff" } }))
                .is_err()
        );
        assert!(serde_json::from_value::<ConsentOptions>(json!({ "capture": "desktop" })).is_err());
        assert_eq!(
            serde_json::from_value::<ConsentOptions>(json!({})).unwrap(),
            ConsentOptions::default()
        );
    }

    #[test]
    fn record_input_gets_the_app_version_and_server() {
        let input = json!({ "decision": "declined", "choices": { "guided": false, "share_logs": false, "capture": "window" },
                            "wording": { "title": "t" }, "app_version": null, "embed_token": "gdemb_x", "session_id": "ss_1" });
        let v: Value = serde_json::from_str(&record_input(
            input.as_object().unwrap().clone(),
            &defaults(),
        ))
        .unwrap();
        assert_eq!(
            (&v["app_version"], &v["server"]),
            (&json!("2.4.1"), &json!("https://api.gaiadesk.net"))
        );
        let own = json!({ "decision": "accepted", "app_version": "9" });
        let v: Value =
            serde_json::from_str(&record_input(own.as_object().unwrap().clone(), &defaults()))
                .unwrap();
        assert_eq!(
            v,
            json!({ "decision": "accepted", "app_version": "9" }),
            "no token: no server"
        );
    }

    #[test]
    fn consent_events_are_forwarded_others_are_not() {
        assert_eq!(
            event(r#"{"type":"consent_shown","locale":"en","wording_version":"v"}"#),
            Some((
                "gaiadesk-support:consent_shown".into(),
                json!({ "locale": "en", "wording_version": "v" })
            ))
        );
        assert_eq!(
            event(r#"{"type":"consent_declined","reason":"timeout"}"#)
                .unwrap()
                .1,
            json!({ "reason": "timeout" })
        );
        assert_eq!(event(r#"{"type":"joined"}"#), None);
        assert_eq!(event("nope"), None);
    }

    #[test]
    fn server_origin_is_the_apis() {
        assert_eq!(server_origin(None), "https://api.gaiadesk.net");
        assert_eq!(
            server_origin(Some("https://gaiadesk.net")),
            "https://api.gaiadesk.net"
        );
        assert_eq!(
            server_origin(Some("http://localhost:9000/")),
            "http://localhost:9000"
        );
    }
}
