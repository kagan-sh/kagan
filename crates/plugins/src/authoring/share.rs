use std::ffi::OsStr;
use std::path::Path;

use kg_domain::{GIT_SOURCE, IndexEntry, OpError, OpResult};

use crate::source::{git, git_command};
use crate::{Result, discover};

/// Checks a plugin that is committed and pushed, and returns its index entry and the line a
/// teammate runs. The entry pins the commit that was checked.
pub(crate) fn share(
    path: &Path,
    data_dir: &Path,
    components: &gpui_shell::FrozenComponentRegistry,
) -> Result<OpResult> {
    crate::development::check(path, data_dir, components)?;
    let manifest = discover(path, data_dir)?.manifest;
    let (_keep, never) = async_channel::bounded::<()>(1);
    let inside = git_command(
        Some(path),
        ["rev-parse", "--is-inside-work-tree"].map(OsStr::new),
        &never,
    )?;
    if inside.code != 0 || inside.stdout.trim_ascii() != b"true" {
        return Err(OpError::Invalid(format!(
            "{} is not in a git repository",
            path.display()
        )));
    }
    let run = |args: &[&str]| git(Some(path), args.iter().map(OsStr::new), &never);
    let top = run(&["rev-parse", "--show-toplevel"])?;
    if !run(&["status", "--porcelain", "--", "."])?.is_empty() {
        return Err(OpError::Invalid(format!(
            "commit the changes in {} first; a teammate gets the commit, not your files",
            path.display()
        )));
    }
    let commit = run(&["rev-parse", "HEAD"])?;
    let branches = run(&[
        "branch",
        "-r",
        "--contains",
        "HEAD",
        "--format=%(refname:short)",
    ])?;
    let remote = branches
        .lines()
        .filter_map(|branch| branch.split_once('/').map(|(remote, _)| remote))
        .min_by_key(|remote| *remote != "origin")
        .ok_or_else(|| {
            OpError::Invalid(format!(
                "push commit {commit} first; no remote branch holds it"
            ))
        })?;
    let url = crate::index::public(&run(&["remote", "get-url", remote])?);
    let folder = dunce::canonicalize(path)?;
    let plugin = folder
        .strip_prefix(dunce::canonicalize(top)?)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| relative.to_string_lossy().replace('\\', "/"));
    let source = format!("{GIT_SOURCE}{url}@{commit}");
    let name = manifest
        .id
        .0
        .rsplit('.')
        .next()
        .unwrap_or(&manifest.id.0)
        .to_owned();
    let description = manifest
        .instructions
        .as_deref()
        .and_then(|text| text.lines().next())
        .unwrap_or(&manifest.name)
        .chars()
        .take(300)
        .collect();
    let install = match &plugin {
        Some(plugin) => format!("kg plugin install {source} --plugin {plugin}"),
        None => format!("kg plugin install {source}"),
    };
    let entry = IndexEntry {
        name,
        description,
        source,
        plugin,
    };
    Ok(OpResult::Shared { entry, install })
}
