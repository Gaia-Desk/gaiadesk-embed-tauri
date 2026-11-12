//! The JS commands: start_help, stop, set_paused, set_masks, status,
//! get_info and the screen-permission ones.

use std::ffi::{c_char, c_void, CStr};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::error::{Error, Result};
use crate::events::{self, Status};
use crate::ffi::{GdEmbedCallbacks, GdRect, Lib};
use crate::followup::{warn, Creds};
use crate::http::Created;
use crate::options::{self, Brand, CssRect, HelpOptions, Native};
use crate::{
    branding, http, indicator, keys, lock, permission, requests, Handle, Session, Support,
};

/// Ask, create the session if needed, start. Resolves once sharing has
/// started (events follow); rejects with `{code, message}`.
#[tauri::command]
pub(crate) async fn start_help<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    options: Option<HelpOptions>,
) -> Result<Status> {
    let o = options.unwrap_or_default();
    blocking(move || {
        let sup = app.state::<Support>();
        let keys = [
            o.publishable_key.as_deref(),
            sup.config.publishable_key.as_deref(),
        ];
        if keys.into_iter().flatten().any(keys::is_secret_key) {
            return Err(Error::new("secret_key_in_app", keys::SECRET_KEY_IN_APP));
        }
        let lib = sup.lib(&app)?;
        // Before any session exists: no agent ever joins one that cannot share.
        if permission::screen() != "granted" {
            let _ = app.emit(
                &format!("{}permission_required", events::PREFIX),
                json!({ "permission": "screen", "restartRequired": permission::RESTART_REQUIRED }),
            );
            return Err(permission::required());
        }
        claim(&sup.session)?;
        let r = start_claimed(&app, &sup, lib, &window, &o);
        if r.is_err() {
            *lock(&sup.session.status) = Status::idle();
        }
        r
    })
    .await
}

/// Mark the session as being asked for; refuse when one is up. Frees the
/// handle of a session that already ended.
fn claim(s: &Session) -> Result<()> {
    let mut st = lock(&s.status);
    if st.active() {
        return Err(Error::new("busy", "a support session is already running"));
    }
    *st = Status {
        state: "consent".into(),
        ..Status::default()
    };
    drop(st);
    stop_handle(s);
    Ok(())
}

