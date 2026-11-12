//! Blocking HTTPS (call from `spawn_blocking` or a thread of our own): the
//! session API, a logo download, and the session-request stream.

use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use serde_json::Value;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::error::{Error, Result};

fn agent(timeout: Option<Duration>, body_timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::Rustls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .http_status_as_error(false)
        .timeout_global(timeout)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(body_timeout)
        .build()
        .into()
}

fn short() -> ureq::Agent {
    agent(Some(Duration::from_secs(20)), None)
}

/// GET at most `limit` bytes.
pub fn get_limited(url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut r = short()
        .get(url)
        .call()
        .map_err(|e| Error::new("http", e.to_string()))?;
    if !r.status().is_success() {
        return Err(Error::new(
            "http",
            format!("{url}: HTTP {}", r.status().as_u16()),
        ));
    }
    let mut v = Vec::new();
    r.body_mut()
        .as_reader()
        .take(limit as u64)
        .read_to_end(&mut v)
        .map_err(|e| Error::new("http", e.to_string()))?;
    Ok(v)
}

/// `POST` a JSON body with a bearer token (and an `Origin` for a
/// publishable key). Returns the answer's JSON, or the server's error.
pub fn post_json(url: &str, bearer: &str, origin: Option<&str>, body: &str) -> Result<Value> {
    let mut req = short()
        .post(url)
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Content-Type", "application/json");
    if let Some(o) = origin {
        req = req.header("Origin", o);
    }
    let mut r = req
        .send(body)
        .map_err(|e| Error::new("network", format!("could not reach GaiaDesk: {e}")))?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().unwrap_or_default();
    answer(status, &text)
}

/// `GET` JSON with a bearer token.
pub fn get_json(url: &str, bearer: &str) -> Result<Value> {
    let mut r = short()
        .get(url)
        .header("Authorization", format!("Bearer {bearer}"))
        .call()
        .map_err(|e| Error::new("network", format!("could not reach GaiaDesk: {e}")))?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().unwrap_or_default();
    answer(status, &text)
}

/// `POST` a file's bytes (`X-Filename` names it), with a bearer token.
pub fn post_file(url: &str, bearer: &str, filename: &str, body: &[u8]) -> Result<Value> {
    let mut r = agent(Some(Duration::from_secs(300)), None)
        .post(url)
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Content-Type", "application/octet-stream")
        .header("X-Filename", filename)
        .send(body)
        .map_err(|e| Error::new("network", format!("could not reach GaiaDesk: {e}")))?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().unwrap_or_default();
    answer(status, &text)
}

/// A 2xx answer's JSON, or the server's `{error: {reason, message}}` as an
/// Error (`http_<status>` when it has none). Pure.
pub fn answer(status: u16, body: &str) -> Result<Value> {
    let j: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if (200..300).contains(&status) && j.is_object() {
        return Ok(j);
    }
    let reason = j
        .pointer("/error/reason")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(format!("http_{status}"));
    let message = j
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("GaiaDesk refused the request.");
    Err(Error::new(&reason, message))
}

/// A session the server created (with a publishable key, or by accepting a
/// request).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Created {
    pub token: String,
    pub session_id: Option<String>,
    pub estimated_wait_seconds: Option<u64>,
}

/// The session out of a create's answer. Pure.
pub fn created(j: &Value) -> Result<Created> {
    let token = j
        .get("embed_token")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("create_failed", "the server's answer had no embed_token"))?;
    Ok(Created {
        token: token.to_string(),
        session_id: j.get("id").and_then(Value::as_str).map(str::to_string),
        estimated_wait_seconds: j.get("estimated_wait_seconds").and_then(Value::as_u64),
    })
}

/// `POST {api}/support/sessions` with a publishable key, as the app's
/// native origin.
pub fn create_session(
    api: &str,
    publishable_key: &str,
    origin: &str,
    body: &str,
) -> Result<Created> {
    let j = post_json(
        &format!("{api}/support/sessions"),
        publishable_key,
        Some(origin),
        body,
    )
    .map_err(|e| match e.code.as_str() {
        "network" => Error::new("create_failed", e.message),
        _ => e,
    })?;
    created(&j)
}

