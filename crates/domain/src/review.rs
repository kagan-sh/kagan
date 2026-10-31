use std::ops::Range;

use crate::{DiffFileView, DiffLine, DiffLineKind};

/// A patch row or a run of unchanged lines between hunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewRow {
    Line {
        line: DiffLine,
        changed: Option<Range<usize>>,
    },
    Fold {
        lines: Vec<DiffLine>,
    },
}

/// Builds review rows from a patch and the new side of the file.
#[must_use]
pub fn review_rows(view: &DiffFileView) -> Vec<ReviewRow> {
    let file: Vec<&str> = view.file.as_deref().unwrap_or("").lines().collect();
    let mut rows = Vec::new();
    let mut next_new = 1u32;
    let mut next_old = 1u32;
    for hunk in &view.hunks {
        let count = hunk.new_start.saturating_sub(next_new) as usize;
        let start = next_new.saturating_sub(1) as usize;
        let end = start.saturating_add(count).min(file.len());
        if start < end {
            let lines = file[start..end]
                .iter()
                .enumerate()
                .map(|(offset, text)| {
                    let offset = u32::try_from(offset).unwrap_or(u32::MAX);
                    DiffLine {
                        kind: DiffLineKind::Context,
                        old: Some(next_old.saturating_add(offset)),
                        new: Some(next_new.saturating_add(offset)),
                        text: (*text).to_owned(),
                    }
                })
                .collect();
            rows.push(ReviewRow::Fold { lines });
        }
        next_old = hunk.old_start.max(1);
        next_new = hunk.new_start.max(1);
        let mut index = 0;
        while index < hunk.lines.len() {
            let start = index;
            while index < hunk.lines.len() && hunk.lines[index].kind == DiffLineKind::Remove {
                index += 1;
            }
            let middle = index;
            while index < hunk.lines.len() && hunk.lines[index].kind == DiffLineKind::Add {
                index += 1;
            }
            if start == index {
                rows.push(ReviewRow::Line {
                    line: hunk.lines[index].clone(),
                    changed: None,
                });
                index += 1;
            } else {
                let removed = &hunk.lines[start..middle];
                let added = &hunk.lines[middle..index];
                for (offset, line) in removed.iter().enumerate() {
                    let changed = added
                        .get(offset)
                        .and_then(|other| similar_change(&line.text, &other.text))
                        .map(|(old, _)| old);
                    rows.push(ReviewRow::Line {
                        line: line.clone(),
                        changed,
                    });
                }
                for (offset, line) in added.iter().enumerate() {
                    let changed = removed
                        .get(offset)
                        .and_then(|other| similar_change(&other.text, &line.text))
                        .map(|(_, new)| new);
                    rows.push(ReviewRow::Line {
                        line: line.clone(),
                        changed,
                    });
                }
            }
        }
        for line in &hunk.lines {
            if let Some(old) = line.old {
                next_old = old.saturating_add(1);
            }
            if let Some(new) = line.new {
                next_new = new.saturating_add(1);
            }
        }
    }
    let start = next_new.saturating_sub(1) as usize;
    if start < file.len() {
        let lines = file[start..]
            .iter()
            .enumerate()
            .map(|(offset, text)| {
                let offset = u32::try_from(offset).unwrap_or(u32::MAX);
                DiffLine {
                    kind: DiffLineKind::Context,
                    old: Some(next_old.saturating_add(offset)),
                    new: Some(next_new.saturating_add(offset)),
                    text: (*text).to_owned(),
                }
            })
            .collect();
        rows.push(ReviewRow::Fold { lines });
    }
    rows
}

fn similar_change(old: &str, new: &str) -> Option<(Range<usize>, Range<usize>)> {
    let (old_range, new_range) = word_change(old, new);
    let kept = old[..old_range.start].chars().count() + old[old_range.end..].chars().count();
    (kept * 2 >= old.chars().count().max(new.chars().count())).then_some((old_range, new_range))
}

fn word_change(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let mut prefix = 0;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        prefix += a.len_utf8();
    }
    while prefix > 0
        && old[prefix..].chars().next().is_some_and(is_word)
        && new[prefix..].chars().next().is_some_and(is_word)
        && old[..prefix].chars().next_back().is_some_and(is_word)
    {
        prefix -= old[..prefix].chars().next_back().map_or(0, char::len_utf8);
    }
    let mut suffix = 0;
    for (a, b) in old[prefix..].chars().rev().zip(new[prefix..].chars().rev()) {
        if a != b {
            break;
        }
        suffix += a.len_utf8();
    }
    while suffix > 0
        && old[..old.len() - suffix]
            .chars()
            .next_back()
            .is_some_and(is_word)
        && new[..new.len() - suffix]
            .chars()
            .next_back()
            .is_some_and(is_word)
        && old[old.len() - suffix..]
            .chars()
            .next()
            .is_some_and(is_word)
    {
        suffix -= old[old.len() - suffix..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
    }
    (prefix..old.len() - suffix, prefix..new.len() - suffix)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
