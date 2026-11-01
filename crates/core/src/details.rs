use std::path::Path;

use gpui_kit::{AppContext as _, AsyncApp};
use kg_domain::{BaseBranch, DiffCommit, GitDetails, OpError, OpResult, ThreadDetails, ThreadId};

use crate::changes::{current_branch, git, repository};
use crate::database::{Database, Result};
use crate::workspace::missing;

const COMMITS: &str = "5";

/// Reads a thread's details off the window's thread. Nothing here fetches or writes to the repository.
pub(crate) async fn read(database: Database, id: ThreadId, cx: &mut AsyncApp) -> Result<OpResult> {
    let lookup = id.clone();
    let (thread, environment) = database
        .call(move |store| {
            let Some(thread) = store.thread(&lookup)? else {
                return Ok((None, None));
            };
            let environment = match &thread.environment_id {
                Some(id) => store.environment(id)?,
                None => None,
            };
            Ok((Some(thread), environment))
        })
        .await?;
    let thread = thread.ok_or_else(|| missing("thread", &id))?;
    let environment =
        environment.ok_or_else(|| OpError::Invalid("the thread has no folder".into()))?;
    let folder = environment.path;
    let hint = environment.base_branch;
    let root = folder.clone();
    let git = cx
        .background_spawn(async move { git_details(&root, hint.as_deref()) })
        .await;
    Ok(OpResult::Details(ThreadDetails {
        folder,
        environment: environment.provider,
        program: thread.provider,
        git,
    }))
}

fn git_details(root: &Path, hint: Option<&str>) -> Option<GitDetails> {
    if !repository(root) {
        return None;
    }
    let base = crate::diff::resolve_base(root, None, hint)
        .ok()
        .and_then(|name| {
            let range = format!("{name}...HEAD");
            let counts = git(root, &["rev-list", "--left-right", "--count", &range, "--"]).ok()?;
            let mut counts = counts.split_whitespace().map(str::parse::<u32>);
            let (Some(Ok(behind)), Some(Ok(ahead))) = (counts.next(), counts.next()) else {
                return None;
            };
            Some(BaseBranch {
                name,
                ahead,
                behind,
            })
        });
    let commits = git(root, &["log", "-n", COMMITS, "--format=%h%x09%s"])
        .map(|text| {
            text.lines()
                .filter_map(|line| {
                    let (rev, summary) = line.split_once('\t')?;
                    Some(DiffCommit {
                        rev: rev.to_owned(),
                        summary: summary.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(GitDetails {
        branch: current_branch(root),
        base,
        commits,
    })
}
