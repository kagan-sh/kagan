use kg_domain::{OpError, Project, ProjectId, ProjectKind};

use crate::database::Result;
use crate::workspace::{closed, id};

pub(crate) fn add_project(store: &kg_store::Store, path: &std::path::Path) -> Result<Project> {
    let path = dunce::canonicalize(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidFilename {
            OpError::Invalid("the project path is too long".into())
        } else {
            OpError::Invalid(format!("{}: {error}", path.display()))
        }
    })?;
    if !path.is_dir() {
        return Err(OpError::Invalid(format!(
            "{} is not a folder",
            path.display()
        )));
    }
    if let Some(project) = store
        .projects()
        .map_err(closed)?
        .into_iter()
        .find(|project| project.path == path)
    {
        return Err(OpError::Invalid(format!(
            "{} is already the project {}",
            path.display(),
            project.name
        )));
    }
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let sort = store
        .projects()
        .map_err(closed)?
        .iter()
        .filter(|project| project.kind == ProjectKind::Standard)
        .map(|project| project.sort)
        .max()
        .map_or(0, |sort| sort.saturating_add(1));
    let project = Project {
        id: ProjectId(id("prj")),
        kind: ProjectKind::Standard,
        name,
        // `get-url` applies url.*.insteadOf, so a machine's https token rewrite
        // would store an ssh origin as https. The repo config is the origin.
        git_remote_url: crate::git::git(
            &path,
            &["config", "--local", "--get", "remote.origin.url"],
        )
        .ok()
        .map(|url| without_credentials(url.trim())),
        path,
        sort,
    };
    if store.removal_pending(&project.path).map_err(closed)? {
        return Err(OpError::Invalid("that folder is being removed".into()));
    }
    store.put_project(&project).map_err(closed)?;
    Ok(project)
}

fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let ssh = scheme.eq_ignore_ascii_case("ssh");
    if !ssh && !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return url.to_owned();
    }
    let (authority, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_owned();
    };
    if ssh {
        let user = userinfo.split_once(':').map_or(userinfo, |(user, _)| user);
        return format!("{scheme}://{user}@{host}{path}");
    }
    format!("{scheme}://{host}{path}")
}