fn start_claimed<R: Runtime>(
    app: &AppHandle<R>,
    sup: &Support,
    lib: &'static Lib,
    window: &WebviewWindow<R>,
    o: &HelpOptions,
) -> Result<Status> {
    let brand = brand_of(app, sup, o)?;
    let masks = library_rects(window, &o.masks);
    // The calling window (the dialog attaches to it, the indicator keeps
    // clear of it): only a library that knows the keys gets them.
    let native = lib
        .supports_native()
        .then(|| native_window(window))
        .flatten();
    let native_lib = lib.supports_native();

    if (o.consent_token.is_some() || o.consent_receipt.is_some()) && !lib.supports_consent() {
        return Err(Error::new(
            "unsupported",
            format!(
                "consentToken / consentReceipt need libgaiadesk_embed 0.4.0 (this is {})",
                lib.version()
            ),
        ));
    }
    if !o.consent_granted && o.consent_token.is_none() && o.consent_receipt.is_none() {
        let mut ex = options::consent_options(o, &brand);
        if native_lib {
            ex = options::with_native(
                &ex,
                &Native {
                    parent_window: native,
                    masked: Some(o.masking),
                    ..Native::default()
                },
            );
        }
        let (company, capture, guided) = (brand.company.clone(), o.capture.as_str(), o.guided);
        let (tx, rx) = std::sync::mpsc::channel();
        // A modal dialog: on macOS it must be the main thread.
        app.run_on_main_thread(move || {
            let _ = tx.send(lib.show_consent(&ex, &company, capture, guided));
        })
        .map_err(|e| Error::new("wrong_thread", e.to_string()))?;
        let answer = rx.recv().unwrap_or(Ok(0)).inspect_err(|e| {
            // Our options were refused: a bug to report, not the user's "no".
            let _ = app.emit(&format!("{}error", events::PREFIX), e);
        })?;
        if answer == 0 {
            if let Some(id) = &o.request_id {
                // The agent hears the no at once rather than at the request's expiry.
                let _ = requests::decline(app, id);
            }
        }
        match answer {
            1 => {}
            -1 => return Err(Error::new("no_native_dialog", "this platform has no native consent dialog: ask your user yourself, then pass consentGranted: true")),
            -2 => return Err(Error::new("wrong_thread", "the consent dialog could not be shown on the main thread")),
            _ => return Err(Error::new("declined", "your user declined")),
        }
    }

    let client = client_info(app);
    let created = match (
        &o.embed_token,
        &o.request_id,
        o.publishable_key
            .as_ref()
            .or(sup.config.publishable_key.as_ref()),
    ) {
        (Some(t), _, _) => Created {
            token: t.clone(),
            ..Created::default()
        },
        (None, Some(id), _) => requests::accept(app, id, &client)?,
        (None, None, Some(key)) => {
            let origin = options::native_origin(&app.config().identifier);
            http::create_session(&sup.api(), key, &origin, &options::session_body(o, &client))?
        }
        (None, None, None) => {
            return Err(Error::new(
                "bad_options",
                "give startHelp an embedToken or a requestId, or set publishableKey in the plugin config",
            ))
        }
    };

    {
        let mut st = lock(&sup.session.status);
        st.state = "starting".into();
        st.estimated_wait_seconds = created.estimated_wait_seconds;
    }
    *lock(&sup.creds) = Some(Creds {
        token: created.token.clone(),
        session_id: created.session_id.clone(),
        ended_at: None,
        rated: false,
    });
    let mut config = options::start_config(
        &created.token,
        sup.config.server.as_deref(),
        o,
        &brand,
        &masks,
        lib.supports_branding(),
    );
    if native_lib {
        let n = Native {
            parent_window: native,
            indicator_position: o.indicator_position,
            indicator_origin: indicator::saved(app),
            ..Native::default()
        };
        config = options::with_native(&config, &n);
    }
    let ctx = Box::into_raw(Box::new(Ctx {
        app: app.clone(),
        session: sup.session.clone(),
    }));
    let cb = GdEmbedCallbacks {
        on_event: Some(on_event::<R>),
        user_data: ctx as *mut c_void,
    };
    match lib.start(&config, cb) {
        Ok(h) => {
            *lock(&sup.session.handle) = Some(Handle(h));
            Ok(lock(&sup.session.status).clone())
        }
        Err(e) => {
            // SAFETY: refused before any callback; nothing else holds ctx.
            drop(unsafe { Box::from_raw(ctx) });
            Err(e)
        }
    }
}

/// `client` for the server: this OS, its version, the app's version, this plugin.
fn client_info<R: Runtime>(app: &AppHandle<R>) -> Value {
    let version = app.package_info().version.to_string();
    options::client_info(
        std::env::consts::OS,
        options::os_version().as_deref(),
        &version,
    )
}

/// The company named to the customer: branding name, then the config's
/// `company`, then the app's product name.
pub(crate) fn company_of<R: Runtime>(app: &AppHandle<R>) -> String {
    let sup = app.state::<Support>();
    let base = sup.config.branding.as_ref().and_then(|b| b.name.clone());
    base.or(sup.config.company.clone())
        .unwrap_or_else(|| app.package_info().name.clone())
}

