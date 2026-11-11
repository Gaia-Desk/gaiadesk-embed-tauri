//! macOS Screen Recording: every capture mode needs it (ScreenCaptureKit,
//! even for the app's own window). Other OSes need nothing: `granted`.
//!
//! macOS's public API tells only granted or not (`CGPreflightScreenCaptureAccess`):
//! never asked and refused read the same, so both are `denied`. A grant takes
//! effect when the app is next launched, hence `restartRequired`.

use crate::error::{Error, Result};

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// `granted` or `denied`.
pub fn screen() -> &'static str {
    #[cfg(target_os = "macos")]
    // SAFETY: no arguments; safe from any thread (macOS 10.15+).
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        return "denied";
    }
    "granted"
}

/// Ask macOS (it shows its prompt the first time only; later it is System
/// Settings). Returns the state after.
pub fn request() -> &'static str {
    #[cfg(target_os = "macos")]
    // SAFETY: as above.
    if !unsafe { CGRequestScreenCaptureAccess() } {
        return "denied";
    }
    "granted"
}

/// Whether a grant needs the app relaunched before it counts.
pub const RESTART_REQUIRED: bool = cfg!(target_os = "macos");

/// Open System Settings → Privacy & Security → Screen Recording (macOS; a
/// no-op elsewhere).
pub fn open_settings() -> Result<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .status()
        .map_err(|e| Error::new("internal", format!("could not open System Settings: {e}")))?;
    Ok(())
}

/// The refusal when sharing would fail for want of the permission.
pub fn required() -> Error {
    Error::new(
        "permission_required",
        "macOS Screen Recording is not allowed for this app: call requestScreenPermission() or openScreenPermissionSettings(), then relaunch the app",
    )
}
