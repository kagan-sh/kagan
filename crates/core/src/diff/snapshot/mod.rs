use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use kg_domain::{DiffQuery, DiffScope, OpError};
use serde::{Deserialize, Serialize};

use crate::host::database::Result;

use super::FILE_LIMIT;
use super::git::{diff, git, git_ok, git_text, text};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Snapshot {
    pub(super) tree: String,
    pub(super) skipped: BTreeMap<String, Skipped>,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Skipped {
    pub(super) binary: bool,
    pub(super) too_large: bool,
    pub(super) bytes: u64,
    pub(super) modified: u128,
    pub(super) object: String,
}

pub(super) fn snapshot_key(snapshot: &Snapshot) -> String {
    format!(
        "{}:{}",
        snapshot.tree,
        serde_json::to_string(&snapshot.skipped).unwrap_or_default()
    )
}
pub(super) fn ends(
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

#[rustfmt::skip]
pub(crate) fn resolve_base(
    root: &Path,
    requested: Option<&str>,
    hint: Option<&str>,
) -> Result<String> {
    if let Some(base) = requested.filter(|base| !base.is_empty()).or(hint.filter(|base| !base.is_empty())) {
        return Ok(base.to_owned());
    }
    let origin = git(root, &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"], None)?;
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
struct TempIndex(PathBuf);

impl Drop for TempIndex {
    fn drop(&mut self) {
        // A temp file left behind is harmless.
        let _ = std::fs::remove_file(&self.0);
        let mut lock = self.0.clone().into_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(lock);
    }
}

#[cfg(test)]
mod tests;
