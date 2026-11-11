//! `libgaiadesk_embed`'s C ABI (`gaiadesk_embed.h`), loaded at run time.
//!
//! The library is loaded once and never unloaded: its threads call back into
//! us until a session's `ended`, so it must outlive every session.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::error::Error;

#[repr(C)]
pub struct GdEmbed {
    _private: [u8; 0],
}

pub type GdEventFn = unsafe extern "C" fn(user_data: *mut c_void, event_json: *const c_char);

#[repr(C)]
pub struct GdEmbedCallbacks {
    pub on_event: Option<GdEventFn>,
    pub user_data: *mut c_void,
}

/// A masked region in the library's units (see `options::to_library_rect`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GdRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

type StartFn = unsafe extern "C" fn(*const c_char, GdEmbedCallbacks) -> *mut GdEmbed;
type StopFn = unsafe extern "C" fn(*mut GdEmbed);
type SetMasksFn = unsafe extern "C" fn(*mut GdEmbed, *const GdRect, usize) -> c_int;
type SetPausedFn = unsafe extern "C" fn(*mut GdEmbed, c_int) -> c_int;
type ShowConsentFn = unsafe extern "C" fn(*const c_char, *const c_char, c_int) -> c_int;
type ShowConsentExFn = unsafe extern "C" fn(*const c_char) -> c_int;
type StrFn = unsafe extern "C" fn() -> *const c_char;

/// The loaded library.
pub struct Lib {
    pub path: PathBuf,
    start: StartFn,
    stop: StopFn,
    set_masks: SetMasksFn,
    set_paused: SetPausedFn,
    show_consent: ShowConsentFn,
    /// Newer libraries only: the branded dialog.
    show_consent_ex: Option<ShowConsentExFn>,
    last_error: StrFn,
    version: StrFn,
    build_info: StrFn,
}

static LOADED: OnceLock<&'static Lib> = OnceLock::new();

/// The library, once loaded.
pub fn loaded() -> Option<&'static Lib> {
    LOADED.get().copied()
}

/// The library's file name on this OS.
pub const fn file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "gaiadesk_embed.dll"
    } else if cfg!(target_os = "macos") {
        "libgaiadesk_embed.dylib"
    } else {
        "libgaiadesk_embed.so"
    }
}

/// Where to look, in order: `GAIADESK_EMBED_LIBRARY`, the path the app set,
/// macOS `Contents/Frameworks`, the bundled resources' `gaiadesk/`, and
/// `gaiadesk/` beside the executable (`tauri dev`). Pure.
pub fn candidates(os: &str, env: Option<PathBuf>, configured: Option<PathBuf>, resource_dir: Option<&Path>, exe_dir: Option<&Path>) -> Vec<PathBuf> {
    let name = match os {
        "windows" => "gaiadesk_embed.dll",
        "macos" => "libgaiadesk_embed.dylib",
        _ => "libgaiadesk_embed.so",
    };
    let mut v: Vec<PathBuf> = env.into_iter().chain(configured).collect();
    if let Some(r) = resource_dir {
        if os == "macos" {
            v.push(r.join("../Frameworks").join(name));
        }
        v.push(r.join("gaiadesk").join(name));
    }
    if let Some(e) = exe_dir {
        v.push(e.join("gaiadesk").join(name));
        v.push(e.join(name));
    }
    v
}

