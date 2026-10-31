use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read as _;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, AsyncApp};
use kg_domain::{
    DiffCommit, DiffFile, DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffList, DiffOp,
    DiffQuery, DiffScope, DiffStatus, OpError, OpResult,
};
use kg_process::Captured;

use crate::database::{Database, Result};
use crate::workspace::missing;

const PATCH_LIMIT: usize = 1024 * 1024;
const FILE_LIMIT: u64 = 1024 * 1024;

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
    let lookup = thread_id.clone();
    let thread = database
        .call(move |store| store.thread(&lookup))
        .await?
        .ok_or_else(|| missing("thread", thread_id))?;
    let environment_id = thread
        .environment_id
        .ok_or_else(|| OpError::Invalid("thread has no environment".into()))?;
    let lookup = environment_id.clone();
    let environment = database
        .call(move |store| store.environment(&lookup))
        .await?
        .ok_or_else(|| missing("environment", &environment_id))?;
    if environment.status != kg_domain::EnvironmentStatus::Ready {
        return Err(OpError::Failed("environment is not ready".into()));
    }
    Ok((environment.path, environment.base_branch, op))
}

#[derive(Default, Serialize, Deserialize)]
struct Snapshot {
    tree: String,
    skipped: BTreeMap<String, Skipped>,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct Skipped {
    binary: bool,
    too_large: bool,
    bytes: u64,
    modified: u128,
    object: String,
}

fn trees(
    root: &Path,
    hint: Option<&str>,
    query: &DiffQuery,
) -> Result<(Snapshot, Snapshot, Option<String>)> {
    ends(root, hint, query)
}

/// Runs `f` on the cache entry for `key`, emptied first when its trees are not `left` and `right`.
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
    if !crate::changes::repository(root) {
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

fn snapshot_key(snapshot: &Snapshot) -> String {
    format!(
        "{}:{}",
        snapshot.tree,
        serde_json::to_string(&snapshot.skipped).unwrap_or_default()
    )
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
    if !crate::changes::repository(root) {
        return Ok(DiffFileView {
            path: path.to_owned(),
            revision: String::new(),
            binary: false,
            truncated: false,
            too_large: false,
            hunks: Vec::new(),
            file: None,
            added: 0,
            removed: 0,
        });
    }
    let (left, right, base) = trees(root, hint, query)?;
    if let Some(skipped) = right.skipped.get(path).or_else(|| left.skipped.get(path)) {
        let revision = file_revision(root, &left, &right, path, path)?;
        return Ok(DiffFileView {
            path: path.to_owned(),
            revision,
            binary: skipped.binary,
            truncated: false,
            too_large: skipped.too_large,
            hunks: Vec::new(),
            file: None,
            added: 0,
            removed: 0,
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

fn commits(root: &Path) -> Result<Vec<DiffCommit>> {
    if !crate::changes::repository(root) {
        return Ok(Vec::new());
    }
    let stdout = git_ok(root, &["log", "--format=%H%x09%s", "-n", "50"], None)?;
    let text = String::from_utf8_lossy(&stdout);
    Ok(text
        .lines()
        .filter_map(|line| {
            let (rev, summary) = line.split_once('\t')?;
            Some(DiffCommit {
                rev: rev.to_owned(),
                summary: summary.to_owned(),
            })
        })
        .collect())
}

fn ends(
    root: &Path,
    hint: Option<&str>,
    query: &DiffQuery,
) -> Result<(Snapshot, Snapshot, Option<String>)> {
    match query.scope {
        DiffScope::Uncommitted => Ok((
            Snapshot {
                tree: head_or_empty(root)?,
                ..Snapshot::default()
            },
            snapshot(root)?,
            None,
        )),
        DiffScope::Branch => {
            let base = resolve_base(root, query.base.as_deref(), hint)?;
            let tip = commit_of(root, &base)
                .map_err(|_| OpError::Failed(format!("The base branch {base} does not exist")))?;
            let merge = git_text(root, &["merge-base", "HEAD", &tip], None)?;
            Ok((
                Snapshot {
                    tree: merge,
                    ..Snapshot::default()
                },
                snapshot(root)?,
                Some(base),
            ))
        }
        DiffScope::Commit => {
            let rev = query
                .rev
                .as_deref()
                .filter(|rev| !rev.is_empty())
                .ok_or_else(|| OpError::Invalid("Name the commit to review".into()))?;
            let rev = commit_of(root, rev)?;
            let parent = git(
                root,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &format!("{rev}^"),
                ],
                None,
            )?;
            let left = if parent.code == 0 {
                text(&parent.stdout)
            } else {
                empty_tree(root)?
            };
            Ok((
                Snapshot {
                    tree: left,
                    ..Snapshot::default()
                },
                Snapshot {
                    tree: rev,
                    ..Snapshot::default()
                },
                None,
            ))
        }
    }
}

fn diff_list(root: &Path, left: &str, right: &str, base: Option<String>) -> Result<DiffList> {
    let status = diff(root, &["--name-status", "-z", left, right])?;
    let numbers = diff(root, &["--numstat", "-z", left, right])?;
    let names = parse_status(&status);
    let counts = parse_numstat(&numbers);
    let mut files = Vec::new();
    let mut added = 0u32;
    let mut removed = 0u32;
    for (path, old_path, kind) in names {
        let stat = counts.iter().find(|stat| stat.path == path);
        let file_added = stat.map_or(0, |stat| stat.added);
        let file_removed = stat.map_or(0, |stat| stat.removed);
        let binary = stat.is_some_and(|stat| stat.binary);
        added = added.saturating_add(file_added);
        removed = removed.saturating_add(file_removed);
        files.push(DiffFile {
            path,
            old_path,
            added: file_added,
            removed: file_removed,
            binary,
            status: kind,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(DiffList {
        files,
        added,
        removed,
        base,
        repository: true,
    })
}

fn diff_file(
    root: &Path,
    left: &str,
    right: &str,
    path: &str,
    old: Option<&str>,
) -> Result<DiffFileView> {
    let mut paths = vec!["--", path];
    paths.extend(old);
    let numbers = diff(
        root,
        &[&["--numstat", "-z", left, right], &paths[..]].concat(),
    )?;
    let stat = parse_numstat(&numbers).into_iter().next();
    let added = stat.as_ref().map_or(0, |stat| stat.added);
    let removed = stat.as_ref().map_or(0, |stat| stat.removed);
    let binary = stat.as_ref().is_some_and(|stat| stat.binary);
    if binary {
        return Ok(DiffFileView {
            path: path.to_owned(),
            revision: String::new(),
            binary: true,
            truncated: false,
            too_large: false,
            hunks: Vec::new(),
            file: None,
            added,
            removed,
        });
    }
    let unified = diff(
        root,
        &[&["--no-color", "--unified=3", left, right], &paths[..]].concat(),
    )?;
    let truncated = unified.len() > PATCH_LIMIT;
    let hunks = if truncated {
        Vec::new()
    } else {
        parse_patch(&String::from_utf8_lossy(&unified))
    };
    let (file, too_large, binary) = read_side(root, right, path)?;
    Ok(DiffFileView {
        path: path.to_owned(),
        revision: String::new(),
        binary,
        truncated,
        too_large,
        hunks: if binary { Vec::new() } else { hunks },
        file: if binary { None } else { file },
        added,
        removed,
    })
}

fn read_side(root: &Path, rev: &str, path: &str) -> Result<(Option<String>, bool, bool)> {
    let spec = format!("{rev}:{path}");
    let size = git(root, &["cat-file", "-s", &spec], None)?;
    if size.code != 0 {
        return Ok((None, false, false));
    }
    let bytes = text(&size.stdout).parse::<u64>().unwrap_or(FILE_LIMIT + 1);
    if bytes > FILE_LIMIT {
        return Ok((None, true, false));
    }
    let body = git_ok(root, &["cat-file", "-p", &spec], None)?;
    if body.contains(&0) {
        return Ok((None, false, true));
    }
    Ok((
        Some(String::from_utf8_lossy(&body).into_owned()),
        false,
        false,
    ))
}

struct Stat {
    path: String,
    added: u32,
    removed: u32,
    binary: bool,
}

fn parse_numstat(bytes: &[u8]) -> Vec<Stat> {
    let parts: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut index = 0;
    let mut stats = Vec::new();
    while index < parts.len() {
        let record = parts[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        let record = String::from_utf8_lossy(record);
        let mut fields = record.split('\t');
        let added = fields.next().unwrap_or("");
        let removed = fields.next().unwrap_or("");
        let path = fields.next().unwrap_or("");
        let binary = added == "-" || removed == "-";
        let path = if path.is_empty() {
            if index + 1 >= parts.len() {
                break;
            }
            let path = String::from_utf8_lossy(parts[index + 1]).into_owned();
            index += 2;
            path
        } else {
            path.to_owned()
        };
        stats.push(Stat {
            path,
            added: if binary {
                0
            } else {
                added.parse().unwrap_or(0)
            },
            removed: if binary {
                0
            } else {
                removed.parse().unwrap_or(0)
            },
            binary,
        });
    }
    stats
}

fn parse_status(bytes: &[u8]) -> Vec<(String, Option<String>, DiffStatus)> {
    let parts: Vec<&[u8]> = bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect();
    let mut index = 0;
    let mut files = Vec::new();
    while index < parts.len() {
        let code = String::from_utf8_lossy(parts[index]);
        index += 1;
        if index >= parts.len() {
            break;
        }
        if code.starts_with('R') || code.starts_with('C') {
            let old = String::from_utf8_lossy(parts[index]).into_owned();
            index += 1;
            if index >= parts.len() {
                break;
            }
            let path = String::from_utf8_lossy(parts[index]).into_owned();
            index += 1;
            files.push((path, Some(old), DiffStatus::Renamed));
            continue;
        }
        let path = String::from_utf8_lossy(parts[index]).into_owned();
        index += 1;
        let status = match code.as_bytes().first() {
            Some(b'A') => DiffStatus::Added,
            Some(b'D') => DiffStatus::Deleted,
            _ => DiffStatus::Modified,
        };
        files.push((path, None, status));
    }
    files
}

fn parse_patch(text: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;
    let mut old_line = 0u32;
    let mut new_line = 0u32;
    for line in text.split_inclusive('\n') {
        let line = line.trim_end_matches(['\n', '\r']);
        if let Some(header) = line.strip_prefix("@@") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let (old_start, new_start) = hunk_starts(header);
            old_line = old_start;
            new_line = new_start;
            current = Some(DiffHunk {
                old_start,
                new_start,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            continue;
        };
        let Some(kind) = line.chars().next() else {
            continue;
        };
        if kind == '\\' {
            continue;
        }
        let text = line.get(1..).unwrap_or("").to_owned();
        match kind {
            '+' => {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Add,
                    old: None,
                    new: (new_line > 0).then_some(new_line),
                    text,
                });
                new_line = new_line.saturating_add(1);
            }
            '-' => {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Remove,
                    old: (old_line > 0).then_some(old_line),
                    new: None,
                    text,
                });
                old_line = old_line.saturating_add(1);
            }
            ' ' => {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    old: (old_line > 0).then_some(old_line),
                    new: (new_line > 0).then_some(new_line),
                    text,
                });
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
            }
            _ => {}
        }
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    hunks
}

fn hunk_starts(header: &str) -> (u32, u32) {
    let mut old = 0u32;
    let mut new = 0u32;
    for part in header.split_whitespace() {
        if let Some(rest) = part.strip_prefix('-') {
            old = rest.split(',').next().unwrap_or("0").parse().unwrap_or(0);
        } else if let Some(rest) = part.strip_prefix('+') {
            new = rest.split(',').next().unwrap_or("0").parse().unwrap_or(0);
        }
    }
    (old, new)
}

fn commit_of(root: &Path, rev: &str) -> Result<String> {
    if rev.starts_with('-') {
        return Err(OpError::Invalid(format!("{rev} is not a revision")));
    }
    git_text(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
        None,
    )
}

fn snapshot(root: &Path) -> Result<Snapshot> {
    let git_dir = PathBuf::from(git_text(root, &["rev-parse", "--absolute-git-dir"], None)?);
    let index = TempIndex(git_dir.join(format!("kg-diff-index-{}", uuid::Uuid::new_v4())));
    let real = git_dir.join("index");
    if real.exists() {
        std::fs::copy(&real, &index.0)?;
        // A newer copy makes git trust entries whose size and timestamp still match.
        std::fs::File::options()
            .write(true)
            .open(&index.0)?
            .set_modified(std::fs::metadata(&real)?.modified()?)?;
    } else {
        git_ok(root, &["read-tree", "--empty"], Some(&index.0))?;
    }
    let mut names = git_ok(
        root,
        &[
            "ls-files",
            "-z",
            "--modified",
            "--deleted",
            "--others",
            "--exclude-standard",
        ],
        Some(&index.0),
    )?;
    names.extend(diff(root, &["--cached", "--name-only", "-z"])?);
    let indexed = indexed_paths(root, &index.0)?;
    let mut skipped = BTreeMap::new();
    let mut include = Vec::new();
    for path in names
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8(path.to_vec())
            .map_err(|_| OpError::Failed("a git path is not UTF-8".into()))?;
        // `git add` rejects a pathspec that matches nothing. A staged delete is already gone from this index copy.
        let on_disk = std::fs::symlink_metadata(root.join(&path)).is_ok();
        if !on_disk && !indexed.contains(path.as_bytes()) {
            continue;
        }
        if let Some(mut omitted) = skip(&root.join(&path))? {
            omitted.object = git_text(root, &["hash-object", "--no-filters", "--", &path], None)?;
            git_ok(
                root,
                &["update-index", "--force-remove", "--", &path],
                Some(&index.0),
            )?;
            skipped.insert(path, omitted);
        } else {
            include.push(path);
        }
    }
    for chunk in include.chunks(256) {
        let mut args = vec!["--literal-pathspecs", "add", "-A", "--"];
        args.extend(chunk.iter().map(String::as_str));
        git_ok(root, &args, Some(&index.0))?;
    }
    Ok(Snapshot {
        tree: git_text(root, &["write-tree"], Some(&index.0))?,
        skipped,
    })
}

fn indexed_paths(root: &Path, index: &Path) -> Result<BTreeSet<Vec<u8>>> {
    let listed = git_ok(root, &["ls-files", "-z"], Some(index))?;
    Ok(listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(<[u8]>::to_vec)
        .collect())
}

fn skip(path: &Path) -> Result<Option<Skipped>> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let too_large = meta.len() > FILE_LIMIT;
    let binary = if too_large {
        false
    } else {
        let mut body = Vec::new();
        std::fs::File::open(path)?
            .take(FILE_LIMIT + 1)
            .read_to_end(&mut body)?;
        body.contains(&0)
    };
    if !too_large && !binary {
        return Ok(None);
    }
    Ok(Some(Skipped {
        binary,
        too_large,
        bytes: meta.len(),
        object: String::new(),
        modified: meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| OpError::Failed(error.to_string()))?
            .as_nanos(),
    }))
}

fn head_or_empty(root: &Path) -> Result<String> {
    let head = git(
        root,
        &["rev-parse", "--verify", "--end-of-options", "HEAD"],
        None,
    )?;
    if head.code == 0 {
        Ok(text(&head.stdout))
    } else {
        empty_tree(root)
    }
}

fn empty_tree(root: &Path) -> Result<String> {
    git_text(root, &["mktree"], None)
}

fn resolve_base(root: &Path, requested: Option<&str>, hint: Option<&str>) -> Result<String> {
    if let Some(base) = requested.or(hint).filter(|base| !base.is_empty()) {
        return Ok(base.to_owned());
    }
    let origin = git(
        root,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
        None,
    )?;
    if origin.code == 0 {
        let full = text(&origin.stdout);
        if let Some(name) = full
            .strip_prefix("refs/remotes/")
            .filter(|name| !name.is_empty())
        {
            return Ok(name.to_owned());
        }
    }
    for name in ["main", "master"] {
        if commit_of(root, name).is_ok() {
            return Ok(name.to_owned());
        }
    }
    Err(OpError::Failed("This repository has no base branch".into()))
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

struct TempIndex(PathBuf);

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let mut lock = self.0.clone().into_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(lock);
    }
}

fn diff(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut all = vec![
        "--literal-pathspecs",
        "diff",
        "--no-textconv",
        "--no-ext-diff",
        "--find-renames",
    ];
    all.extend_from_slice(args);
    git_ok(root, &all, None)
}

fn git(root: &Path, args: &[&str], index: Option<&Path>) -> Result<Captured> {
    let program = kg_process::find_program(
        Path::new("git"),
        std::env::var_os("PATH").as_deref(),
        kg_process::pathext().as_deref(),
    )
    .ok_or_else(|| OpError::Failed("git is not installed".into()))?;
    let mut command = Command::new(program);
    command.arg("-C").arg(root).args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    kg_process::capture(&mut command).map_err(OpError::from)
}

fn git_ok(root: &Path, args: &[&str], index: Option<&Path>) -> Result<Vec<u8>> {
    let captured = git(root, args, index)?;
    if captured.code == 0 {
        Ok(captured.stdout)
    } else if captured.stderr.is_empty() {
        Err(OpError::Failed(format!("git failed ({})", captured.code)))
    } else {
        Err(OpError::Failed(captured.stderr))
    }
}

fn git_text(root: &Path, args: &[&str], index: Option<&Path>) -> Result<String> {
    Ok(text(&git_ok(root, args, index)?))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}
