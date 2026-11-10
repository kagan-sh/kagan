//! Environment variables: the names and values every program kagan starts gets, global or per
//! project. Values are secrets, so they live in one file the person alone can read, never in the
//! database and never in a reply. A reply names variables, and never says what they hold.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{EnvOp, OpError, OpResult, ProjectId};
use serde::{Deserialize, Serialize};

use crate::Workspace;
use crate::workspace::{closed, database, missing};

const FILE: &str = "environment.json";
const KEPT: &str = "environment.json.unreadable";

// Every read-modify-write of the file holds this, so two writes cannot erase each other.
static WRITES: Mutex<()> = Mutex::new(());

fn writes() -> MutexGuard<'static, ()> {
    WRITES.lock().unwrap_or_else(PoisonError::into_inner)
}

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

/// `global` with `own` added on top; a name in `own` replaces the same name in `global`. With
/// `fold_case`, names match whatever their letter case, as Windows environment names do.
pub(crate) fn merge_scopes(
    global: BTreeMap<String, String>,
    own: &BTreeMap<String, String>,
    fold_case: bool,
) -> BTreeMap<String, String> {
    let key = |name: &str| {
        if fold_case {
            name.to_ascii_uppercase()
        } else {
            name.to_owned()
        }
    };
    let mut merged: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (name, value) in global {
        merged.insert(key(&name), (name, value));
    }
    for (name, value) in own {
        merged.insert(key(name), (name.clone(), value.clone()));
    }
    merged.into_values().collect()
}

/// The variables a program in `project` gets: the global ones, and that project's, which win.
pub(crate) fn for_project(
    data_dir: &Path,
    project: &ProjectId,
) -> Result<Vec<(OsString, OsString)>, OpError> {
    let stored = read(data_dir)?;
    let own = stored
        .projects
        .get(project.0.as_str())
        .cloned()
        .unwrap_or_default();
    let merged = merge_scopes(stored.global, &own, cfg!(windows));
    Ok(merged
        .into_iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect())
}

/// Drops a removed project's variables, so no value outlives its project.
pub(crate) fn forget_project(data_dir: &Path, project: &ProjectId) -> Result<(), OpError> {
    let _writes = writes();
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
    let _writes = writes();
    let mut notes = Vec::new();
    let mut stored = match read(data_dir) {
        Ok(stored) => stored,
        Err(error) if matches!(op, EnvOp::List { .. }) => return Err(error),
        // A change may not be blocked by a file it cannot read, so the file is kept aside.
        Err(error) => {
            notes.push(keep_aside(data_dir, &error)?);
            Stored::default()
        }
    };
    let done = match op {
        EnvOp::List { project_id } => {
            return Ok(OpResult::Variables(
                stored.scope(project_id.as_ref()).keys().cloned().collect(),
            ));
        }
        EnvOp::Set {
            name,
            project_id,
            value,
            ..
        } => {
            let value = value.ok_or_else(|| OpError::Invalid("the value is empty".into()))?;
            stored.scope(project_id.as_ref()).insert(name, value);
            write(data_dir, &stored)?;
            OpResult::Done
        }
        EnvOp::Remove {
            name, project_id, ..
        } => {
            let scope = stored.scope(project_id.as_ref());
            if scope.remove(&name).is_none() && notes.is_empty() {
                return Err(OpError::NotFound(format!("no variable {name} is set")));
            }
            if let Some(id) = project_id
                && stored
                    .projects
                    .get(id.0.as_str())
                    .is_some_and(BTreeMap::is_empty)
            {
                stored.projects.remove(id.0.as_str());
            }
            write(data_dir, &stored)?;
            OpResult::Done
        }
    };
    Ok(if notes.is_empty() {
        done
    } else {
        OpResult::Warnings(notes)
    })
}

// Moves a file that cannot be read to a name of its own. The rename keeps its owner-only mode,
// and no value is lost: the person can read or delete the kept file.
fn keep_aside(data_dir: &Path, error: &OpError) -> Result<String, OpError> {
    let kept = data_dir.join(KEPT);
    std::fs::rename(path(data_dir), &kept).map_err(|rename| {
        OpError::Failed(format!(
            "{} cannot be read ({error}) and could not be moved aside: {rename}",
            path(data_dir).display()
        ))
    })?;
    Ok(format!(
        "{} cannot be read, so it was kept as {}; its values are not used",
        path(data_dir).display(),
        kept.display()
    ))
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

fn read(data_dir: &Path) -> Result<Stored, OpError> {
    let unreadable = |detail: String| {
        OpError::Failed(format!(
            "{} cannot be read: {detail}. Set any variable to move it aside",
            path(data_dir).display()
        ))
    };
    match std::fs::read(path(data_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| unreadable(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Stored::default()),
        Err(error) => Err(unreadable(error.to_string())),
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

#[cfg(test)]
#[path = "variables_test.rs"]
mod tests;
