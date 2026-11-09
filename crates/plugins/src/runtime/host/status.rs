use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui_kit::WeakEntity;
use gpui_shell::ScriptView;
use kg_domain::{OpError, PluginId};
use serde_json::Value;

use crate::index;
use crate::manifest::{Grant, Setting};
use crate::{PluginStatus, PluginTheme, SET};

use super::PluginHost;

impl PluginHost {
    pub fn settings(&self, id: &PluginId) -> Vec<(String, Setting, Value)> {
        self.active.get(id).map_or_else(Vec::new, |active| {
            active
                .manifest
                .settings
                .iter()
                .map(|(key, schema)| {
                    let value = self.value(id, key).unwrap_or(Value::Null);
                    (key.clone(), schema.clone(), value)
                })
                .collect()
        })
    }

    /// Said beside a secret setting when this system has no keychain, so secrets go to a file.
    #[must_use]
    pub fn secret_warning(&self) -> Option<String> {
        self.secrets.warning()
    }

    /// A setting's value as everyone but the plugin sees it: a secret is `"set"` or `null`.
    pub(crate) fn value(&self, id: &PluginId, key: &str) -> Option<Value> {
        let active = self.active.get(id)?;
        if let Some(Setting::Secret { .. }) = active.manifest.settings.get(key) {
            return Some(self.secrets.is_set(id, key).then_some(SET).into());
        }
        active.value(key)
    }

    /// Why a plugin is not running: a fault, a crash, or a start that failed.
    pub(crate) fn failure(&self, id: &PluginId) -> Option<String> {
        self.active
            .get(id)
            .and_then(|active| {
                active.fault.clone().or_else(|| {
                    active
                        .loaded
                        .as_ref()
                        .and_then(|loaded| loaded.watchdog.error.clone())
                })
            })
            .or_else(|| self.warnings.get(id).cloned())
            .map(|failure| self.secrets.redact(id, &failure))
    }

    /// Index locations without passwords.
    ///
    /// # Errors
    /// When the saved list cannot be read; the text says why.
    pub fn indexes(&self) -> std::result::Result<Vec<String>, OpError> {
        self.indexes
            .as_ref()
            .map(|saved| saved.iter().map(|known| index::shown(known)).collect())
            .map_err(|error| OpError::Failed(error.to_string()))
    }

    #[must_use]
    pub fn allows_open_url(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.powers.open_url)
    }

    #[must_use]
    pub fn installing(&self) -> Option<String> {
        self.installing.borrow().clone()
    }

    /// Stops the install that runs now, so its git steps end. Returns false when none runs.
    #[must_use]
    pub(crate) fn stop_install(&self) -> bool {
        self.install_stop.borrow_mut().take().is_some()
    }

    /// Whether a git install is downloading now, so Cancel applies.
    #[must_use]
    pub fn stoppable(&self) -> bool {
        self.install_stop.borrow().is_some()
    }

    #[must_use]
    pub fn status(&self) -> Vec<PluginStatus> {
        let tool_notes = self.tool_notes();
        self.records
            .iter()
            .map(|(id, record)| {
                let active = self.active.get(id);
                let watchdog = active
                    .and_then(|active| active.loaded.as_ref())
                    .map(|loaded| &loaded.watchdog);
                let approved = crate::ops::approved(record).unwrap_or_else(|error| {
                    eprintln!("kg: {error}");
                    Grant::default()
                });
                PluginStatus {
                    id: id.clone(),
                    name: self
                        .identities
                        .get(id)
                        .map_or_else(|| id.0.clone(), |(name, _)| name.clone()),
                    version: self.identities.get(id).map(|(_, version)| version.clone()),
                    source: crate::index::shown(&record.source),
                    powers: approved.lines(),
                    enabled: record.enabled,
                    slow: watchdog.is_some_and(|watchdog| watchdog.slow),
                    error: self.failure(id),
                    note: tool_notes.get(id).cloned(),
                }
            })
            .collect()
    }

    /// Tool names are global to an agent, so a name two plugins declare reaches it once. The note
    /// names the later plugin and the one that owns the name.
    pub(crate) fn tool_notes(&self) -> BTreeMap<PluginId, String> {
        let mut owners: BTreeMap<&str, &PluginId> = BTreeMap::new();
        let mut notes = BTreeMap::new();
        for (id, active) in &self.active {
            for tool in &active.manifest.tools {
                match owners.get(tool.name.as_str()) {
                    Some(owner) if *owner != id => {
                        notes.insert(
                            id.clone(),
                            format!(
                                "tool {} is also declared by plugin {owner}; the agent sees one",
                                tool.name
                            ),
                        );
                    }
                    Some(_) => {}
                    None => {
                        owners.insert(&tool.name, id);
                    }
                }
            }
        }
        notes
    }

    /// The themes of enabled plugins.
    pub fn themes(&self) -> impl Iterator<Item = PluginTheme<'_>> {
        self.active.values().flat_map(|active| {
            active.themes.iter().map(|(id, file)| PluginTheme {
                id,
                plugin: &active.manifest.name,
                file,
            })
        })
    }

    /// Returns the installed plugin's icon, if it has a readable icon file.
    #[must_use]
    pub fn icon(&self, id: &PluginId) -> Option<&[u8]> {
        self.active
            .get(id)
            .map(|active| active.icon.as_slice())
            .filter(|icon| !icon.is_empty())
    }
}
impl PluginHost {
    #[must_use]
    pub fn view_fetch(
        &self,
        id: gpui_kit::EntityId,
    ) -> Option<(WeakEntity<ScriptView>, PluginId, Vec<String>)> {
        for (plugin, active) in &self.active {
            let Some(loaded) = &active.loaded else {
                continue;
            };
            let view = if loaded.view.entity_id() == id {
                Some(loaded.view.downgrade())
            } else {
                loaded
                    .views
                    .iter()
                    .find(|view| view.entity_id() == id)
                    .cloned()
            };
            if let Some(view) = view {
                let hosts = active
                    .grant
                    .powers
                    .fetch
                    .iter()
                    .filter_map(|fetch| fetch.view_host().ok())
                    .collect();
                return Some((view, plugin.clone(), hosts));
            }
        }
        None
    }
}
impl PluginHost {
    #[must_use]
    pub fn generation(&self, id: &PluginId) -> Option<u64> {
        self.active
            .get(id)
            .filter(|active| active.loaded.is_some())
            .map(|active| active.generation)
    }

    /// Whether every enabled plugin found at launch has had its start.
    #[must_use]
    pub fn started(&self) -> bool {
        self.started
    }

    #[cfg(test)]
    #[must_use]
    pub fn is_loaded(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.loaded.is_some())
    }
}
impl PluginHost {
    #[must_use]
    pub fn plugin_data(&self, id: &PluginId) -> PathBuf {
        kg_core::plugin_data_dir(&self.data_dir, id)
    }

    /// The plugin folder, for serving that plugin's own page.
    #[must_use]
    pub fn plugin_root(&self, id: &PluginId) -> Option<PathBuf> {
        self.active.get(id).map(|active| active.root.clone())
    }

    /// Hosts this plugin's `powers.fetch` names.
    #[must_use]
    pub fn fetch_hosts(&self, id: &PluginId) -> Vec<String> {
        self.active
            .get(id)
            .map(|active| {
                active
                    .grant
                    .powers
                    .fetch
                    .iter()
                    .map(|fetch| fetch.host.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}
