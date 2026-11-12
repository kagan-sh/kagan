use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use gpui_kit::TestAppContext;
use kg_domain::{
    Op, OpError, OpResult, PreferenceOp, UsageAccount, UsageOp, UsageProvider, UsageStatus,
    UsageView,
};
use serde_json::{Value, json};

use super::Fixture;
use crate::Workspace;

#[derive(Clone)]
struct Endpoint {
    url: String,
    hits: Arc<AtomicUsize>,
    heads: Arc<Mutex<Vec<String>>>,
    reply: Arc<Mutex<(u16, String, Option<String>)>>,
}

impl Endpoint {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Self {
            url: format!("http://{}/wham/usage", listener.local_addr().unwrap()),
            hits: Arc::default(),
            heads: Arc::default(),
            reply: Arc::new(Mutex::new((200, usage_reply(), None))),
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
                let (status, body, location) = shared.reply.lock().unwrap().clone();
                let location = location.map_or(String::new(), |to| format!("Location: {to}\r\n"));
                let response = format!(
                    "HTTP/1.1 {status} X\r\n{location}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        endpoint
    }

    fn answer(&self, status: u16, body: &str) {
        *self.reply.lock().unwrap() = (status, body.to_owned(), None);
    }

    fn redirect(&self, to: &str) {
        *self.reply.lock().unwrap() = (302, String::new(), Some(to.to_owned()));
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

/// A server that accepts a request and never answers, so only the client's timeout ends it.
fn silent() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/wham/usage", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::sleep(Duration::from_secs(3));
            drop(stream);
        }
    });
    url
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
    let home: Option<PathBuf> = home.map(Path::to_path_buf);
    cx.update(|cx| crate::use_codex_source(cx, home, url.to_owned()));
}

fn advance(cx: &mut TestAppContext, by: Duration) {
    cx.update(|cx| crate::advance_usage_clock(cx, by));
}

fn cached(cx: &mut TestAppContext) -> Vec<UsageAccount> {
    cx.update(|cx| Workspace::global(cx).read(cx).usage.accounts())
}

fn codex(view: &UsageView) -> Option<&UsageAccount> {
    view.accounts
        .iter()
        .find(|account| account.provider == UsageProvider::Codex)
}

fn error_text(view: &UsageView) -> String {
    match &codex(view).unwrap().status {
        UsageStatus::Error { message } => message.clone(),
        other => panic!("expected an error, got {other:?}"),
    }
}

#[gpui_kit::test]
async fn the_preference_is_on_until_someone_saves_it(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let view = show(cx, UsageOp::Show).await;
    assert!(view.enabled, "usage is on when nothing is saved");
}

#[gpui_kit::test]
async fn nothing_is_read_while_the_preference_is_off(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, false).await;

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
    assert!(head.to_ascii_lowercase().contains("authorization: bearer "));
    assert!(head.contains(&token));
    assert!(head.contains("chatgpt-account-id: acct-1"));
    assert!(!serde_json::to_string(&view).unwrap().contains(&token));
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
    advance(cx, Duration::from_secs(60));
    show(cx, UsageOp::Show).await;
    assert_eq!(
        endpoint.hits(),
        1,
        "a reading under two minutes old is kept"
    );
    advance(cx, Duration::from_secs(61));
    show(cx, UsageOp::Show).await;
    assert_eq!(endpoint.hits(), 2);
}

#[gpui_kit::test]
async fn refresh_waits_thirty_seconds_between_requests(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    show(cx, UsageOp::Refresh).await;
    show(cx, UsageOp::Refresh).await;
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 1, "ten clicks are one request");
    advance(cx, Duration::from_secs(31));
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 2);
}

#[gpui_kit::test]
async fn a_busy_answer_backs_off_one_minute_then_five(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    endpoint.answer(429, "{}");
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 1);
    advance(cx, Duration::from_secs(59));
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 1, "no request inside the first minute");
    advance(cx, Duration::from_secs(2));
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 2);
    advance(cx, Duration::from_secs(240));
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 2, "the second back-off is five minutes");
    advance(cx, Duration::from_secs(61));
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 3);
}

