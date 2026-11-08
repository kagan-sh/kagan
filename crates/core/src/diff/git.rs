use std::path::Path;

use crate::host::database::Result;

pub(super) fn diff(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
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

pub(super) use crate::host::git::{git_bytes as git_ok, git_captured as git};

pub(super) fn git_text(root: &Path, args: &[&str], index: Option<&Path>) -> Result<String> {
    Ok(text(&git_ok(root, args, index)?))
}

pub(super) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}