pub(crate) async fn remove(
    this: &gpui_kit::WeakEntity<crate::Workspace>,
    id: ProjectId,
    requester: Option<crate::Requester>,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<kg_domain::OpResult> {
    let database = crate::workspace::database(this, cx)?;
    let lookup = id.clone();
    let (project, threads) = database
        .call(move |store| {
            Ok((
                store.project(&lookup)?,
                store.threads(Some(&lookup), None, None, None)?,
            ))
        })
        .await?;
    let project = project.ok_or_else(|| crate::workspace::missing("project", &id))?;
    removable(&project, &threads)?;
    let preparing = this
        .update(cx, |workspace, _| {
            workspace.preparing_projects.contains_key(&id)
        })
        .map_err(closed)?;
    if preparing {
        return Err(OpError::Invalid(format!(
            "Stop the programs in “{}” first.",
            project.name
        )));
    }
    let subject = match threads.len() {
        0 => format!("Remove “{}”", project.name),
        1 => format!("Remove “{}” and its thread", project.name),
        count => format!("Remove “{}” and its {count} threads", project.name),
    };
    let question = format!("{subject} from kagan? The folder stays on disk.");
    crate::thread::archive::approve(None, requester, question, "Remove", cx).await?;
    let threads = threads.into_iter().map(|thread| thread.id).collect();
    let submitted = this
        .update(cx, |workspace, _| {
            workspace.submit(crate::workspace::Request::RemoveProject { id, threads })
        })
        .map_err(closed)?;
    submitted.await
}

pub(crate) async fn remove_project(
    this: &gpui_kit::WeakEntity<crate::Workspace>,
    id: ProjectId,
    mut expected: Vec<kg_domain::ThreadId>,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<kg_domain::OpResult> {
    let (database, removing) = this
        .update(cx, |workspace, _| {
            (workspace.database.clone(), workspace.removing.clone())
        })
        .map_err(closed)?;
    let lookup = id.clone();
    let (project, threads, environments) = database
        .call(move |store| {
            Ok((
                store.project(&lookup)?,
                store.threads(Some(&lookup), None, None, None)?,
                store.environments(&lookup)?,
            ))
        })
        .await?;
    let project = project.ok_or_else(|| crate::workspace::missing("project", &id))?;
    removable(&project, &threads)?;
    let mut found: Vec<_> = threads.iter().map(|thread| thread.id.clone()).collect();
    found.sort();
    expected.sort();
    if found != expected {
        return Err(OpError::Invalid(
            "the project's threads changed; remove it again".into(),
        ));
    }
    if environments
        .iter()
        .any(|environment| removing.contains(&environment.id))
    {
        return Err(OpError::Invalid(
            "a worktree is still being removed; try again".into(),
        ));
    }
    let worktrees: Vec<_> = environments
        .into_iter()
        .filter(|environment| environment.provider == kg_domain::EnvironmentProvider::Worktree)
        .map(|mut environment| {
            environment.status = kg_domain::EnvironmentStatus::Destroyed;
            environment.retire_at = None;
            environment
        })
        .collect();
    let recorded = worktrees.clone();
    database
        .call(move |store| {
            for environment in &recorded {
                store.put_environment(environment)?;
            }
            Ok(())
        })
        .await?;
    this.update(cx, |workspace, _| {
        workspace
            .removing
            .extend(worktrees.iter().map(|environment| environment.id.clone()));
    })
    .map_err(closed)?;
    let failures = remove_worktrees(this, &project.path, worktrees, cx).await?;
    let at = crate::workspace::time(this, cx)?;
    let lookup = id.clone();
    let events = database
        .call(move |store| store.remove_project(&lookup, at))
        .await?;
    this.update(cx, |workspace, cx| {
        crate::thread::archive::forget_deleted(workspace, events, cx);
    })
    .map_err(closed)?;
    crate::Workspace::refresh(this, cx).await?;
    crate::thread::agent::forget(this, found, cx).await;
    if failures.is_empty() {
        Ok(kg_domain::OpResult::Done)
    } else {
        Ok(kg_domain::OpResult::Warnings(failures))
    }
}

async fn remove_worktrees(
    this: &gpui_kit::WeakEntity<crate::Workspace>,
    project: &std::path::Path,
    worktrees: Vec<kg_domain::Environment>,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    for environment in worktrees {
        #[cfg(test)]
        {
            let hold = this
                .update(cx, |workspace, _| workspace.removal_hold.clone())
                .map_err(closed)?;
            if let Some(hold) = hold {
                let _ = hold.recv().await;
            }
        }
        let path = environment.path.clone();
        let retirement = crate::environment::Retirement {
            environment: environment.clone(),
            project: project.to_path_buf(),
        };
        let outcome = crate::environment::remove_folder(this, retirement, cx).await;
        if let Err(error) = &outcome {
            let next = format!(
                "Commit, delete, or git-ignore them, then run `git worktree remove {}` in the project folder.",
                path.display()
            );
            let reason = crate::environment::removal_refusal(&path, error, &next);
            failures.push(format!("{}: {reason}", path.display()));
        }
        crate::environment::finish_removal(this, environment, outcome, cx).await?;
    }
    Ok(failures)
}

pub(crate) fn reorder(
    store: &kg_store::Store,
    id: &ProjectId,
    before: Option<&ProjectId>,
) -> kg_store::Result<kg_domain::OpResult> {
    if let Some(before) = before
        && store
            .project(before)?
            .is_some_and(|project| project.kind == ProjectKind::Personal)
    {
        return Err(kg_store::Error::Conflict(
            "Personal stays last; omit --before to move to the end".into(),
        ));
    }
    store.reorder_project(id, before)?;
    Ok(kg_domain::OpResult::Done)
}

fn removable(project: &Project, threads: &[kg_domain::Thread]) -> Result<()> {
    if project.kind == ProjectKind::Personal {
        return Err(OpError::Invalid("Personal cannot be removed".into()));
    }
    if threads.iter().any(|thread| {
        matches!(
            thread.status,
            kg_domain::ThreadStatus::Starting
                | kg_domain::ThreadStatus::Active
                | kg_domain::ThreadStatus::Stopping
        )
    }) {
        return Err(OpError::Invalid(format!(
            "Stop the programs in “{}” first.",
            project.name
        )));
    }
    Ok(())
}
