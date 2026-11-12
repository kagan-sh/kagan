//! Provider usage limits, read live and kept in memory only (decision 249). A token is read for one
//! request to its own provider and is never written, copied, cached or logged.

mod codex;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{OpResult, UsageAccount, UsageOp, UsageProvider, UsageStatus, UsageView, prefs};

use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::workspace::closed;

const FRESH_FOR: Duration = Duration::from_secs(120);

trait LockSafe<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockSafe<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Clone)]
struct Sources {
    codex_home: Option<PathBuf>,
    codex_url: String,
}

impl Sources {
    fn live() -> Self {
        Self {
            codex_home: codex::home(),
            codex_url: codex::URL.to_owned(),
        }
    }
}

struct Entry {
    account: UsageAccount,
    read_at: Instant,
}

pub(crate) struct Cache {
    entries: Mutex<BTreeMap<UsageProvider, Entry>>,
    sources: Mutex<Sources>,
}

impl Cache {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(BTreeMap::new()),
            sources: Mutex::new(Sources::live()),
        })
    }

    #[cfg(any(test, feature = "test"))]
    pub(crate) fn use_sources(&self, codex_home: Option<PathBuf>, codex_url: String) {
        *self.sources.lock_safe() = Sources {
            codex_home,
            codex_url,
        };
    }

    fn sources(&self) -> Sources {
        self.sources.lock_safe().clone()
    }

    fn clear(&self) {
        self.entries.lock_safe().clear();
    }

    fn due(&self, provider: UsageProvider, now: Instant) -> bool {
        let entries = self.entries.lock_safe();
        entries
            .get(&provider)
            .is_none_or(|entry| now.duration_since(entry.read_at) >= FRESH_FOR)
    }

    /// Stores one provider's new answer. A failed answer keeps the last good windows as stale.
    fn apply(&self, provider: UsageProvider, fresh: Option<UsageAccount>, now: Instant) {
        let mut entries = self.entries.lock_safe();
        let Some(account) = fresh else {
            entries.remove(&provider);
            return;
        };
        let account = match (&account.status, entries.get(&provider)) {
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
        entries.insert(
            provider,
            Entry {
                account,
                read_at: now,
            },
        );
    }

    fn accounts(&self) -> Vec<UsageAccount> {
        let entries = self.entries.lock_safe();
        entries
            .values()
            .map(|entry| entry.account.clone())
            .collect()
    }
}

/// Points the Codex reader at a test's own sign-in folder and endpoint, never the person's.
#[cfg(any(test, feature = "test"))]
pub fn use_codex_source(cx: &App, codex_home: Option<PathBuf>, codex_url: String) {
    Workspace::global(cx)
        .read(cx)
        .usage
        .use_sources(codex_home, codex_url);
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
    let cache = this
        .update(cx, |workspace, _| workspace.usage.clone())
        .map_err(closed)?;
    if !enabled(database).await {
        cache.clear();
        return Ok(OpResult::Usage(UsageView {
            enabled: false,
            accounts: Vec::new(),
        }));
    }
    let now = Instant::now();
    if op == UsageOp::Refresh || cache.due(UsageProvider::Codex, now) {
        let sources = cache.sources();
        let fresh = cx
            .background_spawn(async move {
                codex::read(
                    sources.codex_home.as_deref(),
                    &sources.codex_url,
                    codex::unix_now(),
                )
            })
            .await;
        cache.apply(UsageProvider::Codex, fresh, now);
    }
    Ok(OpResult::Usage(UsageView {
        enabled: true,
        accounts: cache.accounts(),
    }))
}
