use std::path::Path;
use std::process::Command;

use kg_domain::OpError;
use kg_process::Captured;

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

pub(super) fn git(root: &Path, args: &[&str], index: Option<&Path>) -> Result<Captured> {
    let program = kg_process::find_program(
        Path::new("git"),
        kg_process::user_path().as_deref(),
        kg_process::pathext().as_deref(),
    )
    .ok_or_else(|| OpError::Failed("git is not installed".into()))?;
    let mut command = Command::new(program);
    command.arg("-C").arg(root).args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    kg_process::capture_with_limit(command, crate::host::git::GIT_LIMIT).map_err(OpError::from)
}

pub(super) fn git_ok(root: &Path, args: &[&str], index: Option<&Path>) -> Result<Vec<u8>> {
    let captured = git(root, args, index)?;
    if captured.code == 0 {
        Ok(captured.stdout)
    } else if captured.stderr.is_empty() {
        Err(OpError::Failed(format!("git failed ({})", captured.code)))
    } else {
        Err(OpError::Failed(captured.stderr))
    }
}

pub(super) fn git_text(root: &Path, args: &[&str], index: Option<&Path>) -> Result<String> {
    Ok(text(&git_ok(root, args, index)?))
}

pub(super) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}
