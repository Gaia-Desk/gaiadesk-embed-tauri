//! Session requests: an agent asks this customer's app for a session
//! (GaiaDesk's "Ask to share"). Your backend mints a listen token for the
//! signed-in user (`POST /v1/support/listeners`, a secret key); the app
//! `listen`s with it, hears `request` events, and answers with
//! `startHelp({ requestId })` (the customer's consent is still asked) or
//! `decline_request`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::commands::{blocking, company_of};
use crate::error::{Error, Result};
use crate::events::PREFIX;
use crate::http::{self, Created};
use crate::{lock, Support};

/// A listen token's prefix.
pub const LISTEN_PREFIX: &str = "gdlis_";
/// One stream round: a connection that died silently is noticed by then.
// shortcut: ureq has no per-read timeout, so a dead connection is caught by this cap; lower it if requests arrive late.
const ROUND: Duration = Duration::from_secs(120);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

fn emit<R: Runtime>(app: &AppHandle<R>, name: &str, payload: Value) {
    let _ = app.emit(&format!("{PREFIX}{name}"), payload);
}

/// A stream event's request → the Tauri event to emit: `request` while it is
/// pending, `request_closed` once it is not. Pure.
pub fn request_event(data: &str, company: &str) -> Option<(&'static str, String, String, Value)> {
    let j: Value = serde_json::from_str(data).ok()?;
    let q = j.get("request")?;
    let s = |k: &str| q.get(k).and_then(Value::as_str);
    let (id, state) = (s("id")?.to_string(), s("state")?.to_string());
    let payload = if state == "pending" {
        (
            "request",
            json!({ "id": id, "company": company, "mode": s("mode"), "message": s("message"), "expiresAt": q.get("expires_at").and_then(Value::as_i64) }),
        )
    } else {
        ("request_closed", json!({ "id": id, "state": state }))
    };
    Some((payload.0, id, state, payload.1))
}

/// Whether a refusal means the token is done for (revoked, expired, wrong). Pure.
pub fn fatal(e: &Error) -> bool {
    matches!(
        e.code.as_str(),
        "invalid_listen_token" | "http_401" | "http_403" | "http_404" | "unauthenticated"
    )
}

/// Listen for session requests with `listenToken` (`gdlis_…`), replacing any
/// earlier listener. Events: `listening`, `request`, `request_closed`,
/// `listen_stopped {reason}`.
#[tauri::command]
pub(crate) fn listen<R: Runtime>(app: AppHandle<R>, listen_token: String) -> Result<()> {
    let token = listen_token.trim().to_string();
    if !token.starts_with(LISTEN_PREFIX) {
        return Err(Error::new(
            "bad_options",
            "listenToken is a gdlis_… token your backend minted (POST /v1/support/listeners)",
        ));
    }
    let sup = app.state::<Support>();
    let gen = sup.listen_gen.fetch_add(1, Ordering::SeqCst) + 1;
    *lock(&sup.listen_token) = Some(token.clone());
    std::thread::Builder::new()
        .name("gaiadesk-support-listen".into())
        .spawn(move || run(app, token, gen))
        .map_err(|e| Error::new("internal", e.to_string()))?;
    Ok(())
}