/// Server-Sent Events, one `(event, data)` at a time. Pure.
#[derive(Debug, Default)]
pub struct SseParser {
    event: String,
    data: Vec<String>,
}

impl SseParser {
    /// Feed one line (without its line break); a blank line ends an event.
    pub fn line(&mut self, l: &str) -> Option<(String, String)> {
        if l.is_empty() {
            let ev = std::mem::take(&mut self.event);
            let data = std::mem::take(&mut self.data).join("\n");
            return (!data.is_empty())
                .then(|| (if ev.is_empty() { "message".into() } else { ev }, data));
        }
        if l.starts_with(':') {
            return None;
        }
        let (field, value) = l.split_once(':').unwrap_or((l, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.event = value.to_string(),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
        None
    }
}

/// `GET` an event stream and hand each event to `on` until the server ends
/// it, `on` returns `false`, or `max` passes (then `Ok`: reconnect). A
/// refusal is the server's error.
pub fn sse(
    url: &str,
    bearer: &str,
    max: Duration,
    mut on: impl FnMut(&str, &str) -> bool,
) -> Result<()> {
    let mut r = agent(None, Some(max))
        .get(url)
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Accept", "text/event-stream")
        .call()
        .map_err(|e| Error::new("network", e.to_string()))?;
    let status = r.status().as_u16();
    if !(200..300).contains(&status) {
        let text = r.body_mut().read_to_string().unwrap_or_default();
        return answer(status, &text).map(|_| ());
    }
    let mut p = SseParser::default();
    for line in BufReader::new(r.body_mut().as_reader()).lines() {
        // A timeout or a dropped connection ends this round; the caller reconnects.
        let Ok(line) = line else { return Ok(()) };
        if let Some((ev, data)) = p.line(line.trim_end_matches('\r')) {
            if !on(&ev, &data) {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers() {
        let c = created(
            &answer(
                201,
                r#"{"id":"ss_1","embed_token":"gdemb_ab","estimated_wait_seconds":45}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            c,
            Created {
                token: "gdemb_ab".into(),
                session_id: Some("ss_1".into()),
                estimated_wait_seconds: Some(45)
            }
        );
        let c = created(
            &answer(
                201,
                r#"{"embed_token":"gdemb_ab","estimated_wait_seconds":null}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(c.estimated_wait_seconds, None);
        let e = answer(
            429,
            r#"{"error":{"reason":"too_many_open_sessions","message":"Too many."}}"#,
        )
        .unwrap_err();
        assert_eq!(
            (e.code.as_str(), e.message.as_str()),
            ("too_many_open_sessions", "Too many.")
        );
        assert_eq!(answer(502, "<html>").unwrap_err().code, "http_502");
        assert_eq!(
            answer(500, r#"{"embed_token":"gdemb_x"}"#)
                .unwrap_err()
                .code,
            "http_500"
        );
        assert_eq!(answer(200, "not json").unwrap_err().code, "http_200");
        assert_eq!(
            created(&serde_json::json!({})).unwrap_err().code,
            "create_failed"
        );
    }

    #[test]
    fn sse_events() {
        let mut p = SseParser::default();
        let text = ": keep-alive\n\nevent: ready\ndata: {\"a\":1}\n\nevent: support_request\ndata: x\ndata:y\n\ndata: plain\n\n";
        let got: Vec<_> = text.split('\n').filter_map(|l| p.line(l)).collect();
        assert_eq!(
            got,
            vec![
                ("ready".to_string(), "{\"a\":1}".to_string()),
                ("support_request".to_string(), "x\ny".to_string()),
                ("message".to_string(), "plain".to_string()),
            ]
        );
    }
}

#[cfg(test)]
#[path = "http_live.rs"]
mod live;
