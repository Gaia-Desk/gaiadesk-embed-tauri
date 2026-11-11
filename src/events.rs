//! The library's event JSON → Tauri events (`gaiadesk-support:<name>`) and
//! the session status. Pure.

use serde::Serialize;
use serde_json::{json, Value};

/// Every Tauri event this plugin emits is `PREFIX` + a name below.
pub const PREFIX: &str = "gaiadesk-support:";

/// Where the session is, for `status()`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `idle`, `consent`, `starting`, `waiting`, `connected`, `ended`.
    pub state: String,
    /// The nine-digit support code, once the session is up.
    pub code: Option<String>,
    pub session_id: Option<String>,
    pub mode: Option<String>,
    /// The agent who joined (their email), while one is connected.
    pub agent: Option<String>,
    /// Guided input is paused (by the customer or the app).
    pub paused: bool,
    pub end_reason: Option<String>,
}

impl Status {
    pub fn idle() -> Self {
        Status { state: "idle".into(), ..Status::default() }
    }

    /// A session is up (not idle, not ended).
    pub fn active(&self) -> bool {
        !matches!(self.state.as_str(), "idle" | "ended")
    }
}

/// One library event → the Tauri event to emit (`started`, `joined`, `left`,
/// `paused`, `action`, `error`, `ended`, `state`), its payload, and the
/// status updated. `None` for a type this version does not know (an `ended`
/// is never lost: an unknown reason reads as `error`).
pub fn map(json: &str, status: &mut Status) -> Option<(&'static str, Value)> {
    let e: Value = serde_json::from_str(json).ok()?;
    let s = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
    match e.get("type")?.as_str()? {
        "state" => {
            let st = s("state")?;
            if st != "ended" {
                status.state = st.clone();
            }
            Some(("state", json!({ "state": st })))
        }
        "code" => {
            status.code = s("code");
            status.session_id = s("session_id");
            status.mode = s("mode");
            Some(("started", json!({ "code": status.code, "sessionId": status.session_id, "mode": status.mode, "expiresAt": e.get("expires_at").and_then(Value::as_f64) })))
        }
        "joined" => {
            status.agent = s("agent");
            Some(("joined", json!({ "agent": status.agent })))
        }
        "left" => {
            status.agent = None;
            Some(("left", json!({})))
        }
        "control" => {
            status.paused = !e.get("allowed")?.as_bool()?;
            Some(("paused", json!({ "paused": status.paused })))
        }
        "action" => Some(("action", json!({ "action": s("action")?, "applied": e.get("applied")?.as_bool()?, "reason": s("reason") }))),
        "error" => Some(("error", json!({ "code": s("code")?, "message": s("message").unwrap_or_default() }))),
        "ended" => {
            let reason = s("reason").filter(|r| ["stopped", "app_stopped", "disconnected", "expired", "error"].contains(&r.as_str())).unwrap_or_else(|| "error".into());
            status.state = "ended".into();
            status.agent = None;
            status.end_reason = Some(reason.clone());
            Some(("ended", json!({ "reason": reason })))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(events: &[&str]) -> (Vec<(&'static str, Value)>, Status) {
        let mut st = Status::idle();
        let out = events.iter().filter_map(|e| map(e, &mut st)).collect();
        (out, st)
    }

    #[test]
    fn a_whole_session() {
        let (out, st) = run(&[
            r#"{"type":"state","state":"starting"}"#,
            r#"{"type":"code","code":"123456789","session_id":"ss_1","mode":"cobrowse","expires_at":1700000000}"#,
            r#"{"type":"state","state":"waiting"}"#,
            r#"{"type":"joined","agent":"ana@acme.com"}"#,
            r#"{"type":"control","allowed":false}"#,
            r#"{"type":"action","action":"click","applied":false,"reason":"control_paused"}"#,
            r#"{"type":"error","code":"capture_failed","message":"no window"}"#,
            r#"{"type":"state","state":"ended"}"#,
            r#"{"type":"ended","reason":"stopped"}"#,
        ]);
        let names: Vec<_> = out.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["state", "started", "state", "joined", "paused", "action", "error", "state", "ended"]);
        assert_eq!(out[1].1, json!({ "code": "123456789", "sessionId": "ss_1", "mode": "cobrowse", "expiresAt": 1700000000.0 }));
        assert_eq!(out[3].1, json!({ "agent": "ana@acme.com" }));
        assert_eq!(out[4].1, json!({ "paused": true }));
        assert_eq!(out[5].1, json!({ "action": "click", "applied": false, "reason": "control_paused" }));
        assert_eq!(out[6].1, json!({ "code": "capture_failed", "message": "no window" }));
        assert_eq!(out[8].1, json!({ "reason": "stopped" }));
        assert_eq!(st.state, "ended");
        assert_eq!(st.code.as_deref(), Some("123456789"));
        assert_eq!(st.end_reason.as_deref(), Some("stopped"));
        assert!(st.agent.is_none() && st.paused && !st.active());
    }

    #[test]
    fn unknown_and_malformed_events_are_skipped_but_ended_is_never_lost() {
        let (out, st) = run(&[r#"{"type":"notice","code":"x"}"#, "not json", r#"{"type":"joined"}"#, r#"{"type":"ended","reason":"new_reason"}"#]);
        assert_eq!(out, vec![("joined", json!({ "agent": null })), ("ended", json!({ "reason": "error" }))]);
        assert_eq!(st.state, "ended");
    }

    #[test]
    fn left_clears_the_agent_and_control_resumes() {
        let (_, st) = run(&[r#"{"type":"joined","agent":"a"}"#, r#"{"type":"left"}"#, r#"{"type":"control","allowed":false}"#, r#"{"type":"control","allowed":true}"#]);
        assert!(st.agent.is_none() && !st.paused);
    }
}
