//! The JS commands: start_help, stop, set_paused, set_masks, status.

use std::ffi::{c_char, c_void, CStr};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::error::{Error, Result};
use crate::events::{self, Status};
use crate::ffi::{GdEmbedCallbacks, GdRect, Lib};
use crate::options::{self, Brand, CssRect, HelpOptions};
use crate::{branding, http, lock, Handle, Session, Support};

/// Ask, create the session if needed, start. Resolves once sharing has
/// started (events follow); rejects with `{code, message}`.
#[tauri::command]
pub(crate) async fn start_help<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>, options: Option<HelpOptions>) -> Result<Status> {
    let o = options.unwrap_or_default();
    blocking(move || {
        let sup = app.state::<Support>();
        let lib = sup.lib(&app)?;
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
    *st = Status { state: "consent".into(), ..Status::default() };
    drop(st);
    stop_handle(s);
    Ok(())
}

fn start_claimed<R: Runtime>(app: &AppHandle<R>, sup: &Support, lib: &'static Lib, window: &WebviewWindow<R>, o: &HelpOptions) -> Result<Status> {
    let resource_dir = app.path().resource_dir().ok();
    let brand = brand_of(app, sup, o, resource_dir.as_deref())?;
    let masks = library_rects(window, &o.masks);

    if !o.consent_granted {
        let ex = options::consent_options(o, &brand);
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
        match answer {
            1 => {}
            -1 => return Err(Error::new("no_native_dialog", "this platform has no native consent dialog: ask your user yourself, then pass consentGranted: true")),
            -2 => return Err(Error::new("wrong_thread", "the consent dialog could not be shown on the main thread")),
            _ => return Err(Error::new("declined", "your user declined")),
        }
    }

    let token = match (&o.embed_token, o.publishable_key.as_ref().or(sup.config.publishable_key.as_ref())) {
        (Some(t), _) => t.clone(),
        (None, Some(key)) => {
            let origin = options::native_origin(&app.config().identifier);
            http::create_session(&options::api_base(sup.config.server.as_deref()), key, &origin, &options::session_body(o))?
        }
        (None, None) => return Err(Error::new("bad_options", "give startHelp an embedToken, or set publishableKey in the plugin config")),
    };

    lock(&sup.session.status).state = "starting".into();
    let config = options::start_config(&token, sup.config.server.as_deref(), o, &brand, &masks, lib.supports_branding());
    let ctx = Box::into_raw(Box::new(Ctx { app: app.clone(), session: sup.session.clone() }));
    let cb = GdEmbedCallbacks { on_event: Some(on_event::<R>), user_data: ctx as *mut c_void };
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

fn brand_of<R: Runtime>(app: &AppHandle<R>, sup: &Support, o: &HelpOptions, resource_dir: Option<&std::path::Path>) -> Result<Brand> {
    let base = sup.config.branding.clone().unwrap_or_default();
    let js = o.branding.clone().unwrap_or_default();
    let company = js.name.or(base.name).or(sup.config.company.clone()).unwrap_or_else(|| app.package_info().name.clone());
    let color = js.color.or(base.color).map(|c| options::brand_color(&c)).transpose()?;
    let logo = js.logo.or(base.logo).map(|l| branding::load_logo(&l, resource_dir)).transpose()?;
    Ok(Brand { company, color, logo_png_base64: logo })
}

/// CSS-pixel rects of the calling page → the library's units.
fn library_rects<R: Runtime>(w: &WebviewWindow<R>, rects: &[CssRect]) -> Vec<GdRect> {
    let scale = w.scale_factor().unwrap_or(1.0);
    let offset = match (w.inner_position(), w.outer_position()) {
        (Ok(i), Ok(o)) => ((i.x - o.x) as f64, (i.y - o.y) as f64),
        _ => (0.0, 0.0),
    };
    rects.iter().map(|r| options::to_library_rect(r, offset, scale, std::env::consts::OS)).collect()
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
        if let Some((name, payload)) = mapped {
            let _ = ctx.app.emit(&format!("{}{name}", events::PREFIX), payload);
            if name == "ended" {
                // SAFETY: `ended` is the library's last call, exactly once.
                drop(unsafe { Box::from_raw(user_data as *mut Ctx<R>) });
            }
        }
    });
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
            -2 => Err(Error::new("pause_refused", "your customer paused guided control; only they can resume it")),
            _ => Err(Error::new("no_session", "no support session is running")),
        }
    })
    .await
}

/// Replace the masks (CSS pixels of the calling page; `[]` clears).
#[tauri::command]
pub(crate) async fn set_masks<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>, masks: Vec<CssRect>) -> Result<()> {
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

/// Run `f` on the live handle (held locked, so `stop` cannot free it meanwhile).
fn with_live<T>(s: &Session, f: impl FnOnce(*mut crate::ffi::GdEmbed) -> T) -> Result<T> {
    let active = lock(&s.status).active();
    let g = lock(&s.handle);
    match g.as_ref() {
        Some(h) if active => Ok(f(h.0)),
        _ => Err(Error::new("no_session", "no support session is running")),
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| Error::new("internal", e.to_string()))?
}