#[gpui_kit::test]
async fn a_server_error_backs_off_too(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    endpoint.answer(503, "{}");
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    show(cx, UsageOp::Refresh).await;
    show(cx, UsageOp::Refresh).await;
    assert_eq!(endpoint.hits(), 1);
    advance(cx, Duration::from_secs(61));
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
    assert!(!serde_json::to_string(&view).unwrap().contains(&token));
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

    endpoint.answer(400, "boom");
    advance(cx, Duration::from_secs(31));
    let view = show(cx, UsageOp::Refresh).await;
    let account = codex(&view).unwrap();
    assert_eq!(account.status, UsageStatus::Stale);
    assert_eq!(account.windows.len(), 2, "the last good windows stay");
}

#[gpui_kit::test]
async fn a_failed_first_reading_is_an_error_not_zero(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    endpoint.answer(400, "boom");
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
async fn a_redirect_is_not_followed_so_the_token_stays_home(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let first = Endpoint::start();
    let second = Endpoint::start();
    first.redirect(&second.url);
    let home = fixture.dir.path().join("codex");
    let token = sign_in(&home, far_future());
    point_at(cx, Some(&home), &first.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert_eq!(error_text(&view), "Codex answered 302.");
    assert_eq!(first.hits(), 1);
    assert_eq!(second.hits(), 0, "the redirect target never sees the token");
    assert!(!serde_json::to_string(&view).unwrap().contains(&token));
}

#[gpui_kit::test]
async fn an_oversized_reply_is_cut_off_at_one_mebibyte(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let mut reply: Value = serde_json::from_str(&usage_reply()).unwrap();
    reply["padding"] = json!("x".repeat(1_100_000));
    endpoint.answer(200, &reply.to_string());
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert!(matches!(
        codex(&view).unwrap().status,
        UsageStatus::Error { .. }
    ));
}

#[gpui_kit::test]
async fn a_silent_endpoint_times_out_quickly(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = fixture.dir.path().join("codex");
    let token = sign_in(&home, far_future());
    point_at(cx, Some(&home), &silent());
    cx.update(|cx| crate::use_codex_timeout(cx, Duration::from_millis(300)));
    set_enabled(cx, true).await;

    let started = std::time::Instant::now();
    let view = show(cx, UsageOp::Refresh).await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the request was not cut off"
    );
    assert_eq!(error_text(&view), "Could not reach Codex.");
    assert!(!serde_json::to_string(&view).unwrap().contains(&token));
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
async fn an_unset_source_reads_no_real_sign_in(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    assert!(view.enabled);
    assert!(
        view.accounts.is_empty(),
        "a test without a source reads nothing"
    );
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
    assert_eq!(error_text(&view), "Usage is only sent to the provider.");
}

#[gpui_kit::test]
async fn turning_the_preference_off_drops_the_readings_at_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let endpoint = Endpoint::start();
    let home = fixture.dir.path().join("codex");
    sign_in(&home, far_future());
    point_at(cx, Some(&home), &endpoint.url);
    set_enabled(cx, true).await;
    show(cx, UsageOp::Refresh).await;
    assert_eq!(cached(cx).len(), 1);

    set_enabled(cx, false).await;
    assert!(
        cached(cx).is_empty(),
        "the saved setting clears the cache without a usage op"
    );
    set_enabled(cx, true).await;
    show(cx, UsageOp::Show).await;
    assert_eq!(
        endpoint.hits(),
        2,
        "a turned-off reading is read again, not kept"
    );
}

/// A made-up `claude` that records its arguments and stdin, then prints `reply` and exits.
#[cfg(unix)]
fn fake_claude(folder: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::create_dir_all(folder).unwrap();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{dir}/args.txt'\npwd > '{dir}/cwd.txt'\necho start >> '{dir}/starts.txt'\ncat > '{dir}/stdin.txt'\ncat <<'REPLY'\n{body}\nREPLY\n",
        dir = folder.display(),
    );
    let path = folder.join("claude");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    folder.to_path_buf()
}

