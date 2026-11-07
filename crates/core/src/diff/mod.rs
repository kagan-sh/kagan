use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, AsyncApp};
use kg_domain::{
    DiffFile, DiffFileView, DiffList, DiffOp, DiffQuery, DiffStatus, OpError, OpResult,
};

use crate::host::database::{Database, Result};

mod git;
mod patch;
mod snapshot;

pub(crate) use snapshot::resolve_base;

use git::{git, git_text};
use patch::{commits, diff_file, diff_list};
use snapshot::{Snapshot, snapshot_key, trees};

pub(super) const PATCH_LIMIT: usize = 1024 * 1024;
pub(super) const FILE_LIMIT: u64 = 1024 * 1024;

struct Entry {
    left: String,
    right: String,
    list: Option<DiffList>,
    files: HashMap<String, DiffFileView>,
}

pub(crate) struct Cache {
    entries: Mutex<HashMap<String, Entry>>,
}

impl Cache {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(HashMap::new()),
        })
    }
}

pub(crate) async fn run(database: Database, op: DiffOp, cx: &mut AsyncApp) -> Result<OpResult> {
    let cache = cx.update(|cx| crate::Workspace::global(cx).read(cx).diffs.clone());
    let (root, hint, op) = locate(database, op).await?;
    cx.background_spawn(async move {
        match op {
            DiffOp::List { query } => {
                files(&cache, &root, hint.as_deref(), &query).map(OpResult::Diff)
            }
            DiffOp::Show { query, path } => {
                file(&cache, &root, hint.as_deref(), &query, &path).map(OpResult::DiffFile)
            }
            DiffOp::Commits { .. } => commits(&root).map(OpResult::Commits),
        }
    })
    .await
}

async fn locate(database: Database, op: DiffOp) -> Result<(PathBuf, Option<String>, DiffOp)> {
    let thread_id = match &op {
        DiffOp::List { query } | DiffOp::Show { query, .. } => &query.thread_id,
        DiffOp::Commits { thread_id } => thread_id,
    };
    let environment = crate::thread::ready_environment(&database, thread_id).await?;
    Ok((environment.path, environment.base_branch, op))
}

fn cached<T>(
    cache: &Cache,
    key: &str,
    left: &str,
    right: &str,
    f: impl FnOnce(&mut Entry) -> T,
) -> T {
    let mut guard = cache
        .entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = guard.entry(key.to_owned()).or_insert_with(|| Entry {
        left: left.to_owned(),
        right: right.to_owned(),
        list: None,
        files: HashMap::new(),
    });
    if entry.left != left || entry.right != right {
        left.clone_into(&mut entry.left);
        right.clone_into(&mut entry.right);
        entry.list = None;
        entry.files.clear();
    }
    f(entry)
}

fn files(cache: &Cache, root: &Path, hint: Option<&str>, query: &DiffQuery) -> Result<DiffList> {
    if !crate::host::git::is_repository(root) {
        return Ok(DiffList {
            files: Vec::new(),
            added: 0,
            removed: 0,
            base: None,
            repository: false,
        });
    }
    let (left, right, base) = trees(root, hint, query)?;
    listed(cache, root, &cache_key(root, query), &left, &right, base)
}

fn listed(
    cache: &Cache,
    root: &Path,
    key: &str,
    left: &Snapshot,
    right: &Snapshot,
    base: Option<String>,
) -> Result<DiffList> {
    let left_key = snapshot_key(left);
    let right_key = snapshot_key(right);
    if let Some(hit) = cached(cache, key, &left_key, &right_key, |entry| {
        entry.list.clone()
    }) {
        return Ok(hit);
    }
    let mut list = diff_list(root, &left.tree, &right.tree, base)?;
    for (path, skipped) in &right.skipped {
        list.files.retain(|file| &file.path != path);
        if left.skipped.get(path) == Some(skipped) {
            continue;
        }
        let existed = left.skipped.contains_key(path)
            || git(
                root,
                &["cat-file", "-e", &format!("{}:{path}", left.tree)],
                None,
            )?
            .code
                == 0;
        list.files.push(DiffFile {
            path: path.clone(),
            old_path: None,
            added: 0,
            removed: 0,
            binary: skipped.binary,
            status: if existed {
                DiffStatus::Modified
            } else {
                DiffStatus::Added
            },
        });
    }
    for (path, skipped) in &left.skipped {
        if !right.skipped.contains_key(path) && !list.files.iter().any(|file| &file.path == path) {
            list.files.push(DiffFile {
                path: path.clone(),
                old_path: None,
                added: 0,
                removed: 0,
                binary: skipped.binary,
                status: DiffStatus::Deleted,
            });
        }
    }
    list.files.sort_by(|left, right| left.path.cmp(&right.path));
    list.added = list
        .files
        .iter()
        .map(|file| file.added)
        .fold(0, u32::saturating_add);
    list.removed = list
        .files
        .iter()
        .map(|file| file.removed)
        .fold(0, u32::saturating_add);
    cached(cache, key, &left_key, &right_key, |entry| {
        entry.list = Some(list.clone());
    });
    Ok(list)
}

fn file(
    cache: &Cache,
    root: &Path,
    hint: Option<&str>,
    query: &DiffQuery,
    path: &str,
) -> Result<DiffFileView> {
    if Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(OpError::Invalid(
            "the diff path must be inside the repository".into(),
        ));
    }
    if !crate::host::git::is_repository(root) {
        return Ok(DiffFileView::empty(path));
    }
    let (left, right, base) = trees(root, hint, query)?;
    if let Some(skipped) = right.skipped.get(path).or_else(|| left.skipped.get(path)) {
        let revision = file_revision(root, &left, &right, path, path)?;
        return Ok(DiffFileView {
            revision,
            binary: skipped.binary,
            too_large: skipped.too_large,
            ..DiffFileView::empty(path)
        });
    }
    let key = cache_key(root, query);
    let left_key = snapshot_key(&left);
    let right_key = snapshot_key(&right);
    if let Some(hit) = cached(cache, &key, &left_key, &right_key, |entry| {
        entry.files.get(path).cloned()
    }) {
        return Ok(hit);
    }
    let list = listed(cache, root, &key, &left, &right, base)?;
    let old = list
        .files
        .iter()
        .find(|file| file.path == path)
        .and_then(|file| file.old_path.as_deref());
    let revision = file_revision(root, &left, &right, old.unwrap_or(path), path)?;
    let mut view = diff_file(root, &left.tree, &right.tree, path, old)?;
    view.revision = revision;
    cached(cache, &key, &left_key, &right_key, |entry| {
        entry.files.insert(path.to_owned(), view.clone());
    });
    Ok(view)
}

fn file_revision(
    root: &Path,
    left: &Snapshot,
    right: &Snapshot,
    old: &str,
    path: &str,
) -> Result<String> {
    fn side(root: &Path, snapshot: &Snapshot, path: &str) -> Result<String> {
        if let Some(skipped) = snapshot.skipped.get(path) {
            return Ok(format!("skipped:{}:{}", skipped.object, skipped.bytes));
        }
        git_text(
            root,
            &["--literal-pathspecs", "ls-tree", &snapshot.tree, "--", path],
            None,
        )
    }
    Ok(format!(
        "{}\n{}",
        side(root, left, old)?,
        side(root, right, path)?
    ))
}
fn cache_key(root: &Path, query: &DiffQuery) -> String {
    format!(
        "{}|{:?}|{}|{}|{}",
        root.display(),
        query.scope,
        query.base.as_deref().unwrap_or(""),
        query.rev.as_deref().unwrap_or(""),
        query.thread_id
    )
}
