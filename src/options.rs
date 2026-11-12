//! `startHelp`'s options → the library's JSON and the session request. Pure:
//! the library checks every value itself and refuses with a code; this only
//! spells it right.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::{Error, Result};
use crate::ffi::GdRect;

/// What is shared.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capture {
    /// This app's window (the default).
    #[default]
    Window,
    /// Every window of this app (macOS; elsewhere the focused one, followed).
    App,
    /// The whole display the app's window is on (opt-in).
    Screen,
}

impl Capture {
    pub fn as_str(self) -> &'static str {
        match self {
            Capture::Window => "window",
            Capture::App => "app",
            Capture::Screen => "screen",
        }
    }
}

/// What the support team may do (asked for when the plugin creates the
/// session with a publishable key; with an embed token your backend chose).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    View,
    Cobrowse,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::View => "view",
            Mode::Cobrowse => "cobrowse",
        }
    }
}

/// The "Sharing with …" indicator's corner (default: top right, clear of your window).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IndicatorPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl IndicatorPosition {
    pub fn as_str(self) -> &'static str {
        match self {
            IndicatorPosition::TopLeft => "top-left",
            IndicatorPosition::TopRight => "top-right",
            IndicatorPosition::BottomLeft => "bottom-left",
            IndicatorPosition::BottomRight => "bottom-right",
        }
    }
}

/// Your company's look on the consent dialog and the indicator.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Branding {
    /// "Sharing with <name>" (default: the plugin config's `company`, then
    /// the app's product name).
    pub name: Option<String>,
    /// `#rrggbb` (or `#rgb`).
    pub color: Option<String>,
    /// A PNG: a path (relative ones are in the app's resources), an
    /// `https://` URL, a `data:image/png;base64,` URL, or `$appIcon` (the
    /// app's window icon). Bigger than 256 KiB: downscaled. A logo that
    /// cannot be used is a `warning` event, never a refusal.
    pub logo: Option<String>,
}

/// A rect in CSS pixels, relative to the top-left of the app's page.
pub type CssRect = GdRect;

/// `startHelp`'s options, as JS sends them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelpOptions {
    /// Overrides the plugin config's publishable key.
    pub publishable_key: Option<String>,
    /// From your backend (`POST /v1/support/sessions`). Wins over a key.
    pub embed_token: Option<String>,
    /// A session request (`sr_…`) heard by `listen`: accepted after the
    /// customer agreed, and its session started. Wins over a key.
    pub request_id: Option<String>,
    /// Who the customer is (publishable-key flow): flat, at most 16 fields.
    pub customer: Option<Map<String, Value>>,
    pub mode: Option<Mode>,
    /// Seconds (publishable-key flow; 60 to 3600).
    pub expires_in: Option<u32>,
    /// Allow guided input (needs a `cobrowse` session too).
    #[serde(default)]
    pub guided: bool,
    #[serde(default)]
    pub capture: Capture,
    pub branding: Option<Branding>,
    /// Masked from the first frame, in CSS pixels of the calling page.
    #[serde(default)]
    pub masks: Vec<CssRect>,
    /// `true`: the app asked its user itself and they agreed; the native
    /// dialog is skipped. Never pass it without asking.
    #[serde(default)]
    pub consent_granted: bool,
    /// Frames per second, 1 to 30.
    pub fps: Option<u32>,
    /// The indicator's corner. The customer can drag it; where they put it
    /// is remembered for this app (`indicator.json` in the app's data dir).
    pub indicator_position: Option<IndicatorPosition>,
    /// Set by the JS side: masking is on (a selector, the default, or rects),
    /// so the consent dialog can say private fields stay hidden.
    #[serde(default)]
    pub masking: bool,
}

