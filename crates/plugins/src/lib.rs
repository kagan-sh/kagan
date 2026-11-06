//! The plugin host: discovery, activation, contributions, and the panels and pages plugins add.

mod authoring;
mod call;
mod install;
mod runtime;
mod schedule;

pub(crate) use authoring::{catalog, contributions, development, manifest, share};
pub(crate) use install::{index, source};
pub use runtime::keys;
pub(crate) use runtime::log;
pub(crate) use runtime::log::{Level, log};
pub use runtime::strip::{PluginStrip, StatusItem, StatusTone};
pub(crate) use runtime::{
    area, browser, clipboard, commands, database, files, host, jobs, ops, page, process, secret,
    strip, threads, ui_host, watchdog, web_store,
};

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::App;
use gpui_shell::HttpRequestGrant;
use kg_domain::{OpError, PluginId, ThemeFile};
use serde_json::Value;

use crate::manifest::{Fetch, Grant};
pub use catalog::{Example, capabilities, capabilities_in, examples, kinds_in};
pub use contributions::{
    Capability, Contributions, Follows, PanelContribution, Placement, StatusContribution,
};
pub(crate) use host::{Agents, Discovered, PluginHostGlobal, discover};
pub use host::{MountError, PluginHost, PluginHostEvent, init};
pub use index::shown;
pub use manifest::{AgentChoice, Setting};
pub use page::{MESSAGE_CAP, PageFileError, SCHEME, parse_message, read_plugin_file};
pub use ui_host::{HostContext, HostUi, HostUiEvent, ReviewComment, ToastAction};
pub use watchdog::PluginStatus;
#[cfg(target_os = "macos")]
pub use web_store::macos_major;
pub use web_store::{
    StoreRemoval, clear_removed_stores, clear_stores, keep_store, read_store_ids, store_dir,
};

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

const TOOL_TIMEOUT: Duration = Duration::from_mins(2);
const INSTRUCT_TIMEOUT: Duration = Duration::from_secs(2);
const INSTRUCT_CAP: usize = 4096;
const INSTRUCT_TOTAL: usize = 16 * 1024;
/// What every reader but the plugin sees for a secret setting that holds a value.
const SET: &str = "set";

/// Enabled contributions, discovered without executing plugin code.
///
/// # Errors
/// When the plugin host has not been initialized.
pub fn contributions(cx: &App) -> Result<Vec<(PluginId, Contributions)>> {
    let host = cx
        .try_global::<PluginHostGlobal>()
        .ok_or_else(|| OpError::Failed("the plugin host is unavailable".into()))?;
    Ok(host
        .0
        .read(cx)
        .active
        .iter()
        .map(|(id, active)| (id.clone(), active.manifest.ui.clone()))
        .collect())
}

/// One plain line per power in an approved grant, as the install screen words them.
///
/// # Errors
/// When `approved` is not a grant.
pub fn power_lines(approved: &Value) -> Result<Vec<String>> {
    Ok(serde_json::from_value::<Grant>(approved.clone())?.lines())
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
