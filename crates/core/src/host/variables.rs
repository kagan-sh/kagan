//! Environment variables: the names and values every program kagan starts gets, global or per
//! project. Values are secrets, so they live in one file the person alone can read, never in the
//! database and never in a reply. A reply names variables, and never says what they hold.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{EnvOp, OpError, OpResult, ProjectId};
use serde::{Deserialize, Serialize};

use crate::Workspace;
use crate::workspace::{closed, database, missing};

const FILE: &str = "environment.json";

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    global: BTreeMap<String, String>,
    #[serde(default)]
    projects: BTreeMap<String, BTreeMap<String, String>>,
}

impl Stored {
    fn scope(&mut self, project: Option<&ProjectId>) -> &mut BTreeMap<String, String> {
        match project {
            None => &mut self.global,
            Some(id) => self.projects.entry(id.0.clone()).or_default(),
        }
    }
}

/// The variables a program in `project` gets: the global ones, and that project's, which win.
pub(crate) fn for_project(
    data_dir: &Path,
    project: &ProjectId,
) -> Result<Vec<(OsString, OsString)>, OpError> {
    let stored = read(data_dir)?;
    let mut merged = stored.global;
    if let Some(own) = stored.projects.get(project.0.as_str()) {
        merged.extend(own.clone());
    }
    Ok(merged
        .into_iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect())
}

/// Drops a removed project's variables, so no value outlives its project.
pub(crate) fn forget_project(data_dir: &Path, project: &ProjectId) -> Result<(), OpError> {
    let mut stored = read(data_dir)?;
    if stored.projects.remove(project.0.as_str()).is_some() {
        write(data_dir, &stored)?;
    }
    Ok(())
}

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: EnvOp,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let data_dir = this
        .update(cx, |workspace, _| workspace.data_dir.clone())
        .map_err(closed)?;
    if let Some(id) = scope_of(&op) {
        let lookup = id.clone();
        let found = database(this, cx)?
            .call(move |store| store.project(&lookup))
            .await?;
        if found.is_none() {
            return Err(missing("project", id));
        }
    }
    cx.background_executor()
        .spawn(async move { apply(&data_dir, op) })
        .await
}

fn scope_of(op: &EnvOp) -> Option<&ProjectId> {
    match op {
        EnvOp::List { project_id }
        | EnvOp::Set { project_id, .. }
        | EnvOp::Remove { project_id, .. } => project_id.as_ref(),
    }
}

fn apply(data_dir: &Path, op: EnvOp) -> Result<OpResult, OpError> {
    let mut stored = read(data_dir)?;
    match op {
        EnvOp::List { project_id } => Ok(OpResult::Variables(
            stored.scope(project_id.as_ref()).keys().cloned().collect(),
        )),
        EnvOp::Set {
            name,
            project_id,
            value,
        } => {
            let value = value.ok_or_else(|| OpError::Invalid("the value is empty".into()))?;
            stored.scope(project_id.as_ref()).insert(name, value);
            write(data_dir, &stored)?;
            Ok(OpResult::Done)
        }
        EnvOp::Remove { name, project_id } => {
            let scope = stored.scope(project_id.as_ref());
            if scope.remove(&name).is_none() {
                return Err(OpError::NotFound(format!("no variable {name} is set")));
            }
            write(data_dir, &stored)?;
            Ok(OpResult::Done)
        }
    }
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

fn read(data_dir: &Path) -> Result<Stored, OpError> {
    match std::fs::read(path(data_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| OpError::Failed("the environment variables file cannot be read".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Stored::default()),
        Err(error) => Err(OpError::Failed(format!(
            "the environment variables file: {error}"
        ))),
    }
}

// A new temporary file is readable only by its owner, and it is renamed over the old file, so a
// crash leaves either the old values or the new ones.
fn write(data_dir: &Path, stored: &Stored) -> Result<(), OpError> {
    let failed =
        |error: std::io::Error| OpError::Failed(format!("the environment variables file: {error}"));
    let mut temporary = tempfile::NamedTempFile::new_in(data_dir).map_err(failed)?;
    let bytes = serde_json::to_vec(stored)
        .map_err(|_| OpError::Failed("the environment variables file cannot be written".into()))?;
    temporary.write_all(&bytes).map_err(failed)?;
    temporary.as_file().sync_all().map_err(failed)?;
    temporary
        .persist(path(data_dir))
        .map_err(|error| failed(error.error))?;
    Ok(())
}
