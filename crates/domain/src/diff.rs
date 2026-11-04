use serde::{Deserialize, Serialize};

use crate::DiffScope;

/// One review note on a line or a range. Line numbers are 1-based.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffNote {
    pub scope: DiffScope,
    /// The commit, when the scope is a commit.
    pub rev: Option<String>,
    pub path: String,
    #[serde(default)]
    pub side: DiffSide,
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffSide {
    Old,
    #[default]
    New,
}

/// Formats notes as one user message. `parse_notes` reads it back.
#[must_use]
pub fn format_notes(notes: &[DiffNote]) -> String {
    let mut text = String::from("Review notes\n");
    for note in notes {
        text.push_str("\nscope: ");
        text.push_str(match note.scope {
            DiffScope::Uncommitted => "uncommitted",
            DiffScope::Branch => "branch",
            DiffScope::Commit => "commit",
        });
        if let Some(rev) = &note.rev {
            text.push(' ');
            text.push_str(rev);
        }
        text.push_str("\nfile: ");
        text.push_str(&note.path);
        text.push_str(match note.side {
            DiffSide::Old => "\nside: old\nlines: ",
            DiffSide::New => "\nside: new\nlines: ",
        });
        text.push_str(&note.start.to_string());
        if note.end != note.start {
            text.push('-');
            text.push_str(&note.end.to_string());
        }
        text.push('\n');
        for line in note.text.trim_end().split('\n') {
            text.push_str("  ");
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

/// The notes in a message from [`format_notes`].
#[must_use]
pub fn parse_notes(text: &str) -> Option<Vec<DiffNote>> {
    let mut lines = text.strip_prefix("Review notes\n")?.lines().peekable();
    let mut notes = Vec::new();
    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }
        let scope = line.strip_prefix("scope: ")?;
        let (scope, rev) = match scope.split_once(' ') {
            Some((scope, rev)) => (scope, Some(rev.to_owned())),
            None => (scope, None),
        };
        let scope = match scope {
            "uncommitted" => DiffScope::Uncommitted,
            "branch" => DiffScope::Branch,
            "commit" => DiffScope::Commit,
            _ => return None,
        };
        let path = lines.next()?.strip_prefix("file: ")?.to_owned();
        let side = match lines.next()? {
            "side: old" => DiffSide::Old,
            "side: new" => DiffSide::New,
            _ => return None,
        };
        let range = lines.next()?.strip_prefix("lines: ")?;
        let (start, end) = if let Some((start, end)) = range.split_once('-') {
            (start.parse().ok()?, end.parse().ok()?)
        } else {
            let line = range.parse().ok()?;
            (line, line)
        };
        if start == 0 || end < start {
            return None;
        }
        let mut body = Vec::new();
        while let Some(line) = lines.peek().and_then(|line| line.strip_prefix("  ")) {
            body.push(line);
            lines.next();
        }
        notes.push(DiffNote {
            scope,
            rev,
            path,
            side,
            start,
            end,
            text: body.join("\n"),
        });
    }
    Some(notes)
}

/// A changed file and its added and removed line counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub path: String,
    pub old_path: Option<String>,
    pub added: u32,
    pub removed: u32,
    pub binary: bool,
    pub status: DiffStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Changed files for one scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffList {
    pub files: Vec<DiffFile>,
    pub added: u32,
    pub removed: u32,
    /// The base branch, when the scope is the branch.
    pub base: Option<String>,
    /// Whether the folder is a git repository. A folder that is not has no changes to list.
    pub repository: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    Context,
    Add,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

/// One file's patch and, when it fits, the whole new side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileView {
    pub path: String,
    /// Identity of both compared versions of this file.
    pub revision: String,
    pub binary: bool,
    /// The patch is too large to return line by line.
    pub truncated: bool,
    /// The new side is too large to return as text.
    pub too_large: bool,
    pub hunks: Vec<DiffHunk>,
    /// The new side's text, when the patch is too large to return as hunks.
    pub file: Option<String>,
    pub added: u32,
    pub removed: u32,
}

impl DiffFileView {
    /// A file with no patch: no revision, no hunks, and no changed lines.
    #[must_use]
    pub fn empty(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            revision: String::new(),
            binary: false,
            truncated: false,
            too_large: false,
            hunks: Vec::new(),
            file: None,
            added: 0,
            removed: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCommit {
    pub rev: String,
    pub summary: String,
}

/// A thread's folder, program and git state, as the Details panel shows them. Read locally; nothing is fetched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDetails {
    pub folder: std::path::PathBuf,
    pub environment: crate::EnvironmentProvider,
    pub program: String,
    /// `None` outside a git folder
    pub git: Option<GitDetails>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDetails {
    /// `None` on a detached HEAD
    pub branch: Option<String>,
    /// `None` when the repository has no base branch
    pub base: Option<BaseBranch>,
    /// The last few commits, newest first, with short revs
    pub commits: Vec<DiffCommit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseBranch {
    pub name: String,
    pub ahead: u32,
    pub behind: u32,
}
