//! After the session: the customer's rating (`rate`) and the diagnostics the
//! app attached (`attach_diagnostics`, sent only after the customer said yes:
//! `send_diagnostics`), both with the session's own embed token, kept in
//! memory for 24 hours after the session ends (or until the next session).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::commands::blocking;
use crate::error::{Error, Result};
use crate::events::PREFIX;
use crate::{http, lock, Support};

/// How long after its end a session takes a rating or attachments.
pub const WINDOW: Duration = Duration::from_secs(24 * 3600);
/// The server's limits.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_SESSION_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_FILES: usize = 10;

/// The last session's credentials.
#[derive(Debug, Clone)]
pub struct Creds {
    pub token: String,
    /// From the create's answer; with an embed token, the library's `code` event.
    pub session_id: Option<String>,
    pub ended_at: Option<Instant>,
    /// The customer rated it (no second rating; attachments may still follow).
    pub rated: bool,
}

impl Creds {
    /// Still good at `now`.
    pub fn live(&self, now: Instant) -> bool {
        self.ended_at
            .map_or(true, |e| now.duration_since(e) <= WINDOW)
    }
}

/// The token and session id to call with, or why not.
fn creds<R: Runtime>(app: &AppHandle<R>) -> Result<(String, String)> {
    let sup = app.state::<Support>();
    let c = lock(&sup.creds).clone().filter(|c| c.live(Instant::now()));
    let id = c
        .as_ref()
        .and_then(|c| c.session_id.clone())
        .or_else(|| lock(&sup.session.status).session_id.clone());
    match (c, id) {
        (Some(c), Some(id)) => Ok((c.token, id)),
        _ => Err(Error::new("no_session", "no support session to rate or attach to (it must be this app run's last one, within 24 hours of its end)")),
    }
}

/// Rate the last session, 1 to 5, once it ended. Server refusals:
/// `session_not_ended`, `already_rated`, `rating_window_closed`, `bad_rating`, `bad_comment`.
#[tauri::command]
pub(crate) async fn rate<R: Runtime>(
    app: AppHandle<R>,
    rating: u8,
    comment: Option<String>,
) -> Result<Value> {
    blocking(move || {
        let (token, id) = creds(&app)?;
        let sup = app.state::<Support>();
        let url = format!("{}/support/sessions/{id}/csat", sup.api());
        let r = http::post_json(
            &url,
            &token,
            None,
            &json!({ "rating": rating, "comment": comment }).to_string(),
        );
        if r.is_ok() || r.as_ref().is_err_and(|e| e.code == "already_rated") {
            if let Some(c) = lock(&sup.creds).as_mut() {
                c.rated = true;
            }
        }
        r
    })
    .await
}

/// What the app attached.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostics {
    /// Files to send (each its last `maxBytes`).
    #[serde(default)]
    pub files: Vec<PathBuf>,
    /// Text to send as `diagnostics.txt`.
    pub text: Option<String>,
    /// Per file, at most 10 MiB (the default).
    pub max_bytes: Option<u64>,
}

/// Check what the app attached. Pure.
pub fn check(d: &Diagnostics) -> Result<()> {
    let count = d.files.len() + d.text.is_some() as usize;
    if count == 0 {
        return Err(Error::new(
            "bad_options",
            "attach at least one file or some text",
        ));
    }
    if count > MAX_FILES {
        return Err(Error::new(
            "bad_options",
            format!("at most {MAX_FILES} attachments (files and text together)"),
        ));
    }
    if d.max_bytes.is_some_and(|m| m == 0 || m > MAX_FILE_BYTES) {
        return Err(Error::new(
            "bad_options",
            format!("maxBytes is 1 to {MAX_FILE_BYTES}"),
        ));
    }
    if d.text
        .as_ref()
        .is_some_and(|t| t.len() as u64 > MAX_FILE_BYTES)
    {
        return Err(Error::new(
            "bad_options",
            format!("text is at most {MAX_FILE_BYTES} bytes"),
        ));
    }
    Ok(())
}

/// Attach diagnostics. Nothing is sent until the customer agrees (the JS
/// side asks, then calls `send_diagnostics`). Returns whether a session is
/// up now (then ask at once).
#[tauri::command]
pub(crate) fn attach_diagnostics<R: Runtime>(
    app: AppHandle<R>,
    diagnostics: Diagnostics,
) -> Result<bool> {
    check(&diagnostics)?;
    let sup = app.state::<Support>();
    *lock(&sup.diagnostics) = Some(diagnostics);
    let active = lock(&sup.session.status).active();
    Ok(active)
}

#[tauri::command]
pub(crate) fn discard_diagnostics<R: Runtime>(app: AppHandle<R>) {
    *lock(&app.state::<Support>().diagnostics) = None;
}

/// The last `max` bytes of a file (logs grow at the end).
pub fn tail(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max)))?;
    let mut v = Vec::new();
    f.take(max).read_to_end(&mut v)?;
    Ok(v)
}

/// A file name as an `X-Filename` header can carry it: printable ASCII
/// (anything else, and `/` or `\\`, becomes `_`), at most 255 characters. Pure.
pub fn safe_name(name: &str) -> String {
    let ok = |c: char| (' '..='~').contains(&c) && c != '/' && c != '\\';
    let n: String = name
        .chars()
        .take(255)
        .map(|c| if ok(c) { c } else { '_' })
        .collect();
    if n.trim().is_empty() {
        "attachment".into()
    } else {
        n
    }
}

