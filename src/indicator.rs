//! Where the customer last dragged the "Sharing with …" indicator, per app:
//! `<app data dir>/gaiadesk-support/indicator.json` (`{"x":…,"y":…}`, the
//! platform's own screen coordinates as the library reports them). The
//! library itself writes nothing to disk; it reports a drag
//! (`indicator_moved`) and is handed the place back (`indicator_origin`),
//! restoring it only while it still fits on a screen.

use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

fn file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("gaiadesk-support").join("indicator.json"))
}

/// `{"x":…,"y":…}` → the place, when both are finite numbers. Pure.
pub fn parse(v: &Value) -> Option<(f64, f64)> {
    let (x, y) = (v.get("x")?.as_f64()?, v.get("y")?.as_f64()?);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

pub(crate) fn saved<R: Runtime>(app: &AppHandle<R>) -> Option<(f64, f64)> {
    let text = std::fs::read_to_string(file(app)?).ok()?;
    parse(&serde_json::from_str(&text).ok()?)
}

/// Best effort: a place that cannot be kept is simply not remembered.
pub(crate) fn save<R: Runtime>(app: &AppHandle<R>, at: &Value) {
    let (Some(path), Some((x, y))) = (file(app), parse(at)) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "x": x, "y": y }).to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_real_place_is_kept() {
        assert_eq!(parse(&json!({ "x": 10.5, "y": -3 })), Some((10.5, -3.0)));
        assert_eq!(parse(&json!({ "x": 1 })), None);
        assert_eq!(parse(&json!({ "x": "1", "y": 2 })), None);
    }
}
