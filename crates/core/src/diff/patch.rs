use std::path::Path;

use kg_domain::{
    DiffCommit, DiffFile, DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffList, DiffStatus,
};

use crate::host::database::Result;

use super::git::{diff, git, git_ok, text};
use super::{FILE_LIMIT, PATCH_LIMIT};

pub(super) fn commits(root: &Path) -> Result<Vec<DiffCommit>> {
    if !crate::host::git::is_repository(root) {
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
pub(super) fn diff_list(
    root: &Path,
    left: &str,
    right: &str,
    base: Option<String>,
) -> Result<DiffList> {
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

pub(super) fn diff_file(
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
            binary: true,
            added,
            removed,
            ..DiffFileView::empty(path)
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
        binary,
        truncated,
        too_large,
        hunks: if binary { Vec::new() } else { hunks },
        file: if binary { None } else { file },
        added,
        removed,
        ..DiffFileView::empty(path)
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::parse_numstat;

    #[derive(Clone, Debug)]
    enum Record {
        Text {
            added: u16,
            removed: u16,
            path: String,
        },
        Binary {
            path: String,
        },
        Rename {
            added: u16,
            removed: u16,
            old: String,
            new: String,
        },
    }

    fn path() -> impl Strategy<Value = String> {
        "[A-Za-z0-9._/-]{1,20}"
    }

    fn record() -> impl Strategy<Value = Record> {
        prop_oneof![
            (0u16..1_000, 0u16..1_000, path()).prop_map(|(added, removed, path)| Record::Text {
                added,
                removed,
                path,
            }),
            path().prop_map(|path| Record::Binary { path }),
            (0u16..1_000, 0u16..1_000, path(), path()).prop_map(|(added, removed, old, new)| {
                Record::Rename {
                    added,
                    removed,
                    old,
                    new,
                }
            }),
        ]
    }

    fn render(record: &Record) -> (Vec<u8>, (String, u32, u32, bool)) {
        match record {
            Record::Text {
                added,
                removed,
                path,
            } => (
                format!("{added}\t{removed}\t{path}\0").into_bytes(),
                (path.clone(), u32::from(*added), u32::from(*removed), false),
            ),
            Record::Binary { path } => (
                format!("-\t-\t{path}\0").into_bytes(),
                (path.clone(), 0, 0, true),
            ),
            Record::Rename {
                added,
                removed,
                old,
                new,
            } => (
                format!("{added}\t{removed}\t\0{old}\0{new}\0").into_bytes(),
                (new.clone(), u32::from(*added), u32::from(*removed), false),
            ),
        }
    }

    #[test]
    fn read_side_keeps_a_file_at_the_limit_and_refuses_one_over() {
        let limit = usize::try_from(super::FILE_LIMIT).expect("file limit fits in usize");
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .current_dir(root)
                .args(args)
                .status()
                .expect("git");
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("at.txt"), vec![b'a'; limit]).expect("write at");
        std::fs::write(root.join("over.txt"), vec![b'a'; limit + 1]).expect("write over");
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "files",
        ]);

        let (body, too_big, binary) = super::read_side(root, "HEAD", "at.txt").expect("at");
        assert_eq!(body.map(|body| body.len()), Some(limit));
        assert!(!too_big);
        assert!(!binary);

        let (body, too_big, binary) = super::read_side(root, "HEAD", "over.txt").expect("over");
        assert_eq!(body, None);
        assert!(too_big);
        assert!(!binary);
    }

    proptest! {
        #[test]
        fn numstat_round_trips(records in prop::collection::vec(record(), 0..8)) {
            let mut bytes = Vec::new();
            let mut expected = Vec::new();
            for record in &records {
                let (rendered, stat) = render(record);
                bytes.extend(rendered);
                expected.push(stat);
            }

            let actual = parse_numstat(&bytes)
                .into_iter()
                .map(|stat| (stat.path, stat.added, stat.removed, stat.binary))
                .collect::<Vec<_>>();
            prop_assert_eq!(actual, expected);
        }

        #[test]
        fn arbitrary_bytes_do_not_panic(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
            let _ = parse_numstat(&bytes);
        }
    }
}
