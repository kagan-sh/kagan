use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use gpui_kit::TestAppContext;
use kg_domain::{
    Op, OpError, OpResult, PreferenceOp, UsageAccount, UsageOp, UsageProvider, UsageStatus,
    UsageView,
};
use serde_json::{Value, json};

use super::Fixture;
use crate::Workspace;

/// A local stand-in for the usage endpoint. It records every request head and answers with the
/// status and body the test last set.
#[derive(Clone)]
struct Endpoint {
    url: String,
    hits: Arc<AtomicUsize>,
    heads: Arc<Mutex<Vec<String>>>,
    reply: Arc<Mutex<(u16, String)>>,
}

impl Endpoint {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Self {
            url: format!("http://{}/wham/usage", listener.local_addr().unwrap()),
            hits: Arc::default(),
            heads: Arc::default(),
            reply: Arc::new(Mutex::new((200, usage_reply()))),
        };
        let shared = endpoint.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() <= 2 {
                        break;
                    }
                    head.push_str(&line);
                }
                shared.hits.fetch_add(1, Ordering::SeqCst);
                shared.heads.lock().unwrap().push(head);
                let (status, body) = shared.reply.lock().unwrap().clone();
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        endpoint
    }

    fn answer(&self, status: u16, body: &str) {
        *self.reply.lock().unwrap() = (status, body.to_owned());
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    fn last_head(&self) -> String {
        self.heads
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

fn usage_reply() -> String {
    json!({
        "plan_type": "plus",
        "rate_limit": {
            "primary_window": {
                "used_percent": 40.0,
                "reset_at": 1_900_000_000,
                "limit_window_seconds": 18_000
            },
            "secondary_window": {
                "used_percent": 18.4,
                "reset_at": 1_900_500_000,
                "limit_window_seconds": 604_800
            }
        }
    })
    .to_string()
}

/// An unsigned JWT with the given payload, which is all kagan reads from it.
fn jwt(payload: &Value) -> String {
    let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("eyJhbGciOiJub25lIn0.{body}.signature")
}

/// Writes a sign-in file and returns its access token, which tests check is sent and never shown.
fn sign_in(home: &Path, expires: i64) -> String {
    let access = jwt(&json!({
        "exp": expires,
        "https://api.openai.com/auth": { "chatgpt_plan_type": "plus", "chatgpt_account_id": "acct-1" },
        "https://api.openai.com/profile": { "email": "person@example.test" }
    }));
    let auth = json!({
        "tokens": { "access_token": access, "id_token": jwt(&json!({})), "account_id": "acct-1" }
    });
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(home.join("auth.json"), auth.to_string()).unwrap();
    access
}

fn far_future() -> i64 {
    4_102_444_800
}

fn past() -> i64 {
    1_000_000_000
}

async fn op(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    cx.update(|cx| crate::Workspace::dispatch(op, cx)).await
}

async fn set_enabled(cx: &mut TestAppContext, on: bool) {
    op(
        cx,
        Op::Preference(PreferenceOp::Set {
            key: "usage-limits".into(),
            value: json!(on),
        }),
    )
    .await
    .unwrap();
}

async fn show(cx: &mut TestAppContext, usage: UsageOp) -> UsageView {
    let Ok(OpResult::Usage(view)) = op(cx, Op::Usage(usage)).await else {
        panic!("usage did not answer with a view");
    };
    view
}

fn point_at(cx: &mut TestAppContext, home: Option<&Path>, url: &str) {
    let usage = cx.update(|cx| Workspace::global(cx).read(cx).usage.clone());
    usage.use_sources(home.map(Path::to_path_buf), url.to_owned());
}

fn codex(view: &UsageView) -> Option<&UsageAccount> {
    view.accounts
        .iter()
        .find(|account| account.provider == UsageProvider::Codex)
}

#[gpui_kit::test]
async fn nothing_is_read_while_the_preference_is_off(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);

    let view = show(cx, UsageOp::Show).await;
    assert!(!view.enabled);
    assert!(view.accounts.is_empty());
    let view = show(cx, UsageOp::Refresh).await;
    assert!(!view.enabled);
    assert_eq!(endpoint.hits(), 0);
}

#[gpui_kit::test]
async fn a_signed_in_codex_reading_shows_its_windows(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    let token = sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Show).await;
    assert!(view.enabled);
    let account = codex(&view).expect("codex account");
    assert_eq!(account.status, UsageStatus::Ok);
    assert_eq!(account.email.as_deref(), Some("person@example.test"));
    assert_eq!(account.plan.as_deref(), Some("plus"));
    let labels: Vec<_> = account.windows.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(labels, ["5h", "7d"]);
    assert!((account.windows[0].percent - 40.0).abs() < f64::EPSILON);
    assert_eq!(account.windows[0].resets_at, Some(1_900_000_000));
    assert_eq!(endpoint.hits(), 1);

    let head = endpoint.last_head();
    assert!(
        head.contains(&format!("uthorization: Bearer {token}"))
            || head.contains(&format!("uthorization: bearer {token}"))
    );
    assert!(head.contains("chatgpt-account-id: acct-1"));
    assert!(!format!("{view:?}").contains(&token));
}

