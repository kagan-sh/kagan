use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum UsageOp {
    /// The usage each signed-in provider reported. Asks again when the last answer is over two
    /// minutes old
    Show,
    /// Ask each signed-in provider for its usage now
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageProvider {
    Claude,
    Codex,
}

impl UsageProvider {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
        }
    }
}

/// Unix seconds now, or 0 when the clock is before 1970.
#[must_use]
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

/// How long until a reset, in the words the window and `kg usage` show: `3d 4h`, `2h 10m` or `5m`.
/// A reset in the past reads as `0m`.
#[must_use]
pub fn resets_in(resets_at: i64, now: i64) -> String {
    let seconds = (resets_at - now).max(0);
    let (days, hours, minutes) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
    );
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// One limit window, such as the 5h or 7d window. `percent` is 0 to 100.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub label: String,
    pub percent: f64,
    /// Unix seconds when the window resets, when the provider says
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum UsageStatus {
    Ok,
    /// The newest refresh failed. The windows are the last reading that worked.
    Stale,
    /// The sign-in has expired. Signing in again in the provider's own app fixes it.
    Expired,
    /// The sign-in reports no subscription limits, such as an API key sign-in
    NoLimits,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageAccount {
    pub provider: UsageProvider,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub status: UsageStatus,
    /// Unix seconds of the reading the windows show
    pub fetched_at: Option<i64>,
}

/// What the window and `kg usage` show. `enabled` is false when the `usage-limits` preference is
/// off, and then nothing was read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageView {
    pub enabled: bool,
    pub accounts: Vec<UsageAccount>,
}
