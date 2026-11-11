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
    /// `https://` URL or a `data:image/png;base64,` URL. At most 256 KiB.
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

/// `POST /v1/support/sessions`'s body. Pure.
pub fn session_body(o: &HelpOptions) -> String {
    let mut b = json!({ "mode": o.mode.unwrap_or_default().as_str() });
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
        let b: Value = serde_json::from_str(&session_body(&opts(json!({ "mode": "cobrowse", "customer": { "name": "Ada", "plan": "pro" }, "expiresIn": 900 })))).unwrap();
        assert_eq!(
            b,
            json!({ "mode": "cobrowse", "customer": { "name": "Ada", "plan": "pro" }, "expires_in": 900 })
        );
        assert_eq!(session_body(&HelpOptions::default()), r#"{"mode":"view"}"#);
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
