use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{EventKind, NoticeSeverity, OpError, OpResult, Run, ThreadId, ThreadStatus};
use kg_process::pathext;

use super::clean_title;
use crate::Workspace;
use crate::host::database::Result;
use crate::host::program::locate;
use crate::workspace::{id, missing, time};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ResolveError {
    #[error("{0}")]
    Line(kg_domain::SplitProgramError),
    #[error("program {0} was not found")]
    NotFound(String),
}

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
    environment: Option<&crate::host::program::ProgramEnvironment>,
    home: Option<&Path>,
) -> std::result::Result<ResolvedProgram, ResolveError> {
    let (name, args) = kg_domain::split_program(line).map_err(ResolveError::Line)?;
    let found = locate(
        Path::new(&name),
        crate::host::program::search_path(environment).as_deref(),
        pathext().as_deref(),
        home,
    )
    .ok_or(ResolveError::NotFound(name))?;
    Ok(ResolvedProgram {
        path: found,
        args: args.into_iter().map(OsString::from).collect(),
    })
}

pub(crate) fn worktree_branch(
    environment: &kg_domain::EnvironmentSpec,
    title: &str,
    thread_id: &ThreadId,
    prefix: &str,
) -> Result<Option<String>> {
    let requested = match environment {
        kg_domain::EnvironmentSpec::Worktree { branch, .. } => branch.as_deref(),
        kg_domain::EnvironmentSpec::ProjectOn {
            branch: kg_domain::ProjectBranch::New { .. },
        } => None,
        _ => return Ok(None),
    };
    let name = if let Some(name) = requested {
        name.to_owned()
    } else {
        let slug = kg_domain::branch_slug(title);
        if slug.is_empty() {
            let suffix = thread_id.0.strip_prefix("thr_").unwrap_or(&thread_id.0);
            format!("{prefix}thread-{suffix}")
        } else {
            format!("{prefix}{slug}")
        }
    };
    if kg_domain::valid_branch(&name) {
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
    quiet: bool,
    lifecycle_owner_thread_id: Option<ThreadId>,
    metadata: Option<kg_domain::Metadata>,
}

pub(crate) async fn prepare(
    this: &WeakEntity<Workspace>,
    run: Run,
    cx: &mut AsyncApp,
) -> Result<PreparedRun> {
    let environment = crate::host::program::program_environment_of(this, cx);
    let home = crate::host::program::program_home_of(this, cx);
    resolve_program(&run.program, environment.as_ref(), home.as_deref())
        .map_err(|error| OpError::Invalid(error.to_string()))?;
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
    let prefix = database
        .call(|store| store.setting(kg_domain::prefs::BRANCH_PREFIX))
        .await?;
    let prefix = prefix
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let branch = worktree_branch(&run.environment, &title, &thread_id, prefix)?;
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
        quiet: run.quiet,
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
        quiet,
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
    crate::workspace::hold(this, crate::workspace::Hold::Run, cx).await;
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
        quiet,
        notify: kg_domain::ThreadNotify::Default,
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
    let events = database
        .call(move |store| store.create_thread(&saved, metadata.as_ref(), created_with(notices)))
        .await?;
    this.update(cx, |_, cx| {
        for event in events {
            cx.emit(crate::WorkspaceEvent::Event {
                event,
                project_id: record.project_id.clone(),
            });
        }
        cx.notify();
    })
    .map_err(crate::workspace::closed)?;
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

#[cfg(test)]
mod tests {
    use kg_domain::{EnvironmentSpec, ProjectBranch, ThreadId};

    use super::worktree_branch;

    fn thread() -> ThreadId {
        ThreadId("thr_abc123".into())
    }

    #[test]
    fn kagan_named_branches_get_the_prefix() {
        let worktree = EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        };
        let new_on_project = EnvironmentSpec::ProjectOn {
            branch: ProjectBranch::New {
                base: "main".into(),
            },
        };
        for spec in [&worktree, &new_on_project] {
            let branch = worktree_branch(spec, "Fix login", &thread(), "bb/").unwrap();
            assert_eq!(branch.as_deref(), Some("bb/fix-login"));
        }
        let empty_title = worktree_branch(&worktree, "!!", &thread(), "bb/").unwrap();
        assert_eq!(empty_title.as_deref(), Some("bb/thread-abc123"));
    }

    #[test]
    fn a_typed_branch_is_used_as_typed() {
        let typed = EnvironmentSpec::Worktree {
            base_branch: None,
            branch: Some("feat/login".into()),
        };
        let branch = worktree_branch(&typed, "Fix login", &thread(), "bb/").unwrap();
        assert_eq!(branch.as_deref(), Some("feat/login"));
    }

    #[test]
    fn an_empty_prefix_keeps_the_old_name() {
        let worktree = EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        };
        let branch = worktree_branch(&worktree, "Fix login", &thread(), "").unwrap();
        assert_eq!(branch.as_deref(), Some("fix-login"));
    }
}
