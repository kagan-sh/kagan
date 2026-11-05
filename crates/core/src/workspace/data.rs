use std::path::{Path, PathBuf};

use kg_domain::OpError;
use kg_store::Store;

use crate::database::Result;
use crate::workspace::clock::closed;

fn ensure_personal(data_dir: &Path, store: &Store) -> Result<()> {
    let personal = data_dir.join("personal");
    std::fs::create_dir_all(&personal)
        .map_err(|error| OpError::Failed(format!("{}: {error}", personal.display())))?;
    if store
        .project(&kg_domain::ProjectId("personal".into()))
        .map_err(closed)?
        .is_none()
    {
        store
            .put_project(&kg_domain::Project {
                id: kg_domain::ProjectId("personal".into()),
                kind: kg_domain::ProjectKind::Personal,
                name: "Personal".into(),
                path: dunce::canonicalize(&personal)
                    .map_err(|error| OpError::Failed(format!("{}: {error}", personal.display())))?,
                git_remote_url: None,
                sort: 0,
            })
            .map_err(closed)?;
    }
    Ok(())
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
