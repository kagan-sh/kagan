use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    EnvironmentStatus, EventKind, NoticeSeverity, OpError, OpResult, Run, ThreadId, ThreadStatus,
};
use kg_process::{find_program, pathext};

use super::clean_title;
use crate::Workspace;
use crate::database::Result;
use crate::workspace::{id, missing, time};

pub(crate) struct ResolvedProgram {
    pub(crate) path: PathBuf,
    pub(crate) args: Vec<OsString>,
}

/// The program named by `line`: words split on spaces, where single or double quotes keep spaces
/// together. A backslash is an ordinary character, so Windows paths work unquoted.
///
/// # Errors
///
/// The line is empty, a quote is unfinished, or the program is not on the login `PATH`.
pub(crate) fn resolve_program(
    line: &str,
    environment: Option<&crate::program::ProgramEnvironment>,
) -> std::result::Result<ResolvedProgram, String> {
    let (name, args) = kg_domain::split_program(line)?;
    let found = find_program(
        Path::new(&name),
        crate::program::search_path(environment).as_deref(),
        pathext().as_deref(),
    )
    .ok_or_else(|| format!("program {name} was not found"))?;
    Ok(ResolvedProgram {
        path: found,
        args: args.into_iter().map(OsString::from).collect(),
    })
}

pub(crate) fn worktree_branch(
    environment: &kg_domain::EnvironmentSpec,
    title: &str,
    thread_id: &ThreadId,
) -> Result<Option<String>> {
    let kg_domain::EnvironmentSpec::Worktree { branch, .. } = environment else {
        return Ok(None);
    };
    let name = if let Some(name) = branch {
        name.clone()
    } else {
        let slug = kg_domain::branch_slug(title);
        if slug.is_empty() {
            let suffix = thread_id.0.strip_prefix("thr_").unwrap_or(&thread_id.0);
            format!("thread-{suffix}")
        } else {
            slug
        }
    };
    if crate::git::valid_branch(&name) {
        Ok(Some(name))
    } else {
        Err(OpError::Invalid(format!(
            "branch name {name} is not allowed"
        )))
    }
}

pub(crate) struct PreparedRun {
    thread_id: ThreadId,
    project: kg_domain::Project,
    environment: crate::environment::Prepared,
    title: String,
    title_locked: bool,
    program: String,
    visibility: kg_domain::Visibility,
    parent_thread_id: Option<ThreadId>,
    lifecycle_owner_thread_id: Option<ThreadId>,
    metadata: Option<kg_domain::Metadata>,
}

pub(crate) async fn prepare(
    this: &WeakEntity<Workspace>,
    run: Run,
    cx: &mut AsyncApp,
) -> Result<PreparedRun> {
    let environment = crate::program::program_environment_of(this, cx);
    resolve_program(&run.program, environment.as_ref()).map_err(OpError::Invalid)?;
    let program = run.program.clone();
    let project_id = run.project_id.clone();
    let database = crate::workspace::database(this, cx)?;
    let project = database
        .call(move |store| store.project(&project_id))
        .await?
        .ok_or_else(|| missing("project", &run.project_id))?;
    let thread_id = ThreadId(id("thr"));
    let title_locked = run.title.is_some();
    let title = if let Some(title) = run.title.as_deref() {
        clean_title(title)?
    } else {
        let mut chars = run.program.chars();
        let mut program: String = chars.by_ref().take(80).collect();
        if chars.next().is_some() {
            program.pop();
            program.push('…');
        }
        clean_title(&program).unwrap_or_else(|_| "Thread".into())
    };
    let branch = worktree_branch(&run.environment, &title, &thread_id)?;
    let environment =
        crate::environment::prepare(this, &project, run.environment, branch, &title, cx).await?;
    Ok(PreparedRun {
        thread_id,
        project,
        environment,
        title,
        title_locked,
        program,
        visibility: run.visibility.unwrap_or_default(),
        parent_thread_id: run.parent_thread_id,
        lifecycle_owner_thread_id: run.lifecycle_owner_thread_id,
        metadata: run.metadata,
    })
}

pub(crate) async fn commit(
    this: &WeakEntity<Workspace>,
    prepared: PreparedRun,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let PreparedRun {
        thread_id,
        project,
        environment,
        title,
        title_locked,
        program,
        visibility,
        parent_thread_id,
        lifecycle_owner_thread_id,
        metadata,
    } = prepared;
    let database = crate::workspace::database(this, cx)?;
    for id in [&parent_thread_id, &lifecycle_owner_thread_id]
        .into_iter()
        .flatten()
    {
        let record = super::thread(this, id, cx)?.read_with(cx, |thread, _| thread.record.clone());
        if record.project_id != project.id || record.archived_at.is_some() {
            return Err(OpError::Invalid(
                "parent and lifecycle owner must be unarchived and in this project".into(),
            ));
        }
    }
    let (environment, notices) =
        crate::environment::commit(&database, &project, environment).await?;
    #[cfg(test)]
    {
        let hold = this
            .update(cx, |workspace, _| workspace.run_hold.clone())
            .map_err(crate::workspace::closed)?;
        if let Some(hold) = hold {
            let _ = hold.recv().await;
        }
    }
    let folder = environment.path.clone();
    let mut row = environment.clone();
    row.retire_at = None;
    let project_id = project.id.clone();
    let at = time(this, cx)?;
    let record = kg_domain::Thread {
        id: thread_id,
        project_id: project.id,
        environment_id: Some(environment.id),
        title,
        title_locked,
        provider: program,
        status: ThreadStatus::Starting,
        visibility,
        parent_thread_id,
        lifecycle_owner_thread_id,
        section_id: None,
        pinned: false,
        last_read_at: Some(at),
        latest_attention_at: at,
        mark: kg_domain::ThreadMark::None,
        archived_at: None,
        created_at: at,
        updated_at: at,
    };
    let saved = record.clone();
    database
        .call(move |store| {
            let current = store.environment(&row.id)?;
            if current.is_none_or(|current| {
                current.status != EnvironmentStatus::Ready || current.project_id != project_id
            }) {
                return Err(kg_store::Error::Conflict(
                    "environment must be ready and belong to this project".into(),
                ));
            }
            if store.removal_pending(&folder)? {
                return Err(kg_store::Error::Conflict(
                    "that folder is being removed".into(),
                ));
            }
            store.put_environment(&row)?;
            store.put_thread(&saved)?;
            if let Some(metadata) = metadata {
                store.set_thread_metadata(&saved.id, &metadata.plugin_id, &metadata.value)?;
            }
            Ok(())
        })
        .await?;
    Workspace::events(this, record.id.clone(), created_with(notices), cx).await?;
    Workspace::refresh(this, cx).await?;
    let entity = super::thread(this, &record.id, cx)?;
    super::start_terminal(this, &entity, super::agent::Start::First, cx).await?;
    Ok(OpResult::Thread(
        entity.read_with(cx, |thread, _| thread.record.clone()),
    ))
}

fn created_with(notices: Vec<String>) -> Vec<EventKind> {
    let mut kinds = vec![EventKind::Created];
    kinds.extend(notices.into_iter().map(|text| EventKind::Notice {
        severity: NoticeSeverity::Warning,
        text,
    }));
    kinds
}
