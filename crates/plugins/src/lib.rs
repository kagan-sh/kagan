//! The plugin host: discovery, activation, contributions, and the panels and pages plugins add.

mod authoring;
mod call;
mod install;
mod runtime;
mod schedule;

pub(crate) use authoring::{contributions, development, manifest, share};
pub(crate) use install::{index, source};
pub use runtime::keys;
pub(crate) use runtime::log;
pub(crate) use runtime::log::{Level, log};
pub use runtime::strip::{PluginStrip, StatusItem, StatusTone};
pub(crate) use runtime::{
    area, ask, browser, clipboard, commands, database, files, host, jobs, ops, page, process,
    secret, strip, threads, ui_host, watchdog, web_store,
};

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::App;
use gpui_shell::HttpRequestGrant;
use kg_domain::{OpError, PluginId, ThemeFile};
use serde_json::Value;

use crate::manifest::Fetch;
pub use contributions::{Contributions, Follows, PanelContribution, Placement, StatusContribution};
pub(crate) use host::{Discovered, PluginHostGlobal, discover};
pub use host::{MountError, PluginHost, init};
pub use index::shown;
pub use manifest::{AgentChoice, Setting};
pub use page::{MESSAGE_CAP, PageFileError, SCHEME, parse_message, read_plugin_file};
pub use ui_host::{HostContext, HostUi, HostUiEvent, ReviewComment, ToastAction};
pub use watchdog::PluginStatus;
pub use web_store::keep_store;
#[cfg(target_os = "macos")]
pub use web_store::macos_major;

type Result<T> = std::result::Result<T, OpError>;
type DropPages = Rc<dyn Fn(&PluginId, &[[u8; 16]], &mut App)>;

fn event_data(kind: &kg_domain::EventKind) -> Value {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.get("data").cloned())
        .unwrap_or(Value::Null)
}

fn http_grants(
    fetch: &[Fetch],
) -> std::result::Result<Vec<HttpRequestGrant>, manifest::FetchError> {
    fetch.iter().map(Fetch::http_grant).collect()
}

const INSTRUCT_CAP: usize = 4096;
const INSTRUCT_TOTAL: usize = 16 * 1024;
// What every reader but the plugin sees for a secret setting that holds a value.
const SET: &str = "set";

/// Enabled contributions, discovered without executing plugin code.
///
/// # Errors
/// When the plugin host has not been initialized.
pub fn contributions(cx: &App) -> Result<Vec<(PluginId, Contributions)>> {
    let host = PluginHost::try_global(cx)
        .ok_or_else(|| OpError::Failed("the plugin host is unavailable".into()))?;
    Ok(host
        .read(cx)
        .active
        .iter()
        .map(|(id, active)| (id.clone(), active.manifest.ui.clone()))
        .collect())
}

pub use commands::{CommandContribution, CommandWhen, KeySource, RunCommand};

/// A theme an enabled plugin ships.
#[derive(Clone, Copy, Debug)]
pub struct PluginTheme<'a> {
    /// `<plugin id>/<theme id>`
    pub id: &'a str,
    /// The plugin's name
    pub plugin: &'a str,
    pub file: &'a ThemeFile,
}

/// The kagan version this build is. A plugin's `kagan` requirement is checked against it.
pub const KAGAN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Whether `running` is at least `minimum`. `None` when either is not a semantic version.
#[must_use]
pub fn kagan_at_least(minimum: &str, running: &str) -> Option<bool> {
    let minimum = semver::Version::parse(minimum).ok()?;
    let running = semver::Version::parse(running).ok()?;
    Some(running >= minimum)
}

#[cfg(test)]
mod kagan_version_tests {
    use super::kagan_at_least;

    #[test]
    fn a_build_meets_a_minimum_it_reaches_and_not_one_above_it() {
        assert_eq!(kagan_at_least("0.1.3", "0.1.3"), Some(true));
        assert_eq!(kagan_at_least("0.1.3", "0.9.0"), Some(true));
        assert_eq!(kagan_at_least("0.1.3", "0.1.0"), Some(false));
        assert_eq!(kagan_at_least("0.1.3", "not a version"), None);
    }
}