/// What a library from 0.3.0 also takes (an older one refuses unknown keys):
/// the calling window, the indicator's corner and remembered place, and
/// whether masking is on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Native {
    /// `NSWindow*` (macOS) or `HWND` (Windows) of the window that called.
    pub parent_window: Option<u64>,
    pub masked: Option<bool>,
    pub indicator_position: Option<IndicatorPosition>,
    pub indicator_origin: Option<(f64, f64)>,
}

/// `json` (a start config or consent options) with `n`'s keys. Pure.
pub fn with_native(json: &str, n: &Native) -> String {
    let mut c: Value = serde_json::from_str(json).expect("our own JSON");
    let m = c.as_object_mut().expect("object");
    if let Some(p) = n.parent_window {
        m.insert("parent_window".into(), p.into());
    }
    if let Some(b) = n.masked {
        m.insert("masked".into(), b.into());
    }
    if let Some(p) = n.indicator_position {
        m.insert("indicator_position".into(), p.as_str().into());
    }
    if let Some((x, y)) = n.indicator_origin.filter(|(x, y)| x.is_finite() && y.is_finite()) {
        m.insert("indicator_origin".into(), json!({ "x": x, "y": y }));
    }
    c.to_string()
}

/// Branding, checked and ready for the library.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Brand {
    pub company: String,
    pub color: Option<String>,
    pub logo_png_base64: Option<String>,
}

/// `#rrggbb` (lowercase) from `#rrggbb` or `#rgb`. Pure.
pub fn brand_color(c: &str) -> Result<String> {
    let h = c
        .trim()
        .strip_prefix('#')
        .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
    match h {
        Some(h) if h.len() == 6 => Ok(format!("#{}", h.to_ascii_lowercase())),
        Some(h) if h.len() == 3 => Ok(format!(
            "#{}",
            h.chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase()
        )),
        _ => Err(Error::new(
            "bad_color",
            format!("branding.color must be #rrggbb (not {c:?})"),
        )),
    }
}

/// `gd_embed_start`'s JSON. Branding keys only when the library knows them
/// (`branded`): an older library refuses unknown keys. Pure.
pub fn start_config(
    token: &str,
    server: Option<&str>,
    o: &HelpOptions,
    brand: &Brand,
    masks: &[GdRect],
    branded: bool,
) -> String {
    let mut c = json!({
        "embed_token": token,
        "consent_granted": true,
        "company": brand.company,
        "capture": o.capture.as_str(),
        "guided": o.guided,
    });
    let m = c.as_object_mut().expect("object");
    if let Some(s) = server {
        m.insert("server".into(), s.into());
    }
    if let Some(f) = o.fps {
        m.insert("fps".into(), f.into());
    }
    if !masks.is_empty() {
        m.insert("masks".into(), serde_json::to_value(masks).expect("rects"));
    }
    if branded {
        add_brand(m, brand);
    }
    c.to_string()
}

/// `gd_embed_show_consent_ex`'s JSON. Pure.
pub fn consent_options(o: &HelpOptions, brand: &Brand) -> String {
    let mut c =
        json!({ "company": brand.company, "capture": o.capture.as_str(), "guided": o.guided });
    add_brand(c.as_object_mut().expect("object"), brand);
    c.to_string()
}

fn add_brand(m: &mut Map<String, Value>, brand: &Brand) {
    if let Some(c) = &brand.color {
        m.insert("brand_color".into(), c.clone().into());
    }
    if let Some(l) = &brand.logo_png_base64 {
        m.insert("logo_png_base64".into(), l.clone().into());
    }
}

/// The `Origin` a publishable key registered for this app must match:
/// `app://<identifier>`, lowercased. Pure.
pub fn native_origin(identifier: &str) -> String {
    format!("app://{}", identifier.trim().to_ascii_lowercase())
}

/// The platform API for a server: `https://api.gaiadesk.net/v1` by default,
/// else `<server>/v1`. Pure.
pub fn api_base(server: Option<&str>) -> String {
    match server.map(|s| s.trim().trim_end_matches('/')) {
        None | Some("https://gaiadesk.net") => "https://api.gaiadesk.net/v1".to_string(),
        Some(s) => format!("{s}/v1"),
    }
}

