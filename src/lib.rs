//! In-app remote support for Tauri v2 apps, over GaiaDesk's prebuilt
//! `libgaiadesk_embed`: a "Get help" button that shares your app's window
//! with your support team in the GaiaDesk console, after your user agreed,
//! with the library's "Sharing with …" indicator, masks, and optional guided
//! control.
//!
//! ```ignore
//! tauri::Builder::default()
//!     .plugin(tauri_plugin_gaiadesk_support::init())
//! #   ;
//! ```
//!
//! The library is loaded at run time (see [`ffi::candidates`]) from your
//! bundle; `gaiadesk-embed-fetch` puts it there at build time. The JS side is
//! `@gaiadesk/tauri-plugin-support`.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tauri::plugin::TauriPlugin;
use tauri::{Manager, RunEvent, Runtime};

mod branding;
mod commands;
pub mod consent;
pub mod error;
mod events;
pub mod ffi;
mod followup;
mod http;
mod indicator;
pub mod keys;
pub mod options;
mod permission;
mod requests;

pub use consent::ConsentOptions;
pub use error::Error;
pub use events::Status;
pub use options::{Branding, Capture, HelpOptions, IndicatorPosition, Mode};

/// The plugin's section of `tauri.conf.json` (`plugins."gaiadesk-support"`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    /// `pk_live_…` / `pk_test_…` (or an older `ak_…`), a publishable key
    /// registered for `app://<your identifier>`: `startHelp()` then creates
    /// the session itself. A secret key (`sk_…`) fails the build and is
    /// refused at run time (`secret_key_in_app`).
    pub publishable_key: Option<String>,
    /// Default company name (else the app's product name).
    pub company: Option<String>,
    /// The GaiaDesk server (default `https://gaiadesk.net`).
    pub server: Option<String>,
    /// Default branding, merged under `startHelp`'s.
    pub branding: Option<Branding>,
    /// Load the library from here first.
    pub library_path: Option<PathBuf>,
}

/// Configure the plugin from Rust (each value wins over `tauri.conf.json`).
#[derive(Debug, Default)]
pub struct Builder {
    over: Config,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publishable_key(mut self, key: impl Into<String>) -> Self {
        self.over.publishable_key = Some(key.into());
        self
    }

    pub fn company(mut self, name: impl Into<String>) -> Self {
        self.over.company = Some(name.into());
        self
    }

    pub fn server(mut self, url: impl Into<String>) -> Self {
        self.over.server = Some(url.into());
        self
    }

    pub fn branding(mut self, b: Branding) -> Self {
        self.over.branding = Some(b);
        self
    }

    pub fn library_path(mut self, p: impl Into<PathBuf>) -> Self {
        self.over.library_path = Some(p.into());
        self
    }

    pub fn build<R: Runtime>(self) -> TauriPlugin<R, Option<Config>> {
        let over = self.over;
        tauri::plugin::Builder::<R, Option<Config>>::new("gaiadesk-support")
            .invoke_handler(tauri::generate_handler![
                commands::start_help,
                commands::stop,
                commands::set_paused,
                commands::set_masks,
                commands::status,
                commands::get_info,
                commands::screen_permission,
                commands::request_screen_permission,
                commands::open_screen_permission_settings,
                followup::rate,
                followup::attach_diagnostics,
                followup::send_diagnostics,
                followup::discard_diagnostics,
                requests::listen,
                requests::stop_listening,
                requests::decline_request,
                consent::consent,
                consent::preview,
                consent::record_consent,
                consent::wait_info
            ])
            .setup(move |app, api| {
                let file = api.config().clone().unwrap_or_default();
                let config = Config {
                    publishable_key: over.publishable_key.or(file.publishable_key),
                    company: over.company.or(file.company),
                    server: over.server.or(file.server),
                    branding: over.branding.or(file.branding),
                    library_path: over.library_path.or(file.library_path),
                };
                app.manage(Support {
                    config,
                    session: Arc::new(Session::default()),
                    creds: Mutex::new(None),
                    diagnostics: Mutex::new(None),
                    listen_token: Mutex::new(None),
                    listen_gen: AtomicU64::new(0),
                });
                Ok(())
            })
            .on_event(|app, e| {
                if let RunEvent::Exit = e {
                    // Quitting ends the session (the library's `app_stopped`) and listening.
                    commands::stop_now(app);
                    requests::stop_now(app);
                }
            })
            .build()
    }
}

/// The plugin with `tauri.conf.json`'s config.
pub fn init<R: Runtime>() -> TauriPlugin<R, Option<Config>> {
    Builder::new().build()
}

/// A live library handle.
pub(crate) struct Handle(pub(crate) *mut ffi::GdEmbed);
// SAFETY: the library's handle calls are thread-safe (gaiadesk_embed.h).
unsafe impl Send for Handle {}

/// The one session (the library runs one at a time per app).
pub(crate) struct Session {
    /// Separate locks: the library may call back (status) during a handle call.
    pub(crate) handle: Mutex<Option<Handle>>,
    pub(crate) status: Mutex<Status>,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            handle: Mutex::new(None),
            status: Mutex::new(Status::idle()),
        }
    }
}

pub(crate) struct Support {
    pub(crate) config: Config,
    pub(crate) session: Arc<Session>,
    /// The last session's embed token, for its rating and attachments.
    pub(crate) creds: Mutex<Option<followup::Creds>>,
    /// Diagnostics the app attached, waiting for the customer's yes.
    pub(crate) diagnostics: Mutex<Option<followup::Diagnostics>>,
    /// The listen token while listening for session requests.
    pub(crate) listen_token: Mutex<Option<String>>,
    /// Bumped by every listen/stop: an older listener thread sees it and quits.
    pub(crate) listen_gen: AtomicU64,
}

impl Support {
    /// The library, loaded on first use.
    pub(crate) fn lib<R: Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
    ) -> Result<&'static ffi::Lib, Error> {
        if let Some(l) = ffi::loaded() {
            return Ok(l);
        }
        let resource_dir = app.path().resource_dir().ok();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(PathBuf::from));
        let env = std::env::var_os("GAIADESK_EMBED_LIBRARY").map(PathBuf::from);
        let c = ffi::candidates(
            std::env::consts::OS,
            env,
            self.config.library_path.clone(),
            resource_dir.as_deref(),
            exe_dir.as_deref(),
        );
        ffi::Lib::load_first(&c)
    }

    /// The API base (`https://api.gaiadesk.net/v1` or `<server>/v1`).
    pub(crate) fn api(&self) -> String {
        options::api_base(self.config.server.as_deref())
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