/// The attachments to upload: `(file name, bytes)`; a file that cannot be
/// read is skipped and named in the second list. Stops at the session cap.
pub fn collect(d: &Diagnostics) -> (Vec<(String, Vec<u8>)>, Vec<String>) {
    let max = d.max_bytes.unwrap_or(MAX_FILE_BYTES);
    let (mut out, mut skipped, mut total) = (Vec::new(), Vec::new(), 0u64);
    let text = d.text.as_ref().map(|t| {
        (
            "diagnostics.txt".to_string(),
            Ok(t.as_bytes()[t.len().saturating_sub(max as usize)..].to_vec()),
        )
    });
    let files = d.files.iter().map(|p| {
        let name = p
            .file_name()
            .map(|n| safe_name(&n.to_string_lossy()))
            .unwrap_or_else(|| "attachment".into());
        (
            name,
            tail(p, max).map_err(|e| format!("{}: {e}", p.display())),
        )
    });
    for (name, bytes) in text.into_iter().chain(files) {
        match bytes {
            Ok(b) if b.is_empty() => skipped.push(format!("{name}: empty")),
            Ok(b) if total + b.len() as u64 > MAX_SESSION_BYTES => skipped.push(format!(
                "{name}: over the session's {MAX_SESSION_BYTES} bytes"
            )),
            Ok(b) => {
                total += b.len() as u64;
                out.push((name, b));
            }
            Err(e) => skipped.push(e),
        }
    }
    (out, skipped)
}

/// Upload what was attached (the customer agreed). Returns the server's
/// attachment objects; skipped files are a `warning` event.
#[tauri::command]
pub(crate) async fn send_diagnostics<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Value>> {
    blocking(move || {
        let (token, id) = creds(&app)?;
        let sup = app.state::<Support>();
        let d = lock(&sup.diagnostics)
            .take()
            .ok_or_else(|| Error::new("bad_options", "no diagnostics attached"))?;
        let (files, skipped) = collect(&d);
        if !skipped.is_empty() {
            warn(&app, "diagnostics_skipped", &skipped.join("; "));
        }
        let url = format!("{}/support/sessions/{id}/attachments", sup.api());
        let mut sent = Vec::new();
        for (name, bytes) in files {
            match http::post_file(&url, &token, &name, &bytes) {
                Ok(a) => sent.push(a),
                Err(e) => {
                    warn(&app, &e.code, &format!("{name}: {}", e.message));
                    return Err(e);
                }
            }
        }
        let _ = app.emit(
            &format!("{PREFIX}diagnostics_sent"),
            json!({ "attachments": sent }),
        );
        Ok(sent)
    })
    .await
}

/// A `warning` event: something went wrong that does not stop the session.
pub(crate) fn warn<R: Runtime>(app: &AppHandle<R>, code: &str, message: &str) {
    let _ = app.emit(
        &format!("{PREFIX}warning"),
        json!({ "code": code, "message": message }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_may_be_attached() {
        let d = |files: usize, text: bool, max: Option<u64>| Diagnostics {
            files: vec![PathBuf::from("/x"); files],
            text: text.then(|| "t".into()),
            max_bytes: max,
        };
        assert!(check(&d(1, false, None)).is_ok());
        assert!(check(&d(9, true, Some(1))).is_ok());
        for bad in [
            d(0, false, None),
            d(10, true, None),
            d(1, false, Some(0)),
            d(1, false, Some(MAX_FILE_BYTES + 1)),
        ] {
            assert_eq!(check(&bad).unwrap_err().code, "bad_options", "{bad:?}");
        }
    }

    #[test]
    fn files_are_tailed_and_missing_ones_skipped() {
        let dir = std::env::temp_dir().join(format!("gd-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.log"), b"0123456789").unwrap();
        std::fs::write(dir.join("empty.log"), b"").unwrap();
        let d = Diagnostics {
            files: vec![
                dir.join("app.log"),
                dir.join("missing.log"),
                dir.join("empty.log"),
            ],
            text: Some("hello".into()),
            max_bytes: Some(4),
        };
        let (out, skipped) = collect(&d);
        assert_eq!(
            out,
            vec![
                ("diagnostics.txt".into(), b"ello".to_vec()),
                ("app.log".into(), b"6789".to_vec())
            ]
        );
        assert_eq!(skipped.len(), 2, "{skipped:?}");
        assert!(skipped[0].contains("missing.log") && skipped[1].contains("empty"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_names_fit_a_header() {
        assert_eq!(safe_name("app.log"), "app.log");
        assert_eq!(safe_name("journal-é\n.txt"), "journal-__.txt");
        assert_eq!(safe_name("a\\b"), "a_b");
        assert_eq!(safe_name("   "), "attachment");
        assert_eq!(safe_name(&"a".repeat(300)).len(), 255);
    }

    #[test]
    fn creds_last_a_day_after_the_end() {
        let now = Instant::now();
        let c = |ended: Option<Instant>| Creds {
            token: "t".into(),
            session_id: None,
            ended_at: ended,
            rated: false,
        };
        assert!(c(None).live(now));
        assert!(c(Some(now)).live(now + WINDOW));
        assert!(!c(Some(now)).live(now + WINDOW + Duration::from_secs(1)));
    }
}