fn run<R: Runtime>(app: AppHandle<R>, token: String, gen: u64) {
    let current = || app.state::<Support>().listen_gen.load(Ordering::SeqCst) == gen;
    let url = format!("{}/support/requests/stream", app.state::<Support>().api());
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut backoff = Duration::from_secs(1);
    while current() {
        let company = company_of(&app);
        let r = http::sse(&url, &token, ROUND, |ev, data| {
            if !current() {
                return false;
            }
            backoff = Duration::from_secs(1);
            if ev == "ready" {
                emit(&app, "listening", json!({}));
            } else if let Some((name, id, state, payload)) = request_event(data, &company) {
                // Every reconnect replays what is pending: tell the app once per change.
                if seen.insert(id, state.clone()).as_deref() != Some(state.as_str()) {
                    emit(&app, name, payload);
                }
            }
            true
        });
        if !current() {
            return;
        }
        match r {
            Err(e) if fatal(&e) => {
                emit(
                    &app,
                    "listen_stopped",
                    json!({ "reason": e.code, "message": e.message }),
                );
                let sup = app.state::<Support>();
                if sup
                    .listen_gen
                    .compare_exchange(gen, gen + 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    *lock(&sup.listen_token) = None;
                }
                return;
            }
            Err(_) => {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
            // The server ended the round (or ours ran out): go again, gently.
            Ok(()) => std::thread::sleep(Duration::from_secs(1)),
        }
    }
}

/// Stop listening (a thread mid-read quits at its next event or keep-alive,
/// and emits nothing more).
#[tauri::command]
pub(crate) fn stop_listening<R: Runtime>(app: AppHandle<R>) {
    stop_now(&app);
}

pub(crate) fn stop_now<R: Runtime>(app: &AppHandle<R>) {
    if let Some(sup) = app.try_state::<Support>() {
        sup.listen_gen.fetch_add(1, Ordering::SeqCst);
        *lock(&sup.listen_token) = None;
    }
}

fn listen_token<R: Runtime>(app: &AppHandle<R>) -> Result<String> {
    lock(&app.state::<Support>().listen_token)
        .clone()
        .ok_or_else(|| Error::new("not_listening", "call listen({ listenToken }) first"))
}

fn request_url<R: Runtime>(app: &AppHandle<R>, id: &str, verb: &str) -> Result<String> {
    if !id.starts_with("sr_") || !id[3..].bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::new(
            "bad_options",
            "requestId is an sr_… id from a request event",
        ));
    }
    Ok(format!(
        "{}/support/requests/{id}/{verb}",
        app.state::<Support>().api()
    ))
}

/// Accept request `id` (the customer agreed): its new session.
pub(crate) fn accept<R: Runtime>(app: &AppHandle<R>, id: &str, client: &Value) -> Result<Created> {
    let url = request_url(app, id, "accept")?;
    let j = http::post_json(
        &url,
        &listen_token(app)?,
        None,
        &json!({ "client": client }).to_string(),
    )?;
    http::created(&j)
}

/// Decline request `id`.
pub(crate) fn decline<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<()> {
    let url = request_url(app, id, "decline")?;
    http::post_json(&url, &listen_token(app)?, None, "{}").map(|_| ())
}

#[tauri::command]
pub(crate) async fn decline_request<R: Runtime>(app: AppHandle<R>, id: String) -> Result<()> {
    blocking(move || decline(&app, &id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_requests_become_events() {
        let pending = r#"{"event":"support_request","request":{"id":"sr_1","state":"pending","mode":"cobrowse","message":"Let us look","expires_at":1791300600,"owner":"x@y"}}"#;
        let (name, id, state, p) = request_event(pending, "Acme").unwrap();
        assert_eq!(
            (name, id.as_str(), state.as_str()),
            ("request", "sr_1", "pending")
        );
        assert_eq!(
            p,
            json!({ "id": "sr_1", "company": "Acme", "mode": "cobrowse", "message": "Let us look", "expiresAt": 1791300600 })
        );
        let gone = r#"{"event":"support_request","request":{"id":"sr_1","state":"expired"}}"#;
        assert_eq!(
            request_event(gone, "Acme").unwrap().3,
            json!({ "id": "sr_1", "state": "expired" })
        );
        assert!(request_event(r#"{"event":"ready","listener":{}}"#, "A").is_none());
        assert!(request_event("nope", "A").is_none());
    }

    #[test]
    fn a_dead_token_stops_listening_but_a_network_error_retries() {
        for code in ["invalid_listen_token", "http_401", "http_404"] {
            assert!(fatal(&Error::new(code, "")), "{code}");
        }
        for code in ["network", "http_502", "rate_limited"] {
            assert!(!fatal(&Error::new(code, "")), "{code}");
        }
    }
}