pub(crate) fn brand_of<R: Runtime>(
    app: &AppHandle<R>,
    sup: &Support,
    o: &HelpOptions,
) -> Result<Brand> {
    let base = sup.config.branding.clone().unwrap_or_default();
    let js = o.branding.clone().unwrap_or_default();
    let company = js.name.unwrap_or_else(|| company_of(app));
    let color = js
        .color
        .or(base.color)
        .map(|c| options::brand_color(&c))
        .transpose()?;
    // A logo never stops a session: any failure is a warning and text-only branding.
    let logo = js.logo.or(base.logo).and_then(|l| {
        let resource_dir = app.path().resource_dir().ok();
        let icon = app.default_window_icon();
        let icon = icon.map(|i| (i.rgba(), i.width(), i.height()));
        branding::load_logo(&l, resource_dir.as_deref(), icon)
            .inspect_err(|e| warn(app, &e.code, &e.message))
            .ok()
    });
    Ok(Brand {
        company,
        color,
        logo_png_base64: logo,
    })
}

/// The calling window's native handle: `NSWindow*` on macOS, `HWND` on Windows.
pub(crate) fn native_window<R: Runtime>(w: &WebviewWindow<R>) -> Option<u64> {
    #[cfg(target_os = "macos")]
    return w.ns_window().ok().map(|p| p as usize as u64);
    #[cfg(windows)]
    return w.hwnd().ok().map(|h| h.0 as usize as u64);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = w;
        None
    }
}

/// CSS-pixel rects of the calling page → the library's units.
pub(crate) fn library_rects<R: Runtime>(w: &WebviewWindow<R>, rects: &[CssRect]) -> Vec<GdRect> {
    let scale = w.scale_factor().unwrap_or(1.0);
    let offset = match (w.inner_position(), w.outer_position()) {
        (Ok(i), Ok(o)) => ((i.x - o.x) as f64, (i.y - o.y) as f64),
        _ => (0.0, 0.0),
    };
    rects
        .iter()
        .map(|r| options::to_library_rect(r, offset, scale, std::env::consts::OS))
        .collect()
}

struct Ctx<R: Runtime> {
    app: AppHandle<R>,
    session: Arc<Session>,
}

/// The library's event callback (its threads, or the main thread).
unsafe extern "C" fn on_event<R: Runtime>(user_data: *mut c_void, event_json: *const c_char) {
    let _ = std::panic::catch_unwind(|| {
        // SAFETY: our Ctx, alive until the `ended` below frees it.
        let ctx = unsafe { &*(user_data as *const Ctx<R>) };
        let json = unsafe { CStr::from_ptr(event_json) }.to_string_lossy();
        let mapped = events::map(&json, &mut lock(&ctx.session.status));
        if let Some(("indicator_moved", p)) = &mapped {
            // Kept for this app's next session; not an app-facing event.
            indicator::save(&ctx.app, p);
            return;
        }
        if let Some((name, mut payload)) = mapped {
            if name == "ended" {
                payload["canRate"] = json!(ended(&ctx.app));
            }
            let _ = ctx.app.emit(&format!("{}{name}", events::PREFIX), payload);
            if name == "ended" {
                // SAFETY: `ended` is the library's last call, exactly once.
                drop(unsafe { Box::from_raw(user_data as *mut Ctx<R>) });
            }
        }
    });
}

/// Start the rating window; whether the session can be rated.
fn ended<R: Runtime>(app: &AppHandle<R>) -> bool {
    let sup = app.state::<Support>();
    let session_id = lock(&sup.session.status).session_id.clone();
    let mut c = lock(&sup.creds);
    let Some(c) = c.as_mut() else { return false };
    c.ended_at = Some(Instant::now());
    if c.session_id.is_none() {
        c.session_id = session_id;
    }
    c.session_id.is_some() && !c.rated
}

/// End the session (the library's `ended` follows, `app_stopped`).
#[tauri::command]
pub(crate) async fn stop<R: Runtime>(app: AppHandle<R>) -> Result<Status> {
    blocking(move || {
        stop_now(&app);
        Ok(lock(&app.state::<Support>().session.status).clone())
    })
    .await
}

pub(crate) fn stop_now<R: Runtime>(app: &AppHandle<R>) {
    if let Some(sup) = app.try_state::<Support>() {
        stop_handle(&sup.session);
    }
}

