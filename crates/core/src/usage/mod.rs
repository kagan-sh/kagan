//! Provider usage limits, read live and kept in memory only (decision 268). A token is read for
//! one request to its own provider and is never written, copied, cached or logged.

mod codex;

use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{OpResult, UsageAccount, UsageOp, UsageStatus, UsageView, prefs};

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

pub(crate) struct Cache {
    entry: Option<Entry>,
    source: codex::Source,
    skew: Duration,
}

impl Cache {
    pub(crate) fn new() -> Self {
        Self {
            entry: None,
            source: codex::Source::initial(),
            skew: Duration::ZERO,
        }
    }

    fn now(&self) -> Instant {
        Instant::now() + self.skew
    }

    fn plan(&mut self, op: UsageOp, now: Instant) -> Option<codex::Source> {
        if let Some(entry) = &mut self.entry {
            if now < entry.next_ask {
                return None;
            }
            if op == UsageOp::Show && now.duration_since(entry.read_at) < FRESH_FOR {
                return None;
            }
            entry.next_ask = now + FLOOR;
        }
        Some(self.source.clone())
    }

    fn apply(&mut self, answer: Option<codex::Answer>, started: Instant) {
        let Some(codex::Answer { account, busy }) = answer else {
            self.entry = None;
            return;
        };
        let previous = self.entry.take();
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
        self.entry = Some(Entry {
            account,
            read_at: started,
            next_ask: started + wait,
            busy_runs,
        });
    }

    pub(crate) fn accounts(&self) -> Vec<UsageAccount> {
        self.entry
            .iter()
            .map(|entry| entry.account.clone())
            .collect()
    }

    pub(crate) fn clear(&mut self) {
        self.entry = None;
    }

    #[cfg(any(test, feature = "test"))]
    fn use_source(&mut self, home: Option<std::path::PathBuf>, url: String) {
        self.source.home = home;
        self.source.url = url;
    }

    #[cfg(any(test, feature = "test"))]
    fn use_timeout(&mut self, timeout: Duration) {
        self.source.timeout = timeout;
    }

    #[cfg(any(test, feature = "test"))]
    fn advance(&mut self, by: Duration) {
        self.skew += by;
    }
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
        .unwrap_or(false)
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
    let planned = this
        .update(cx, |workspace, _| {
            let now = workspace.usage.now();
            workspace.usage.plan(op, now).map(|source| (source, now))
        })
        .map_err(closed)?;
    if let Some((source, started)) = planned {
        let answer = cx
            .background_spawn(async move { codex::read(&source, kg_domain::unix_now()) })
            .await;
        this.update(cx, |workspace, _| workspace.usage.apply(answer, started))
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
