use std::path::{Path, PathBuf};

use kg_domain::{OpError, Plugin, prefs};
use kg_store::Store;

use crate::host::database::Result;
use crate::workspace::clock::closed;

fn ensure_personal(data_dir: &Path, store: &Store) -> Result<()> {
    let personal = data_dir.join("personal");
    std::fs::create_dir_all(&personal)
        .map_err(|error| OpError::Failed(format!("{}: {error}", personal.display())))?;
    if store
        .project(&kg_domain::ProjectId::personal())
        .map_err(closed)?
        .is_none()
    {
        store
            .put_project(&kg_domain::Project {
                id: kg_domain::ProjectId::personal(),
                kind: kg_domain::ProjectKind::Personal,
                name: "Personal".into(),
                path: dunce::canonicalize(&personal)
                    .map_err(|error| OpError::Failed(format!("{}: {error}", personal.display())))?,
                git_remote_url: None,
                sort: 0,
                program: None,
            })
            .map_err(closed)?;
    }
    Ok(())
}

/// Makes a preview's data folder with one plugin already in it, approved, and the `preview`
/// preference that stops a reload from asking a question nobody can see.
///
/// # Errors
/// When the folder or its database cannot be opened, or the plugin cannot be saved.
pub fn seed_preview(data_dir: &Path, plugin: &Plugin) -> Result<()> {
    let (store, _) = open_data(data_dir)?;
    store.insert_plugin(plugin).map_err(closed)?;
    store
        .set_setting(prefs::PREVIEW, &serde_json::Value::Bool(true))
        .map_err(closed)
}

pub(crate) fn open_data(data_dir: &Path) -> Result<(Store, PathBuf)> {
    std::fs::create_dir_all(data_dir)
        .map_err(|error| OpError::Failed(format!("{}: {error}", data_dir.display())))?;
    let data_dir = dunce::canonicalize(data_dir)
        .map_err(|error| OpError::Failed(format!("{}: {error}", data_dir.display())))?;
    let database = data_dir.join("kagan.sqlite3");
    let store = Store::open(&database)
        .map_err(|error| OpError::Failed(format!("{}: {error}", database.display())))?;
    ensure_personal(&data_dir, &store)?;
    Ok((store, data_dir))
}

#[cfg(test)]
#[path = "data_test.rs"]
mod tests;