fn stop_handle(s: &Session) {
    let h = lock(&s.handle).take();
    if let (Some(h), Some(lib)) = (h, crate::ffi::loaded()) {
        // SAFETY: taken out of the slot, so used once.
        unsafe { lib.stop(h.0) };
    }
}

/// Pause (`true`) or resume guided input. Resuming a pause the customer
/// made is refused (`pause_refused`).
#[tauri::command]
pub(crate) async fn set_paused<R: Runtime>(app: AppHandle<R>, paused: bool) -> Result<Status> {
    blocking(move || {
        let sup = app.state::<Support>();
        let lib = sup.lib(&app)?;
        let rc = with_live(&sup.session, |h| unsafe { lib.set_paused(h, paused) })?;
        match rc {
            0 => {
                lock(&sup.session.status).paused = paused;
                Ok(lock(&sup.session.status).clone())
            }
            -2 => Err(Error::new(
                "pause_refused",
                "your customer paused guided control; only they can resume it",
            )),
            _ => Err(Error::new("no_session", "no support session is running")),
        }
    })
    .await
}

/// Replace the masks (CSS pixels of the calling page; `[]` clears).
#[tauri::command]
pub(crate) async fn set_masks<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    masks: Vec<CssRect>,
) -> Result<()> {
    blocking(move || {
        let sup = app.state::<Support>();
        let lib = sup.lib(&app)?;
        let rects = library_rects(&window, &masks);
        with_live(&sup.session, |h| unsafe { lib.set_masks(h, &rects) })?
    })
    .await
}

#[tauri::command]
pub(crate) fn status<R: Runtime>(app: AppHandle<R>) -> Status {
    lock(&app.state::<Support>().session.status).clone()
}

/// What this app build can do, for feature checks. Never fails.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    available: bool,
    plugin_version: &'static str,
    library_version: Option<String>,
    library_found: bool,
    library_path: Option<String>,
    library_error: Option<Error>,
    platform: &'static str,
    guided_supported: bool,
    screen_permission: &'static str,
    company: String,
}

#[tauri::command]
pub(crate) async fn get_info<R: Runtime>(app: AppHandle<R>) -> Info {
    // Loading the library may touch the disk: off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let lib = app.state::<Support>().lib(&app);
        Info {
            available: true,
            plugin_version: env!("CARGO_PKG_VERSION"),
            library_version: lib.as_ref().ok().map(|l| l.version()),
            library_found: lib.is_ok(),
            library_path: lib.as_ref().ok().map(|l| l.path.display().to_string()),
            library_error: lib.err(),
            platform: std::env::consts::OS,
            // Linux is view-only: X11 cannot deliver input to one app only.
            guided_supported: std::env::consts::OS != "linux",
            screen_permission: permission::screen(),
            company: company_of(&app),
        }
    })
    .await
    .expect("get_info does not panic")
}

/// `granted` or `denied` (macOS Screen Recording; always `granted` elsewhere).
#[tauri::command]
pub(crate) fn screen_permission() -> &'static str {
    permission::screen()
}

/// Ask macOS for Screen Recording (its prompt shows once; later, System
/// Settings). The state after; a grant counts after the app is relaunched.
#[tauri::command]
pub(crate) async fn request_screen_permission() -> &'static str {
    tauri::async_runtime::spawn_blocking(permission::request)
        .await
        .unwrap_or("denied")
}

#[tauri::command]
pub(crate) fn open_screen_permission_settings() -> Result<()> {
    permission::open_settings()
}

/// Run `f` on the live handle (held locked, so `stop` cannot free it meanwhile).
fn with_live<T>(s: &Session, f: impl FnOnce(*mut crate::ffi::GdEmbed) -> T) -> Result<T> {
    let active = lock(&s.status).active();
    let g = lock(&s.handle);
    match g.as_ref() {
        Some(h) if active => Ok(f(h.0)),
        _ => Err(Error::new("no_session", "no support session is running")),
    }
}

pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| Error::new("internal", e.to_string()))?
}
