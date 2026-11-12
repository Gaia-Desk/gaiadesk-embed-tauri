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
    /// The company's typical wait for an agent, seconds, when the server has
    /// an estimate (from the session's create or accept).
    pub estimated_wait_seconds: Option<u64>,
    /// What the session runs with (the consent step's choices), from the
    /// `waiting` state: `{guided, share_logs, capture}`.
    pub choices: Option<Value>,
}

impl Status {
    pub fn idle() -> Self {
        Status {
            state: "idle".into(),
            ..Status::default()
        }
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
            let mut p = json!({ "state": st });
            if let Some(c) = e.get("choices").filter(|c| c.is_object()) {
                status.choices = Some(c.clone());
                p["choices"] = c.clone();
            }
            Some(("state", p))
        }
        "code" => {
            status.code = s("code");
            status.session_id = s("session_id");
            status.mode = s("mode");
            if let Some(w) = e.get("estimated_wait_seconds").and_then(Value::as_u64) {
                status.estimated_wait_seconds = Some(w);
            }
            Some((
                "started",
                json!({ "code": status.code, "sessionId": status.session_id, "mode": status.mode, "expiresAt": e.get("expires_at").and_then(Value::as_f64), "estimatedWaitSeconds": status.estimated_wait_seconds }),
            ))
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
        "action" => Some((
            "action",
            json!({ "action": s("action")?, "applied": e.get("applied")?.as_bool()?, "reason": s("reason") }),
        )),
        // The customer dragged the indicator: kept by the plugin, not emitted.
        "indicator_moved" => Some((
            "indicator_moved",
            json!({ "x": e.get("x")?.as_f64()?, "y": e.get("y")?.as_f64()? }),
        )),
        "error" => Some((
            "error",
            json!({ "code": s("code")?, "message": s("message").unwrap_or_default() }),
        )),
        "ended" => {
            let reason = s("reason")
                .filter(|r| {
                    [
                        "stopped",
                        "app_stopped",
                        "customer_cancelled",
                        "customer_declined",
                        "consent_timeout",
                        "disconnected",
                        "expired",
                        "error",
                    ]
                    .contains(&r.as_str())
                })
                .unwrap_or_else(|| "error".into());
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
        run_from(Status::idle(), events)
    }

    fn run_from(mut st: Status, events: &[&str]) -> (Vec<(&'static str, Value)>, Status) {
        let out = events.iter().filter_map(|e| map(e, &mut st)).collect();
        (out, st)
    }

    #[test]
    fn a_whole_session() {
        let mut st = Status::idle();
        st.estimated_wait_seconds = Some(45);
        let (out, st) = run_from(
            st,
            &[
                r#"{"type":"state","state":"starting"}"#,
                r#"{"type":"code","code":"123456789","session_id":"ss_1","mode":"cobrowse","expires_at":1700000000}"#,
                r#"{"type":"state","state":"waiting"}"#,
                r#"{"type":"joined","agent":"ana@acme.com"}"#,
                r#"{"type":"control","allowed":false}"#,
                r#"{"type":"action","action":"click","applied":false,"reason":"control_paused"}"#,
                r#"{"type":"error","code":"capture_failed","message":"no window"}"#,
                r#"{"type":"state","state":"ended"}"#,
                r#"{"type":"ended","reason":"stopped"}"#,
            ],
        );
        let names: Vec<_> = out.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            [
                "state", "started", "state", "joined", "paused", "action", "error", "state",
                "ended"
            ]
        );
        assert_eq!(
            out[1].1,
            json!({ "code": "123456789", "sessionId": "ss_1", "mode": "cobrowse", "expiresAt": 1700000000.0, "estimatedWaitSeconds": 45 })
        );
        assert_eq!(out[3].1, json!({ "agent": "ana@acme.com" }));
        assert_eq!(out[4].1, json!({ "paused": true }));
        assert_eq!(
            out[5].1,
            json!({ "action": "click", "applied": false, "reason": "control_paused" })
        );
        assert_eq!(
            out[6].1,
            json!({ "code": "capture_failed", "message": "no window" })
        );
        assert_eq!(out[8].1, json!({ "reason": "stopped" }));
        assert_eq!(st.state, "ended");
        assert_eq!(st.code.as_deref(), Some("123456789"));
        assert_eq!(st.end_reason.as_deref(), Some("stopped"));
        assert!(st.agent.is_none() && st.paused && !st.active());
        // A library that sends its own estimate wins.
        let (out, _) = run(&[r#"{"type":"code","code":"1","estimated_wait_seconds":30}"#]);
        assert_eq!(out[0].1["estimatedWaitSeconds"], json!(30));
    }

    #[test]
    fn unknown_and_malformed_events_are_skipped_but_ended_is_never_lost() {
        let (out, st) = run(&[
            r#"{"type":"notice","code":"x"}"#,
            "not json",
            r#"{"type":"joined"}"#,
            r#"{"type":"ended","reason":"new_reason"}"#,
        ]);
        assert_eq!(
            out,
            vec![
                ("joined", json!({ "agent": null })),
                ("ended", json!({ "reason": "error" }))
            ]
        );
        assert_eq!(st.state, "ended");
    }

    #[test]
    fn left_clears_the_agent_and_control_resumes() {
        let (_, st) = run(&[
            r#"{"type":"joined","agent":"a"}"#,
            r#"{"type":"left"}"#,
            r#"{"type":"control","allowed":false}"#,
            r#"{"type":"control","allowed":true}"#,
        ]);
        assert!(st.agent.is_none() && !st.paused);
    }

    #[test]
    fn a_cancel_while_waiting_is_its_own_reason_and_a_drag_is_kept_not_emitted() {
        let (out, st) = run(&[
            r#"{"type":"state","state":"waiting"}"#,
            r#"{"type":"indicator_moved","x":120.5,"y":-30}"#,
            r#"{"type":"ended","reason":"customer_cancelled"}"#,
        ]);
        assert_eq!(
            out[1],
            ("indicator_moved", json!({ "x": 120.5, "y": -30.0 }))
        );
        assert_eq!(out[2], ("ended", json!({ "reason": "customer_cancelled" })));
        assert_eq!(st.end_reason.as_deref(), Some("customer_cancelled"));
    }

    #[test]
    fn waiting_carries_the_choices_and_a_declined_consent_ends_with_its_reason() {
        let (out, st) = run(&[
            r#"{"type":"state","state":"waiting","choices":{"guided":false,"share_logs":true,"capture":"window"}}"#,
            r#"{"type":"ended","reason":"customer_declined"}"#,
        ]);
        let choices = json!({ "guided": false, "share_logs": true, "capture": "window" });
        assert_eq!(
            out[0],
            ("state", json!({ "state": "waiting", "choices": choices }))
        );
        assert_eq!(st.choices, Some(choices));
        assert_eq!(out[1].1, json!({ "reason": "customer_declined" }));
        let (out, _) = run(&[r#"{"type":"ended","reason":"consent_timeout"}"#]);
        assert_eq!(out[0].1, json!({ "reason": "consent_timeout" }));
    }
}