/// What the app says about itself (`client` on a create or an accept): the
/// OS, its version when known, the app's version and this plugin. Values the
/// server would refuse (empty, over 64, not printable ASCII) are left out. Pure.
pub fn client_info(os: &str, os_version: Option<&str>, app_version: &str) -> Value {
    let fields = [
        ("os", Some(os)),
        ("os_version", os_version),
        ("app_version", Some(app_version)),
        ("sdk", Some("tauri-plugin-gaiadesk-support")),
        ("sdk_version", Some(env!("CARGO_PKG_VERSION"))),
    ];
    let ok =
        |v: &&str| !v.is_empty() && v.len() <= 64 && v.bytes().all(|b| (0x20..0x7f).contains(&b));
    Value::Object(
        fields
            .into_iter()
            .filter_map(|(k, v)| v.filter(ok).map(|v| (k.to_string(), Value::from(v))))
            .collect(),
    )
}

/// This OS's version, where it is cheap to read: macOS's SystemVersion.plist,
/// Linux's os-release. `None` elsewhere.
// shortcut: no Windows version (needs a registry or RtlGetVersion call); add one if agents ask.
pub fn os_version() -> Option<String> {
    let read = |p: &str| std::fs::read_to_string(p).ok();
    match std::env::consts::OS {
        "macos" => read("/System/Library/CoreServices/SystemVersion.plist")
            .and_then(|t| plist_string(&t, "ProductVersion")),
        "linux" => read("/etc/os-release").and_then(|t| os_release(&t, "VERSION_ID")),
        _ => None,
    }
}

