//! Codex's usage: the sign-in file in `CODEX_HOME` (default `~/.codex`) and the `ChatGPT` usage
//! endpoint. The token goes only to that endpoint, and it is never refreshed.

use std::path::PathBuf;
use std::time::Duration;

use base64::Engine as _;
use kg_domain::{UsageAccount, UsageProvider, UsageStatus, UsageWindow};
use serde_json::Value;

use super::http::{self, Refused};
use super::{Answer, as_text, error, plain};

pub(super) const HOST: &str = "chatgpt.com";
pub(super) const URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const TIMEOUT: Duration = Duration::from_secs(15);
const AUTH_CLAIM: &str = "https://api.openai.com/auth";
const PROFILE_CLAIM: &str = "https://api.openai.com/profile";

#[cfg(debug_assertions)]
const DEV_URL: Option<&str> = option_env!("KG_USAGE_CODEX_URL");

#[derive(Clone)]
pub(super) struct Source {
    pub(super) home: Option<PathBuf>,
    pub(super) url: String,
    pub(super) timeout: Duration,
}

impl Source {
    pub(super) fn live() -> Self {
        #[cfg(debug_assertions)]
        let url = DEV_URL.unwrap_or(URL).to_owned();
        #[cfg(not(debug_assertions))]
        let url = URL.to_owned();
        Self {
            home: home(),
            url,
            timeout: TIMEOUT,
        }
    }

    pub(super) fn initial() -> Self {
        if cfg!(any(test, feature = "test")) {
            Self::dead()
        } else {
            Self::live()
        }
    }

    pub(super) fn dead() -> Self {
        Self {
            home: None,
            url: "https://127.0.0.1:1/wham/usage".to_owned(),
            timeout: TIMEOUT,
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::host::program::user_home().map(|home| home.join(".codex")))
}

pub(super) fn read(source: &Source, now: i64) -> Option<Answer> {
    let path = source.home.as_deref()?.join("auth.json");
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return Some(plain(
                UsageProvider::Codex,
                error("Could not read the Codex sign-in."),
                now,
            ));
        }
    };
    let Ok(file) = serde_json::from_str::<Value>(&text) else {
        return Some(plain(
            UsageProvider::Codex,
            error("The Codex sign-in file is not valid."),
            now,
        ));
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
        return Some(plain(UsageProvider::Codex, status, now));
    };
    let login = Login::new(tokens, token);
    if login.expires_at.is_some_and(|expires| now >= expires) {
        return Some(login.answer(Vec::new(), UsageStatus::Expired, now, false));
    }
    Some(match fetch(&login, source) {
        Ok(body) => {
            let (status, windows) = parse(&body);
            login.answer(windows, status, now, false)
        }
        Err(Refused::Status(status)) => login.answer(Vec::new(), status, now, false),
        Err(Refused::Busy(status)) => login.answer(Vec::new(), status, now, true),
    })
}

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

    fn answer(
        &self,
        windows: Vec<UsageWindow>,
        status: UsageStatus,
        now: i64,
        busy: bool,
    ) -> Answer {
        Answer {
            account: UsageAccount {
                provider: UsageProvider::Codex,
                email: self.email.clone(),
                plan: self.plan.clone(),
                windows,
                status,
                fetched_at: Some(now),
            },
            busy,
        }
    }
}

fn claims(jwt: &str) -> Option<Value> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn fetch(login: &Login, source: &Source) -> Result<Vec<u8>, Refused> {
    let headers: Vec<(&str, &str)> = login
        .account_id
        .as_deref()
        .map(|id| ("chatgpt-account-id", id))
        .into_iter()
        .collect();
    http::get(
        "Codex",
        &source.url,
        HOST,
        &login.token,
        &headers,
        source.timeout,
    )
}

fn parse(body: &[u8]) -> (UsageStatus, Vec<UsageWindow>) {
    let Ok(reply) = serde_json::from_slice::<Value>(body) else {
        return (
            error("Codex sent a reply kagan could not read."),
            Vec::new(),
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
    (status, windows)
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

#[cfg(test)]
#[path = "codex_test.rs"]
mod codex_test;
