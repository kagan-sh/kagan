//! Provider usage limits, read live and kept in memory only (decision 268). A token is read for
//! one request to its own provider and is never written, copied, cached or logged.

mod codex;
mod http;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{OpResult, UsageAccount, UsageOp, UsageProvider, UsageStatus, UsageView, prefs};

use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::workspace::closed;

const FRESH_FOR: Duration = Duration::from_secs(120);
const FLOOR: Duration = Duration::from_secs(30);
const BACKOFF: [Duration; 2] = [Duration::from_secs(60), Duration::from_secs(300)];

struct Entry {
    account: UsageAccount,
    read_at: Instant,
    next_ask: Instant,
    busy_runs: usize,
}

/// One provider's reading. `busy` is set for a 429 or a 5xx, which backs the next ask off.
pub(super) struct Answer {
    pub(super) account: UsageAccount,
    pub(super) busy: bool,
}

#[derive(Clone)]
pub(super) struct Sources {
    codex: codex::Source,
}

pub(crate) struct Cache {
    entries: BTreeMap<UsageProvider, Entry>,
    sources: Sources,
    skew: Duration,
}

impl Cache {
    pub(crate) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            sources: Sources {
                codex: codex::Source::initial(),
            },
            skew: Duration::ZERO,
        }
    }

    fn now(&self) -> Instant {
        Instant::now() + self.skew
    }

    /// The providers to ask now. A provider that is asked gets its floor at once, so a click
    /// that lands before its answer comes back does not ask again.
    fn plan(&mut self, op: UsageOp, now: Instant) -> Vec<UsageProvider> {
        let mut asks = Vec::new();
        for provider in UsageProvider::ALL {
            let Some(entry) = self.entries.get_mut(&provider) else {
                asks.push(provider);
                continue;
            };
            if now < entry.next_ask
                || (op == UsageOp::Show && now.duration_since(entry.read_at) < FRESH_FOR)
            {
                continue;
            }
            entry.next_ask = now + FLOOR;
            asks.push(provider);
        }
        asks
    }

    fn apply(&mut self, provider: UsageProvider, answer: Option<Answer>, started: Instant) {
        let Some(Answer { account, busy }) = answer else {
            self.entries.remove(&provider);
            return;
        };
        let previous = self.entries.remove(&provider);
        let account = match (&account.status, previous.as_ref()) {
            (UsageStatus::Error { .. }, Some(last))
                if matches!(last.account.status, UsageStatus::Ok | UsageStatus::Stale) =>
            {
                UsageAccount {
                    status: UsageStatus::Stale,
                    ..last.account.clone()
                }
            }
            _ => account,
        };
        let busy_runs = if busy {
            previous.as_ref().map_or(0, |last| last.busy_runs) + 1
        } else {
            0
        };
        let wait = if busy {
            BACKOFF[(busy_runs - 1).min(BACKOFF.len() - 1)]
        } else {
            FLOOR
        };
        self.entries.insert(
            provider,
            Entry {
                account,
                read_at: started,
                next_ask: started + wait,
                busy_runs,
            },
        );
    }

    pub(crate) fn accounts(&self) -> Vec<UsageAccount> {
        self.entries
            .values()
            .map(|entry| entry.account.clone())
            .collect()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(any(test, feature = "test"))]
    fn use_source(&mut self, home: Option<std::path::PathBuf>, url: String) {
        self.sources.codex.home = home;
        self.sources.codex.url = url;
    }

    #[cfg(any(test, feature = "test"))]
    fn use_timeout(&mut self, timeout: Duration) {
        self.sources.codex.timeout = timeout;
    }

    #[cfg(any(test, feature = "test"))]
    fn advance(&mut self, by: Duration) {
        self.skew += by;
    }
}

fn read(provider: UsageProvider, sources: &Sources, now: i64) -> Option<Answer> {
    match provider {
        UsageProvider::Codex => codex::read(&sources.codex, now),
    }
}

pub(super) fn plain(provider: UsageProvider, status: UsageStatus, now: i64) -> Answer {
    Answer {
        account: UsageAccount {
            provider,
            email: None,
            plan: None,
            windows: Vec::new(),
            status,
            fetched_at: Some(now),
        },
        busy: false,
    }
}

pub(super) fn error(message: &str) -> UsageStatus {
    UsageStatus::Error {
        message: message.to_owned(),
    }
}

pub(super) fn as_text(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[cfg(any(test, feature = "test"))]
pub fn use_codex_source(
    cx: &mut gpui_kit::App,
    codex_home: Option<std::path::PathBuf>,
    codex_url: String,
) {
    let workspace = Workspace::global(cx);
    workspace.update(cx, |workspace, _| {
        workspace.usage.use_source(codex_home, codex_url);
    });
}

#[cfg(any(test, feature = "test"))]
pub fn use_codex_timeout(cx: &mut gpui_kit::App, timeout: Duration) {
    let workspace = Workspace::global(cx);
    workspace.update(cx, |workspace, _| workspace.usage.use_timeout(timeout));
}

#[cfg(any(test, feature = "test"))]
pub fn advance_usage_clock(cx: &mut gpui_kit::App, by: Duration) {
    let workspace = Workspace::global(cx);
    workspace.update(cx, |workspace, _| workspace.usage.advance(by));
}

async fn enabled(database: &Database) -> bool {
    database
        .call(|store| store.setting(prefs::USAGE_LIMITS))
        .await
        .ok()
        .flatten()
        .and_then(|value| value.as_bool())
        .unwrap_or(prefs::USAGE_LIMITS_DEFAULT)
}

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    database: &Database,
    op: UsageOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if !enabled(database).await {
        this.update(cx, |workspace, _| workspace.usage.clear())
            .map_err(closed)?;
        return Ok(OpResult::Usage(UsageView {
            enabled: false,
            accounts: Vec::new(),
        }));
    }
    let (asks, sources, started) = this
        .update(cx, |workspace, _| {
            let now = workspace.usage.now();
            (
                workspace.usage.plan(op, now),
                workspace.usage.sources.clone(),
                now,
            )
        })
        .map_err(closed)?;
    let now = kg_domain::unix_now();
    let reads: Vec<_> = asks
        .into_iter()
        .map(|provider| {
            let sources = sources.clone();
            let task = cx.background_spawn(async move { read(provider, &sources, now) });
            (provider, task)
        })
        .collect();
    for (provider, task) in reads {
        let answer = task.await;
        this.update(cx, |workspace, _| {
            workspace.usage.apply(provider, answer, started);
        })
        .map_err(closed)?;
    }
    let accounts = this
        .update(cx, |workspace, _| workspace.usage.accounts())
        .map_err(closed)?;
    Ok(OpResult::Usage(UsageView {
        enabled: true,
        accounts,
    }))
}