#[cfg(unix)]
const CLAUDE_REPLY: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"initialize","response":{"account":{"email":"person@example.test","subscriptionType":"max"}}}}
{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":37,"resets_at":"2099-01-01T05:00:00+00:00"},"seven_day":{"utilization":12,"resets_at":"2099-01-05T00:00:00+00:00"},"model_scoped":[{"display_name":"Sonnet","utilization":9,"resets_at":"2099-01-05T00:00:00+00:00"}]}}}}"#;

#[cfg(unix)]
#[gpui_kit::test]
async fn claude_bars_come_from_the_installed_cli(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let folder = fake_claude(&fixture.dir.path().join("bin"), CLAUDE_REPLY);
    cx.update(|cx| crate::use_claude_path(cx, Some(folder.clone().into_os_string())));
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    let account = view
        .accounts
        .iter()
        .find(|account| account.provider == UsageProvider::Claude)
        .unwrap();
    assert_eq!(account.status, UsageStatus::Ok);
    assert_eq!(account.plan.as_deref(), Some("Max"));
    let labels: Vec<_> = account.windows.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(labels, ["5h", "7d", "7d · Sonnet"]);
    let stdin = std::fs::read_to_string(folder.join("stdin.txt")).unwrap();
    assert!(stdin.contains(r#""subtype":"get_usage""#));
    assert!(stdin.contains(r#""skip_behaviors":true"#));
    let args = std::fs::read_to_string(folder.join("args.txt")).unwrap();
    let args: Vec<&str> = args.lines().collect();
    let (flags, config) = args.split_at(CLAUDE_ARGS.len());
    assert_eq!(flags, CLAUDE_ARGS, "the exact flags, one per argument");
    assert_eq!(config.len(), 1, "then one path to the empty MCP config");
    let cwd = std::fs::read_to_string(folder.join("cwd.txt")).unwrap();
    let home = crate::host::program::user_home().unwrap();
    assert_eq!(
        std::fs::canonicalize(cwd.trim()).unwrap(),
        std::fs::canonicalize(home).unwrap(),
        "claude runs in the home folder, never in a project"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn no_claude_on_the_path_is_a_message_not_a_crash(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let empty = fixture.dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    cx.update(|cx| crate::use_claude_path(cx, Some(empty.into_os_string())));
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    let account = view
        .accounts
        .iter()
        .find(|account| account.provider == UsageProvider::Claude)
        .unwrap();
    assert_eq!(
        account.status,
        UsageStatus::Error {
            message: "Claude Code is not installed on this computer.".to_owned()
        }
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_slow_claude_is_stopped_and_says_so(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::new(cx).await;
    let folder = fixture.dir.path().join("slow");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("claude");
    std::fs::write(&path, "#!/bin/sh\ncat > /dev/null\nsleep 30\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    cx.update(|cx| {
        crate::use_claude_path(cx, Some(folder.into_os_string()));
        crate::use_claude_timeout(cx, Duration::from_secs(1));
    });
    set_enabled(cx, true).await;

    let started = std::time::Instant::now();
    let view = show(cx, UsageOp::Refresh).await;
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait is cut short"
    );
    let account = view
        .accounts
        .iter()
        .find(|account| account.provider == UsageProvider::Claude)
        .unwrap();
    assert_eq!(
        account.status,
        UsageStatus::Error {
            message: "Claude did not answer in time.".to_owned()
        }
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_reply_kagan_cannot_read_is_a_message(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let folder = fake_claude(&fixture.dir.path().join("bin"), "not json at all");
    cx.update(|cx| crate::use_claude_path(cx, Some(folder.into_os_string())));
    set_enabled(cx, true).await;

    let view = show(cx, UsageOp::Refresh).await;
    let account = view
        .accounts
        .iter()
        .find(|account| account.provider == UsageProvider::Claude)
        .unwrap();
    assert_eq!(
        account.status,
        UsageStatus::Error {
            message: "Claude sent a reply kagan could not read.".to_owned()
        }
    );
}

#[cfg(unix)]
const CLAUDE_ARGS: &[&str] = &[
    "--output-format",
    "stream-json",
    "--verbose",
    "--input-format",
    "stream-json",
    "--no-session-persistence",
    "--setting-sources=",
    "--strict-mcp-config",
    "--mcp-config",
];

#[cfg(unix)]
#[gpui_kit::test]
async fn two_quick_asks_start_one_claude(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let folder = fake_claude(&fixture.dir.path().join("bin"), CLAUDE_REPLY);
    cx.update(|cx| crate::use_claude_path(cx, Some(folder.clone().into_os_string())));
    set_enabled(cx, true).await;

    let first = cx.update(|cx| Workspace::dispatch(Op::Usage(UsageOp::Show), cx));
    let second = cx.update(|cx| Workspace::dispatch(Op::Usage(UsageOp::Show), cx));
    let (first, second) = futures::join!(first, second);
    first.unwrap();
    second.unwrap();
    let starts = std::fs::read_to_string(folder.join("starts.txt")).unwrap();
    assert_eq!(
        starts.lines().count(),
        1,
        "the second ask waits for the first"
    );
}