impl Lib {
    /// Load the first candidate that exists.
    pub fn load_first(candidates: &[PathBuf]) -> Result<&'static Lib, Error> {
        let Some(path) = candidates.iter().find(|p| p.is_file()) else {
            let tried: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
            return Err(Error::new(
                "library_missing",
                format!("{} was not found (tried {}); see the README's \"Bundle the library\"", file_name(), tried.join(", ")),
            ));
        };
        Lib::load(path)
    }

    /// Load the library at `path` (once per process: later calls return the
    /// first one). Leaked on purpose: see the module docs.
    pub fn load(path: &Path) -> Result<&'static Lib, Error> {
        let fail = |e: libloading::Error| Error::new("library_unusable", format!("{}: {e}", path.display()));
        // SAFETY: loading a library runs its initialisers; libgaiadesk_embed's are
        // the Rust runtime's own. Its symbols have the C signatures declared here.
        unsafe {
            let lib = libloading::Library::new(path).map_err(fail)?;
            let l = Lib {
                path: path.to_path_buf(),
                start: *lib.get::<StartFn>(b"gd_embed_start\0").map_err(fail)?,
                stop: *lib.get::<StopFn>(b"gd_embed_stop\0").map_err(fail)?,
                set_masks: *lib.get::<SetMasksFn>(b"gd_embed_set_mask_rects\0").map_err(fail)?,
                set_paused: *lib.get::<SetPausedFn>(b"gd_embed_set_paused\0").map_err(fail)?,
                show_consent: *lib.get::<ShowConsentFn>(b"gd_embed_show_consent\0").map_err(fail)?,
                show_consent_ex: lib.get::<ShowConsentExFn>(b"gd_embed_show_consent_ex\0").ok().map(|s| *s),
                last_error: *lib.get::<StrFn>(b"gd_embed_last_error\0").map_err(fail)?,
                version: *lib.get::<StrFn>(b"gd_embed_version\0").map_err(fail)?,
                build_info: *lib.get::<StrFn>(b"gd_embed_build_info\0").map_err(fail)?,
            };
            std::mem::forget(lib);
            let l: &'static Lib = Box::leak(Box::new(l));
            Ok(*LOADED.get_or_init(|| l))
        }
    }

    /// Whether this build has the branded consent dialog and accepts
    /// `brand_color` / `logo_png_base64` (libraries after 0.1.0).
    pub fn supports_branding(&self) -> bool {
        self.show_consent_ex.is_some()
    }

    pub fn version(&self) -> String {
        // SAFETY: a static NUL-terminated string.
        unsafe { cstr((self.version)()) }
    }

    pub fn build_info(&self) -> String {
        // SAFETY: as above.
        unsafe { cstr((self.build_info)()) }
    }

    /// `gd_embed_last_error` as an Error. Per thread: call right after the
    /// call that failed, on the same thread.
    fn last_error(&self, fallback: &str) -> Error {
        // SAFETY: valid until the next call on this thread; copied at once.
        let s = unsafe { cstr((self.last_error)()) };
        serde_json::from_str::<Error>(&s).unwrap_or_else(|_| Error::new(fallback, if s.is_empty() { fallback.to_string() } else { s }))
    }

    /// Start a session. `callbacks.user_data` must stay valid until `ended`.
    pub fn start(&self, config_json: &str, callbacks: GdEmbedCallbacks) -> Result<*mut GdEmbed, Error> {
        let c = CString::new(config_json).map_err(|_| Error::new("bad_config", "config contains a NUL byte"))?;
        // SAFETY: a NUL-terminated config; the caller keeps user_data alive.
        let h = unsafe { (self.start)(c.as_ptr(), callbacks) };
        if h.is_null() {
            Err(self.last_error("start_failed"))
        } else {
            Ok(h)
        }
    }

    /// End the session and free the handle.
    ///
    /// # Safety
    /// `h` came from `start` and is not used again.
    pub unsafe fn stop(&self, h: *mut GdEmbed) {
        (self.stop)(h)
    }

    /// # Safety
    /// `h` is a live handle.
    pub unsafe fn set_masks(&self, h: *mut GdEmbed, rects: &[GdRect]) -> Result<(), Error> {
        if (self.set_masks)(h, rects.as_ptr(), rects.len()) == 0 {
            Ok(())
        } else {
            Err(self.last_error("bad_rects"))
        }
    }

    /// 0 ok, -1 no session, -2 refused (the customer paused).
    ///
    /// # Safety
    /// `h` is a live handle.
    pub unsafe fn set_paused(&self, h: *mut GdEmbed, paused: bool) -> c_int {
        (self.set_paused)(h, paused as c_int)
    }

    /// The consent dialog: 1 agreed, 0 declined, -1 no native dialog here,
    /// -2 wrong thread. Uses the branded dialog when the library has it; that
    /// one refuses bad options by showing nothing and returning 0, which comes
    /// back as `Err` (`bad_config`), never as a decline. macOS: call on the
    /// main thread.
    pub fn show_consent(&self, ex_options_json: &str, company: &str, capture: &str, guided: bool) -> Result<c_int, Error> {
        if let Some(ex) = self.show_consent_ex {
            let o = CString::new(ex_options_json).map_err(|_| Error::new("bad_config", "consent options contain a NUL byte"))?;
            // SAFETY: a NUL-terminated JSON string.
            let rc = unsafe { ex(o.as_ptr()) };
            if rc == 0 {
                // shortcut: last_error is not cleared by a plain decline, so a stale bad_config
                // from an earlier refused call on this thread would read as one; harmless once fixed.
                let e = self.last_error("declined");
                if e.code == "bad_config" {
                    return Err(e);
                }
            }
            return Ok(rc);
        }
        let (Ok(co), Ok(ca)) = (CString::new(company), CString::new(capture)) else { return Err(Error::new("bad_config", "company contains a NUL byte")) };
        // SAFETY: NUL-terminated strings.
        Ok(unsafe { (self.show_consent)(co.as_ptr(), ca.as_ptr(), guided as c_int) })
    }
}

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_in_order() {
        let r = Path::new("/App.app/Contents/Resources");
        let e = Path::new("/App.app/Contents/MacOS");
        let v = candidates("macos", Some("/env.dylib".into()), None, Some(r), Some(e));
        assert_eq!(
            v,
            vec![
                PathBuf::from("/env.dylib"),
                r.join("../Frameworks/libgaiadesk_embed.dylib"),
                r.join("gaiadesk/libgaiadesk_embed.dylib"),
                e.join("gaiadesk/libgaiadesk_embed.dylib"),
                e.join("libgaiadesk_embed.dylib"),
            ]
        );
        let w = candidates("windows", None, Some("C:/x.dll".into()), Some(Path::new("C:/app")), None);
        assert_eq!(w, vec![PathBuf::from("C:/x.dll"), Path::new("C:/app").join("gaiadesk/gaiadesk_embed.dll")]);
    }

    #[test]
    fn a_missing_library_says_where_it_looked() {
        let err = Lib::load_first(&[PathBuf::from("/nope/libgaiadesk_embed.so")]).err().unwrap();
        assert_eq!(err.code, "library_missing");
        assert!(err.message.contains("/nope/libgaiadesk_embed.so"));
    }
}
