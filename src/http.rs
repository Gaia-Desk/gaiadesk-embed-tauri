//! Blocking HTTPS (call from `spawn_blocking`): creating a session with a
//! publishable key, and downloading a logo.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::error::{Error, Result};

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(TlsConfig::builder().provider(TlsProvider::Rustls).root_certs(RootCerts::PlatformVerifier).build())
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

/// GET at most `limit` bytes.
pub fn get_limited(url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut r = agent().get(url).call().map_err(|e| Error::new("http", e.to_string()))?;
    if !r.status().is_success() {
        return Err(Error::new("http", format!("{url}: HTTP {}", r.status().as_u16())));
    }
    let mut v = Vec::new();
    r.body_mut().as_reader().take(limit as u64).read_to_end(&mut v).map_err(|e| Error::new("http", e.to_string()))?;
    Ok(v)
}

/// `POST {api}/support/sessions` with a publishable key, as the app's
/// native origin. Returns the session's embed token.
pub fn create_session(api: &str, publishable_key: &str, origin: &str, body: &str) -> Result<String> {
    let mut r = agent()
        .post(format!("{api}/support/sessions"))
        .header("Authorization", format!("Bearer {publishable_key}"))
        .header("Origin", origin)
        .header("Content-Type", "application/json")
        .send(body)
        .map_err(|e| Error::new("create_failed", format!("could not reach GaiaDesk: {e}")))?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().unwrap_or_default();
    embed_token_of(status, &text)
}

/// The embed token from the server's answer, or its error. Pure.
pub fn embed_token_of(status: u16, body: &str) -> Result<String> {
    let j: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    match j.get("embed_token").and_then(Value::as_str) {
        Some(t) if (200..300).contains(&status) => Ok(t.to_string()),
        _ => {
            let reason = j.pointer("/error/reason").and_then(Value::as_str).map(str::to_string).unwrap_or(format!("http_{status}"));
            let message = j.pointer("/error/message").and_then(Value::as_str).unwrap_or("The support session could not be created.");
            Err(Error::new(&reason, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers() {
        assert_eq!(embed_token_of(201, r#"{"id":"ss_1","embed_token":"gdemb_ab"}"#).unwrap(), "gdemb_ab");
        let e = embed_token_of(429, r#"{"error":{"reason":"too_many_open_sessions","message":"Too many."}}"#).unwrap_err();
        assert_eq!((e.code.as_str(), e.message.as_str()), ("too_many_open_sessions", "Too many."));
        assert_eq!(embed_token_of(502, "<html>").unwrap_err().code, "http_502");
        assert_eq!(embed_token_of(500, r#"{"embed_token":"gdemb_x"}"#).unwrap_err().code, "http_500");
    }
}
