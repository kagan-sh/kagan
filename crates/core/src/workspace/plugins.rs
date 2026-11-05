use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use kg_domain::{Plugin, PluginId};
use kg_store::Store;
use serde_json::Value;

use crate::host::database::Result;

use super::Workspace;

/// The activation a plugin setting write belongs to, and the gate that can revoke it.
pub struct Permit {
    activation: u64,
    gate: Arc<Mutex<Option<u64>>>,
}

impl Permit {
    #[must_use]
    pub fn new(activation: u64, gate: Arc<Mutex<Option<u64>>>) -> Self {
        Self { activation, gate }
    }
}
fn write_plugin_config(
    store: &mut Store,
    id: &PluginId,
    key: &str,
    value: &Value,
    folder: Option<&str>,
    permit: &Permit,
) -> kg_store::Result<()> {
    let guard = permit
        .gate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *guard != Some(permit.activation) {
        return Err(kg_store::Error::Conflict(format!(
            "plugin {id} changed while this was saving"
        )));
    }
    if let Some(path) = folder {
        let Some(mut row) = store.plugins()?.into_iter().find(|plugin| plugin.id == *id) else {
            return Err(kg_store::Error::NotFound {
                what: "plugin",
                id: id.to_string(),
            });
        };
        if !patch_folder(&mut row, key, path) {
            return Err(kg_store::Error::Conflict(format!(
                "plugin {id} setting {key} is no longer a folder"
            )));
        }
        store.set_plugin_approved(&row.id, &row.approved)?;
    }
    store.set_plugin_value(id, key, value)?;
    Ok(())
}

fn copy_kept_paths(approved: &mut Value, existing: &Value) {
    let folder_keys: Vec<String> = approved
        .get("folders")
        .and_then(Value::as_object)
        .map(|folders| folders.keys().cloned().collect())
        .unwrap_or_default();
    let existing_paths = existing
        .get("paths")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(approved) = approved.as_object_mut() else {
        return;
    };
    let mut paths = serde_json::Map::new();
    for key in folder_keys {
        if let Some(path) = existing_paths.get(&key) {
            paths.insert(key, path.clone());
        }
    }
    approved.insert("paths".to_owned(), Value::Object(paths));
}

fn patch_folder(plugin: &mut Plugin, key: &str, path: &str) -> bool {
    let Some(approved) = plugin.approved.as_object_mut() else {
        return false;
    };
    let still_folder = approved
        .get("folders")
        .and_then(Value::as_object)
        .is_some_and(|folders| folders.contains_key(key));
    if !still_folder {
        return false;
    }
    let paths = approved
        .entry("paths")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Some(paths) = paths.as_object_mut() else {
        return false;
    };
    paths.insert(key.to_owned(), Value::String(path.to_owned()));
    true
}
impl Workspace {
    fn call<R, F>(&self, call: F) -> impl Future<Output = Result<R>> + use<R, F>
    where
        R: Send + 'static,
        F: FnOnce(&mut Store) -> kg_store::Result<R> + Send + 'static,
    {
        let database = self.database.clone();
        async move { database.call(call).await }
    }
    /// # Errors
    /// When the database is closed or the query fails.
    pub fn plugins(&self) -> impl Future<Output = Result<Vec<Plugin>>> + use<> {
        self.call(|store| store.plugins())
    }
    /// # Errors
    /// When the database is closed, the id exists, or the query fails.
    pub fn insert_plugin(&self, plugin: Plugin) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.insert_plugin(&plugin))
    }
    /// Leaves every column other than `enabled` as stored.
    ///
    /// # Errors
    /// When the database is closed, the plugin is missing, or the query fails.
    pub fn set_plugin_enabled(
        &self,
        id: PluginId,
        enabled: bool,
    ) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.set_plugin_enabled(&id, enabled))
    }
    /// Keeps stored paths for folders the new grant still names, and leaves `enabled` unchanged.
    ///
    /// # Errors
    /// When the database is closed or the query fails.
    pub fn keep_plugin(
        &self,
        id: PluginId,
        root: PathBuf,
        source: String,
        commit: Option<String>,
        mut approved: Value,
    ) -> impl Future<Output = Result<Option<Plugin>>> + use<> {
        self.call(move |store| {
            let Some(existing) = store.plugins()?.into_iter().find(|plugin| plugin.id == id) else {
                return Ok(None);
            };
            copy_kept_paths(&mut approved, &existing.approved);
            store.keep_plugin_checkout(&id, &root, &source, &commit, &approved)?;
            Ok(Some(Plugin {
                id,
                root,
                source,
                commit,
                enabled: existing.enabled,
                approved,
                error: None,
            }))
        })
    }
    /// # Errors
    /// When the database is closed or removing the plugin fails.
    pub fn remove_plugin(&self, id: PluginId) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.delete_plugin(&id))
    }
    /// # Errors
    /// When the database is closed or the query fails.
    pub fn plugin_value(
        &self,
        id: PluginId,
        key: String,
    ) -> impl Future<Output = Result<Option<Value>>> + use<> {
        self.call(move |store| store.plugin_value(&id, &key))
    }
    /// # Errors
    /// When the database is closed or the query fails.
    pub fn set_plugin_value(
        &self,
        id: PluginId,
        key: String,
        value: Value,
    ) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.set_plugin_value(&id, &key, &value))
    }
    /// Writes the setting value only while `permit` still names the plugin's activation. A
    /// folder save also writes that path on `approved` when the key is still a folder, and
    /// does not change `root`, `source`, `commit`, or `enabled`. When that path cannot be
    /// written, it writes nothing.
    ///
    /// # Errors
    /// When the database is closed or the query fails, or the permit is stale, the plugin is
    /// gone, or the key is no longer a folder setting.
    pub fn commit_plugin_config(
        &self,
        id: PluginId,
        key: String,
        value: Value,
        folder: Option<String>,
        permit: Permit,
    ) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| {
            write_plugin_config(store, &id, &key, &value, folder.as_deref(), &permit)
        })
    }
}
