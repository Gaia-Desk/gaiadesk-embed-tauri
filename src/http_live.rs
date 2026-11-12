//! The plugin's own HTTP calls against a real GaiaDesk server. Ignored by default:
//! `GAIADESK_LIVE_KEY=<secret support key> cargo test --lib live -- --ignored`
//! (`GAIADESK_LIVE_API`, default `https://api.gaiadesk.net/v1`). Creates one session
//! and one listener on that key's account, and ends / revokes both.

use std::time::Duration;

use serde_json::{json, Value};

use super::*;

fn call(method: &str, url: &str, key: &str, body: Option<Value>) -> Value {
    let req = ureq::http::Request::builder()
        .method(method)
        .uri(url)
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json");
    let mut r = short()
        .run(
            req.body(body.map(|b| b.to_string()).unwrap_or_default())
                .unwrap(),
        )
        .unwrap();
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().unwrap_or_default();
    assert!(
        (200..300).contains(&status),
        "{method} {url}: {status} {text}"
    );
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

#[test]
#[ignore]
fn live_server() {
    let Ok(key) = std::env::var("GAIADESK_LIVE_KEY") else {
        panic!("set GAIADESK_LIVE_KEY")
    };
    let api = std::env::var("GAIADESK_LIVE_API").unwrap_or("https://api.gaiadesk.net/v1".into());

    // A secret key sent from an app origin is refused (the plugin never does this).
    let e = post_json(
        &format!("{api}/support/sessions"),
        &key,
        Some("app://com.example.live"),
        "{}",
    )
    .unwrap_err();
    assert_eq!(e.code, "secret_key_in_client", "{e:?}");

    // A session from the backend, as an app would get it.
    let s = call(
        "POST",
        &format!("{api}/support/sessions"),
        &key,
        Some(
            json!({"mode": "view", "external_id": "plugin-live-test", "customer": {"name": "Plugin live test"}}),
        ),
    );
    let c = created(&s).unwrap();
    let id = c.session_id.clone().unwrap();
    assert!(c.token.starts_with("gdemb_"));

    // attachDiagnostics' upload, with the embed token.
    let a = post_file(
        &format!("{api}/support/sessions/{id}/attachments"),
        &c.token,
        "diagnostics.txt",
        b"live test\n",
    )
    .unwrap();
    assert_eq!(
        a.get("filename").or(a.get("name")).and_then(Value::as_str),
        Some("diagnostics.txt"),
        "{a}"
    );

    // rate(): refused while open, accepted once ended, then already_rated.
    let csat = format!("{api}/support/sessions/{id}/csat");
    let early = post_json(&csat, &c.token, None, &json!({"rating": 5}).to_string()).unwrap_err();
    assert_eq!(early.code, "session_not_ended", "{early:?}");
    call(
        "POST",
        &format!("{api}/support/sessions/{id}/end"),
        &key,
        None,
    );
    post_json(
        &csat,
        &c.token,
        None,
        &json!({"rating": 5, "comment": "live test"}).to_string(),
    )
    .unwrap();
    let again = post_json(&csat, &c.token, None, &json!({"rating": 4}).to_string()).unwrap_err();
    assert_eq!(again.code, "already_rated", "{again:?}");

    // listen(): the request stream opens with `ready`.
    let l = call(
        "POST",
        &format!("{api}/support/listeners"),
        &key,
        Some(json!({"external_id": "plugin-live-test", "expires_in": 3600})),
    );
    let token = l["listen_token"].as_str().unwrap().to_string();
    let mut ready = false;
    let r = sse(
        &format!("{api}/support/requests/stream"),
        &token,
        Duration::from_secs(20),
        |ev, _| {
            ready = ev == "ready";
            !ready
        },
    );
    call(
        "DELETE",
        &format!("{api}/support/listeners/{}", l["id"].as_str().unwrap()),
        &key,
        None,
    );
    r.unwrap();
    assert!(ready, "no ready event");
}
