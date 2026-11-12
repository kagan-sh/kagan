//! Claude's usage, asked of the installed Claude Code CLI the way the Claude Agent SDK asks it:
//! the CLI starts in stream-json mode, and two control requests go in on stdin, `initialize` and
//! then `get_usage`. kagan never reads Claude's sign-in or token. `get_usage` is experimental in
//! Claude Code, so a reply kagan cannot read shows as an error, never as 0%.

use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use chrono::DateTime;
use kg_domain::{UsageAccount, UsageProvider, UsageStatus, UsageWindow};
use serde_json::{Value, json};

use super::{Answer, as_text, error, plain};

const PROGRAM: &str = "claude";
const ARGS: &[&str] = &[
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
const INITIALIZE: &str = "initialize";
const USAGE: &str = "usage";
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub(super) enum Search {
    /// No search at all: the source is not wired up, as in a test.
    Off,
    /// Find `claude` on the person's PATH, resolved when a read starts.
    Login,
    /// Find `claude` on this PATH. `None` means no PATH, so nothing is found.
    #[cfg(any(test, feature = "test"))]
    Path(Option<std::ffi::OsString>),
}

#[derive(Clone)]
pub(super) struct Source {
    pub(super) search: Search,
    pub(super) timeout: Duration,
}

impl Source {
    pub(super) fn live() -> Self {
        Self {
            search: Search::Login,
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
            search: Search::Off,
            timeout: TIMEOUT,
        }
    }
}

pub(super) fn read(source: &Source, now: i64) -> Option<Answer> {
    let path = match &source.search {
        Search::Off => return None,
        Search::Login => kg_process::user_path(),
        #[cfg(any(test, feature = "test"))]
        Search::Path(path) => path.clone(),
    };
    let Some(program) = kg_process::find_program(
        Path::new(PROGRAM),
        path.as_deref(),
        kg_process::pathext().as_deref(),
    ) else {
        return Some(plain(
            UsageProvider::Claude,
            error("Claude Code is not installed on this computer."),
            now,
        ));
    };
    let Ok(mcp_config) = empty_mcp_config() else {
        return Some(plain(
            UsageProvider::Claude,
            error("Could not start Claude Code."),
            now,
        ));
    };
    let mut command = Command::new(program);
    command
        .args(ARGS)
        .arg(mcp_config.as_os_str())
        .current_dir(crate::host::program::user_home().unwrap_or_else(std::env::temp_dir));
    let input = [
        json!({"type": "control_request", "request_id": INITIALIZE, "request": {"subtype": "initialize"}}),
        json!({"type": "control_request", "request_id": USAGE, "request": {"subtype": "get_usage", "skip_behaviors": true}}),
    ]
    .map(|line| format!("{line}\n"))
    .concat();
    Some(
        match kg_process::capture_with_input(command, input.as_bytes(), source.timeout, || false) {
            Ok(output) => answer(&output.stdout, now),
            Err(failed) if failed.kind() == io::ErrorKind::TimedOut => plain(
                UsageProvider::Claude,
                error("Claude did not answer in time."),
                now,
            ),
            Err(_) => plain(
                UsageProvider::Claude,
                error("Could not start Claude Code."),
                now,
            ),
        },
    )
}

/// An MCP config with no servers, in a file removed when the read ends. A path has no quotes or
/// spaces for a Windows `.cmd` shim to mangle, unlike inline JSON.
fn empty_mcp_config() -> io::Result<tempfile::TempPath> {
    use io::Write as _;

    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(b"{\"mcpServers\":{}}")?;
    Ok(file.into_temp_path())
}

/// The control replies in the CLI's output, one JSON object per line. Other lines are skipped.
fn replies(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line.get("type").and_then(Value::as_str) == Some("control_response"))
        .filter_map(|line| line.get("response").cloned())
        .collect()
}

enum Reply<'a> {
    Ok(&'a Value),
    Failed,
    Missing,
}

fn reply<'a>(replies: &'a [Value], id: &str) -> Reply<'a> {
    let Some(response) = replies
        .iter()
        .find(|response| response.get("request_id").and_then(Value::as_str) == Some(id))
    else {
        return Reply::Missing;
    };
    match response.get("subtype").and_then(Value::as_str) {
        Some("success") => response.get("response").map_or(Reply::Missing, Reply::Ok),
        Some(_) => Reply::Failed,
        None => Reply::Missing,
    }
}

fn answer(stdout: &[u8], now: i64) -> Answer {
    let replies = replies(stdout);
    let initialized = reply(&replies, INITIALIZE);
    let usage = reply(&replies, USAGE);
    let account = match initialized {
        Reply::Ok(initialized) => initialized.get("account"),
        Reply::Failed | Reply::Missing => None,
    };
    match usage {
        Reply::Ok(usage) if usage.get("rate_limits").is_some() => {
            let (status, windows) = parse(usage);
            let plan = usage
                .get("subscription_type")
                .and_then(as_text)
                .or_else(|| {
                    account
                        .and_then(|account| account.get("subscriptionType"))
                        .and_then(as_text)
                })
                .and_then(|raw| badge(&raw));
            let email = account
                .and_then(|account| account.get("email"))
                .and_then(as_text);
            Answer {
                account: UsageAccount {
                    provider: UsageProvider::Claude,
                    email,
                    plan,
                    windows,
                    status,
                    fetched_at: Some(now),
                },
                busy: false,
            }
        }
        Reply::Failed => plain(
            UsageProvider::Claude,
            error("Claude could not give its usage. Check that Claude Code is signed in."),
            now,
        ),
        Reply::Ok(_) | Reply::Missing => plain(
            UsageProvider::Claude,
            error("Claude sent a reply kagan could not read."),
            now,
        ),
    }
}

/// The windows from `rate_limits`: the five-hour and seven-day windows, then one weekly bar per
/// model. A `null` `rate_limits` means the plan has no limits, such as an API key.
fn parse(usage: &Value) -> (UsageStatus, Vec<UsageWindow>) {
    let Some(limits) = usage.get("rate_limits").filter(|limits| !limits.is_null()) else {
        return (UsageStatus::NoLimits, Vec::new());
    };
    let mut windows: Vec<UsageWindow> = [("five_hour", "5h"), ("seven_day", "7d")]
        .into_iter()
        .filter_map(|(key, label)| window(limits.get(key)?, label.to_owned()))
        .collect();
    windows.extend(
        limits
            .get("model_scoped")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| {
                let name = model.get("display_name").and_then(as_text)?;
                window(model, format!("7d · {name}"))
            }),
    );
    let status = if windows.is_empty() {
        UsageStatus::NoLimits
    } else {
        UsageStatus::Ok
    };
    (status, windows)
}

fn window(value: &Value, label: String) -> Option<UsageWindow> {
    let percent = value.get("utilization")?.as_f64()?;
    Some(UsageWindow {
        label,
        percent: percent.clamp(0.0, 100.0),
        resets_at: value.get("resets_at").and_then(as_time),
    })
}

fn as_time(value: &Value) -> Option<i64> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|time| time.timestamp())
}

/// `max` and `claude_max` both read as `Max`.
fn badge(raw: &str) -> Option<String> {
    let name = raw.strip_prefix("claude_").unwrap_or(raw);
    let mut chars = name.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
#[path = "claude_test.rs"]
mod claude_test;
