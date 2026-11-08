use std::path::Path;

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{EnvironmentStatus, OpError, Project, ProjectBranch, ProjectId, ProjectKind};

use super::Prepared;
use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::host::git::{branch_moves, place_project_branch};
use crate::workspace::closed;

pub(super) async fn project_on(
    this: &WeakEntity<Workspace>,
    project: &Project,
    requested: &ProjectBranch,
    new_name: Option<String>,
    cx: &mut AsyncApp,
) -> Result<Prepared> {
    if project.kind != ProjectKind::Standard {
        return Err(OpError::Invalid(
            "Changing branches needs a standard project.".into(),
        ));
    }
    let path = super::canonical(project.path.clone(), cx).await?;
    let moves = {
        let path = path.clone();
        let requested = requested.clone();
        cx.background_executor()
            .spawn(async move { branch_moves(&path, &requested) })
            .await
    };
    if moves {
        let database = this
            .update(cx, |workspace, _| workspace.database.clone())
            .map_err(closed)?;
        let occupied = checkout_in_use(&database, &project.id, &path).await?;
        let path = path.clone();
        let requested = requested.clone();
        cx.background_executor()
            .spawn(async move {
                place_project_branch(&path, &requested, new_name.as_deref(), occupied)
            })
            .await?;
    }
    Ok(Prepared::Checkout(path))
}

async fn checkout_in_use(database: &Database, project: &ProjectId, path: &Path) -> Result<bool> {
    let project = project.clone();
    let path = path.to_path_buf();
    database
        .call(move |store| {
            let threads = store.threads(Some(&project), Some(false), None, None)?;
            let environments = store.environments(&project)?;
            Ok(threads.iter().any(|thread| {
                thread.environment_id.as_ref().is_some_and(|id| {
                    environments.iter().any(|environment| {
                        &environment.id == id
                            && environment.status != EnvironmentStatus::Destroyed
                            && same_folder(&environment.path, &path)
                    })
                })
            }))
        })
        .await
}

fn same_folder(left: &Path, right: &Path) -> bool {
    match (dunce::canonicalize(left), dunce::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
