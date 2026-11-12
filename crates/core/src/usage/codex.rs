//! Codex's usage: the sign-in file in `CODEX_HOME` (default `~/.codex`) and the `ChatGPT` usage
//! endpoint. The token is sent to that endpoint only, and it is never refreshed.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use kg_domain::{UsageAccount, UsageProvider, UsageStatus, UsageWindow};
use serde_json::Value;

pub(super) const URL: &str = match option_env!("KG_USAGE_CODEX_URL") {
    Some(url) => url,
    None => "https://chatgpt.com/backend-api/wham/usage",
};
const TIMEOUT: Duration = Duration::from_secs(15);
const LIMIT: u64 = 1024 * 1024;
const AUTH_CLAIM: &str = "https://api.openai.com/auth";
const PROFILE_CLAIM: &str = "https://api.openai.com/profile";

pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::host::program::user_home().map(|home| home.join(".codex")))
}

pub(super) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

/// The Codex account, or `None` when no sign-in file exists, so there is no tab for Codex.
pub(super) fn read(home: Option<&Path>, url: &str, now: i64) -> Option<UsageAccount> {
    let path = home?.join("auth.json");
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return Some(bare(error("Could not read the Codex sign-in."), now)),
    };
    let Ok(file) = serde_json::from_str::<Value>(&text) else {
        return Some(bare(error("The Codex sign-in file is not valid."), now));
    };
    let tokens = file.get("tokens");
    let token = tokens
        .and_then(|tokens| tokens.get("access_token"))
        .and_then(as_text);
    let (Some(tokens), Some(token)) = (tokens, token) else {
        let status = if file.get("OPENAI_API_KEY").and_then(as_text).is_some() {
            UsageStatus::NoLimits
        } else {
            UsageStatus::Expired
        };
        return Some(bare(status, now));
    };
    let login = Login::new(tokens, token);
    if login.expires_at.is_some_and(|expires| now >= expires) {
        return Some(login.account(Vec::new(), UsageStatus::Expired, now));
    }
    Some(match fetch(&login, url) {
        Ok(body) => parse(&body, &login, now),
        Err(status) => login.account(Vec::new(), status, now),
    })
}

/// What the sign-in file and its token say about the account. The token itself is never stored
/// here in a form that prints, and this type has no `Debug`.
struct Login {
    token: String,
    account_id: Option<String>,
    email: Option<String>,
    plan: Option<String>,
    expires_at: Option<i64>,
}

impl Login {
    fn new(tokens: &Value, token: String) -> Self {
        let id = tokens
            .get("id_token")
            .and_then(as_text)
            .and_then(|jwt| claims(&jwt));
        let access = claims(&token);
        let claim = |key: &str| {
            [access.as_ref(), id.as_ref()]
                .into_iter()
                .flatten()
                .find_map(|claims| claims.get(AUTH_CLAIM)?.get(key).and_then(as_text))
        };
        let email = [id.as_ref(), access.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|claims| {
                claims
                    .get("email")
                    .or_else(|| claims.get(PROFILE_CLAIM)?.get("email"))
                    .and_then(as_text)
            });
        Self {
            account_id: tokens
                .get("account_id")
                .and_then(as_text)
                .or_else(|| claim("chatgpt_account_id")),
            plan: claim("chatgpt_plan_type"),
            email,
            expires_at: access
                .as_ref()
                .and_then(|claims| claims.get("exp"))
                .and_then(Value::as_i64),
            token,
        }
    }

    fn account(&self, windows: Vec<UsageWindow>, status: UsageStatus, now: i64) -> UsageAccount {
        UsageAccount {
            provider: UsageProvider::Codex,
            email: self.email.clone(),
            plan: self.plan.clone(),
            windows,
            status,
            fetched_at: Some(now),
        }
    }
}

fn bare(status: UsageStatus, now: i64) -> UsageAccount {
    UsageAccount {
        provider: UsageProvider::Codex,
        email: None,
        plan: None,
        windows: Vec::new(),
        status,
        fetched_at: Some(now),
    }
}

fn error(message: &str) -> UsageStatus {
    UsageStatus::Error {
        message: message.to_owned(),
    }
}

fn as_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The payload of a JWT, read without checking its signature.
fn claims(jwt: &str) -> Option<Value> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn allowed(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            || (url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost")))
    })
}

fn fetch(login: &Login, url: &str) -> Result<Vec<u8>, UsageStatus> {
    if !allowed(url) {
        return Err(error("Usage only loads over HTTPS."));
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent(concat!("kagan/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| error("Could not start the usage request."))?;
    let mut request = client.get(url).bearer_auth(&login.token);
    if let Some(account) = &login.account_id {
        request = request.header("chatgpt-account-id", account);
    }
    let response = request
        .send()
        .map_err(|_| error("Could not reach Codex."))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageStatus::Expired);
    }
    if !status.is_success() {
        return Err(error(&format!("Codex answered {}.", status.as_u16())));
    }
    let mut body = Vec::new();
    response
        .take(LIMIT)
        .read_to_end(&mut body)
        .map_err(|_| error("Codex's reply stopped early."))?;
    Ok(body)
}

fn parse(body: &[u8], login: &Login, now: i64) -> UsageAccount {
    let Ok(reply) = serde_json::from_slice::<Value>(body) else {
        return login.account(
            Vec::new(),
            error("Codex sent a reply kagan could not read."),
            now,
        );
    };
    let windows: Vec<UsageWindow> = ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|key| {
            reply
                .pointer(&format!("/rate_limit/{key}"))
                .and_then(window)
        })
        .collect();
    let status = if windows.is_empty() {
        UsageStatus::NoLimits
    } else {
        UsageStatus::Ok
    };
    let mut account = login.account(windows, status, now);
    if let Some(plan) = reply.get("plan_type").and_then(as_text) {
        account.plan = Some(plan);
    }
    account
}

fn window(value: &Value) -> Option<UsageWindow> {
    let used = value.get("used_percent")?.as_f64()?;
    Some(UsageWindow {
        label: label(value.get("limit_window_seconds").and_then(Value::as_i64)),
        percent: used.clamp(0.0, 100.0),
        resets_at: value.get("reset_at").and_then(Value::as_i64),
    })
}

fn label(seconds: Option<i64>) -> String {
    match seconds {
        Some(18_000) => "5h".to_owned(),
        Some(86_400) => "1d".to_owned(),
        Some(604_800) => "7d".to_owned(),
        Some(seconds) if seconds > 0 && seconds % 3_600 == 0 => format!("{}h", seconds / 3_600),
        _ => "Limit".to_owned(),
    }
}