#[gpui_kit::test]
async fn show_asks_again_only_after_two_minutes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    show(cx, UsageOp::Show).await;
    show(cx, UsageOp::Show).await;
    assert_eq!(endpoint.hits(), 1);
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 2);
}

#[gpui_kit::test]
async fn an_expired_token_is_not_sent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, past());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert_eq!(codex(&view).unwrap().status, UsageStatus::Expired);
    assert_eq!(endpoint.hits(), 0);
}

#[gpui_kit::test]
async fn a_rejected_token_is_expired_and_never_refreshed(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    endpoint.answer(401, "{}");
    let home = fixture.dir.path().join("codex");
    let token = sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;
    let before = std::fs::read_to_string(home.join("auth.json")).unwrap();

    let view = show(cx, UsageOp::Refresh).await;
    assert_eq!(codex(&view).unwrap().status, UsageStatus::Expired);
    assert_eq!(
        endpoint.hits(),
        1,
        "one request, and no refresh call after it"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("auth.json")).unwrap(),
        before
    );
    assert!(!format!("{view:?}").contains(&token));
}

#[gpui_kit::test]
async fn a_failed_refresh_keeps_the_last_reading_as_stale(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;
    show(cx, UsageOp::Refresh).await;

    endpoint.answer(500, "boom");
    let view = show(cx, UsageOp::Refresh).await;
    let account = codex(&view).unwrap();
    assert_eq!(account.status, UsageStatus::Stale);
    assert_eq!(account.windows.len(), 2, "the last good windows stay");
}

#[gpui_kit::test]
async fn a_failed_first_reading_is_an_error_not_zero(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    endpoint.answer(500, "boom");
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    let account = codex(&view).unwrap();
    assert!(matches!(account.status, UsageStatus::Error { .. }));
    assert!(account.windows.is_empty());
}

#[gpui_kit::test]
async fn no_sign_in_means_no_account_and_no_request(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    point_at(cx, Some(&fixture.dir.path().join("missing")), &endpoint.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert!(view.enabled);
    assert!(view.accounts.is_empty());
    assert_eq!(endpoint.hits(), 0);
}

#[gpui_kit::test]
async fn an_api_key_sign_in_has_no_limits(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("auth.json"),
        json!({ "OPENAI_API_KEY": "sk-fixture" }).to_string(),
    )
    .unwrap();
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert_eq!(codex(&view).unwrap().status, UsageStatus::NoLimits);
    assert_eq!(endpoint.hits(), 0);
}

#[gpui_kit::test]
async fn an_endpoint_that_is_not_https_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), "http://example.test/wham/usage");
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert_eq!(
        codex(&view).unwrap().status,
        UsageStatus::Error {
            message: "Usage only loads over HTTPS.".into()
        }
    );
}

#[gpui_kit::test]
async fn turning_the_preference_off_drops_the_readings(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;
    show(cx, UsageOp::Refresh).await;

    set_enabled(cx, false).await;
    let view = show(cx, UsageOp::Show).await;
    assert!(!view.enabled);
    assert!(view.accounts.is_empty());

    set_enabled(cx, true).await;
    show(cx, UsageOp::Show).await;
    assert_eq!(
        endpoint.hits(),
        2,
        "a turned-off reading is read again, not kept"
    );
}
