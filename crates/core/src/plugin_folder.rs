use std::path::{Path, PathBuf};

use kg_domain::PluginId;

#[must_use]
pub fn plugins_dir(data: &Path) -> PathBuf {
    data.join("plugins")
}

#[must_use]
pub fn previews_dir(data: &Path) -> PathBuf {
    data.join("previews")
}

#[must_use]
pub fn plugin_dir(data: &Path, id: &PluginId) -> PathBuf {
    plugins_dir(data).join(&id.0)
}

#[must_use]
pub fn plugin_data_dir(data: &Path, id: &PluginId) -> PathBuf {
    plugin_dir(data, id).join("data")
}

#[must_use]
pub fn plugin_log_file(data: &Path, id: &PluginId) -> PathBuf {
    plugin_dir(data, id).join("logs.jsonl")
}