/// `<key>k</key><string>v</string>` → v. Pure.
fn plist_string(text: &str, key: &str) -> Option<String> {
    let after = &text[text.find(&format!("<key>{key}</key>"))?..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")?;
    Some(after[start..start + end].trim().to_string())
}

/// `K="v"` (or `K=v`) → v. Pure.
fn os_release(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(|v| v.trim().trim_matches('"').to_string())
}

/// `POST /v1/support/sessions`'s body. Pure.
pub fn session_body(o: &HelpOptions, client: &Value) -> String {
    let mut b = json!({ "mode": o.mode.unwrap_or_default().as_str(), "client": client });
    if let Some(c) = &o.customer {
        b["customer"] = Value::Object(c.clone());
    }
    if let Some(e) = o.expires_in {
        b["expires_in"] = e.into();
    }
    b.to_string()
}

/// A CSS-pixel rect of the page → the library's units, relative to the
/// window frame's top-left: points on macOS, physical pixels on Windows
/// (`GetWindowRect`) and Linux (X11). `content_offset` is the page's origin
/// inside the frame and `scale` the window's scale factor, both from Tauri
/// (`inner_position - outer_position`, physical). Assumes page zoom 1. Pure.
pub fn to_library_rect(r: &CssRect, content_offset: (f64, f64), scale: f64, os: &str) -> GdRect {
    if os == "macos" {
        GdRect {
            x: content_offset.0 / scale + r.x,
            y: content_offset.1 / scale + r.y,
            w: r.w,
            h: r.h,
        }
    } else {
        GdRect {
            x: content_offset.0 + r.x * scale,
            y: content_offset.1 + r.y * scale,
            w: r.w * scale,
            h: r.h * scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(js: Value) -> HelpOptions {
        serde_json::from_value(js).unwrap()
    }

    fn brand() -> Brand {
        Brand {
            company: "Acme".into(),
            color: Some("#0f766e".into()),
            logo_png_base64: Some("iVBO".into()),
        }
    }

    #[test]
    fn start_config_spells_the_library_keys() {
        let o = opts(json!({ "guided": true, "capture": "screen", "fps": 10 }));
        let masks = [GdRect {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
        }];
        let c: Value = serde_json::from_str(&start_config(
            "gdemb_x",
            Some("http://localhost:9000"),
            &o,
            &brand(),
            &masks,
            true,
        ))
        .unwrap();
        assert_eq!(
            c,
            json!({
                "embed_token": "gdemb_x", "consent_granted": true, "company": "Acme", "capture": "screen", "guided": true,
                "server": "http://localhost:9000", "fps": 10, "masks": [{"x":1.0,"y":2.0,"w":3.0,"h":4.0}],
                "brand_color": "#0f766e", "logo_png_base64": "iVBO"
            })
        );
    }

    #[test]
    fn an_older_library_gets_no_branding_keys() {
        let c: Value = serde_json::from_str(&start_config(
            "gdemb_x",
            None,
            &HelpOptions::default(),
            &brand(),
            &[],
            false,
        ))
        .unwrap();
        assert_eq!(
            c,
            json!({ "embed_token": "gdemb_x", "consent_granted": true, "company": "Acme", "capture": "window", "guided": false })
        );
    }

    #[test]
    fn consent_options_carry_the_brand() {
        let c: Value =
            serde_json::from_str(&consent_options(&opts(json!({ "guided": true })), &brand()))
                .unwrap();
        assert_eq!(
            c,
            json!({ "company": "Acme", "capture": "window", "guided": true, "brand_color": "#0f766e", "logo_png_base64": "iVBO" })
        );
        let plain: Value = serde_json::from_str(&consent_options(
            &HelpOptions::default(),
            &Brand {
                company: "A".into(),
                ..Brand::default()
            },
        ))
        .unwrap();
        assert_eq!(
            plain,
            json!({ "company": "A", "capture": "window", "guided": false })
        );
    }

    #[test]
    fn a_new_library_gets_the_window_and_the_indicators_place() {
        let start = start_config("gdemb_x", None, &HelpOptions::default(), &brand(), &[], true);
        let n = Native {
            parent_window: Some(140_230_000),
            masked: None,
            indicator_position: Some(IndicatorPosition::BottomLeft),
            indicator_origin: Some((12.0, -40.5)),
        };
        let c: Value = serde_json::from_str(&with_native(&start, &n)).unwrap();
        assert_eq!(c["parent_window"], json!(140_230_000u64));
        assert_eq!(c["indicator_position"], json!("bottom-left"));
        assert_eq!(c["indicator_origin"], json!({ "x": 12.0, "y": -40.5 }));
        assert!(c.get("masked").is_none());
        let consent = with_native(
            &consent_options(&HelpOptions::default(), &brand()),
            &Native { parent_window: Some(7), masked: Some(true), ..Native::default() },
        );
        let c: Value = serde_json::from_str(&consent).unwrap();
        assert_eq!((c["parent_window"].clone(), c["masked"].clone()), (json!(7), json!(true)));
        assert_eq!(with_native(&start, &Native::default()), start, "nothing to add: unchanged");
        let o = opts(json!({ "indicatorPosition": "top-left", "masking": true }));
        assert_eq!((o.indicator_position, o.masking), (Some(IndicatorPosition::TopLeft), true));
        assert!(serde_json::from_value::<HelpOptions>(json!({ "indicatorPosition": "middle" })).is_err());
    }

    #[test]
    fn js_options_are_strict() {
        assert!(serde_json::from_value::<HelpOptions>(json!({ "capture": "desktop" })).is_err());
        assert!(serde_json::from_value::<HelpOptions>(json!({ "mode": "control" })).is_err());
        assert!(serde_json::from_value::<HelpOptions>(json!({ "embedTokn": "x" })).is_err());
        assert_eq!(opts(json!({ "capture": "app" })).capture, Capture::App);
        let o = opts(
            json!({ "embedToken": "gdemb_1", "branding": { "name": "Acme", "color": "#fff" }, "masks": [{"x":0,"y":0,"w":1,"h":1}] }),
        );
        assert_eq!(o.embed_token.as_deref(), Some("gdemb_1"));
        assert_eq!(o.masks.len(), 1);
    }

    #[test]
    fn colors() {
        assert_eq!(brand_color("#0F766E").unwrap(), "#0f766e");
        assert_eq!(brand_color(" #fA0 ").unwrap(), "#ffaa00");
        for bad in ["0f766e", "#0f766", "#ggg", "red", "#0f766e00"] {
            assert_eq!(brand_color(bad).unwrap_err().code, "bad_color", "{bad}");
        }
    }

    #[test]
    fn origin_is_the_lowercased_identifier() {
        assert_eq!(native_origin("com.Acme.Desk"), "app://com.acme.desk");
    }

    #[test]
    fn api_base_follows_the_server() {
        assert_eq!(api_base(None), "https://api.gaiadesk.net/v1");
        assert_eq!(
            api_base(Some("https://gaiadesk.net/")),
            "https://api.gaiadesk.net/v1"
        );
        assert_eq!(
            api_base(Some("http://127.0.0.1:9000")),
            "http://127.0.0.1:9000/v1"
        );
    }

    #[test]
    fn session_body_has_mode_customer_and_expiry() {
        let client = json!({ "os": "macos" });
        let b: Value = serde_json::from_str(&session_body(&opts(json!({ "mode": "cobrowse", "customer": { "name": "Ada", "plan": "pro" }, "expiresIn": 900 })), &client)).unwrap();
        assert_eq!(
            b,
            json!({ "mode": "cobrowse", "client": { "os": "macos" }, "customer": { "name": "Ada", "plan": "pro" }, "expires_in": 900 })
        );
        assert_eq!(
            session_body(&HelpOptions::default(), &client),
            r#"{"client":{"os":"macos"},"mode":"view"}"#
        );
    }

    #[test]
    fn client_info_names_the_app_and_drops_what_the_server_refuses() {
        let c = client_info("macos", Some("14.5"), "2.3.1");
        assert_eq!(
            c,
            json!({ "os": "macos", "os_version": "14.5", "app_version": "2.3.1",
                    "sdk": "tauri-plugin-gaiadesk-support", "sdk_version": env!("CARGO_PKG_VERSION") })
        );
        let c = client_info("linux", None, "");
        assert!(c.get("os_version").is_none() && c.get("app_version").is_none());
        assert!(client_info("x", Some(&"9".repeat(65)), "é")
            .get("os_version")
            .is_none());
        assert!(client_info("x", None, "é").get("app_version").is_none());
    }

    #[test]
    fn os_versions_are_read_from_the_system_files() {
        let plist = "<dict>\n\t<key>ProductName</key>\n\t<string>macOS</string>\n\t<key>ProductVersion</key>\n\t<string>14.5</string>\n</dict>";
        assert_eq!(
            plist_string(plist, "ProductVersion").as_deref(),
            Some("14.5")
        );
        assert_eq!(plist_string(plist, "Nope"), None);
        let osr = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nVERSION=\"24.04 LTS\"";
        assert_eq!(os_release(osr, "VERSION_ID").as_deref(), Some("24.04"));
        assert_eq!(
            os_release("VERSION_ID=12", "VERSION_ID").as_deref(),
            Some("12")
        );
    }

    #[test]
    fn css_rects_to_library_units() {
        let r = GdRect {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 30.0,
        };
        // macOS, Retina: a 28 pt title bar is 56 physical px above the page.
        assert_eq!(
            to_library_rect(&r, (0.0, 56.0), 2.0, "macos"),
            GdRect {
                x: 10.0,
                y: 48.0,
                w: 100.0,
                h: 30.0
            }
        );
        // Windows at 150 %: 8 px border, 31 px title bar.
        assert_eq!(
            to_library_rect(&r, (8.0, 31.0), 1.5, "windows"),
            GdRect {
                x: 23.0,
                y: 61.0,
                w: 150.0,
                h: 45.0
            }
        );
    }
}
