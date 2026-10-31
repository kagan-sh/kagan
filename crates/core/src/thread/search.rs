use gpui_kit::{App, AppContext as _, Entity, Task};
use kg_domain::{Found, OpResult, SEARCH_DEFAULT, Snippet, Visibility};
use kg_terminal::Lines;

use crate::Workspace;
use crate::database::Result;
use crate::workspace::closed;

use super::Shown;

const BEFORE: usize = 40;
const WIDTH: usize = 120;

impl Shown {
    fn lines(&self) -> Lines {
        match self {
            Self::Running(terminal) => terminal.lines(),
            Self::Ended(finished) => finished.lines(),
        }
    }
}

// Matches one thread at a time on the background executor, so dropping the task stops the search
// at the next thread.
pub(crate) fn search(
    workspace: &Entity<Workspace>,
    text: &str,
    limit: Option<u32>,
    archived: bool,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let database = workspace.read(cx).database.clone();
    let this = workspace.downgrade();
    let needle = fold(text.trim());
    let limit = usize::try_from(limit.unwrap_or(SEARCH_DEFAULT)).unwrap_or(usize::MAX);
    cx.spawn(async move |cx| {
        let (threads, projects) = database
            .call(move |store| {
                let threads = store.threads(None, (!archived).then_some(false), None, None)?;
                Ok((threads, store.projects()?))
            })
            .await?;
        let (live, retired): (Vec<_>, Vec<_>) = threads
            .into_iter()
            .filter(|thread| thread.visibility == Visibility::Visible)
            .partition(|thread| thread.archived_at.is_none());
        let threads: Vec<_> = this
            .update(cx, |workspace, _| {
                live.into_iter()
                    .chain(retired)
                    .map(|thread| {
                        let lines = workspace.terminals.get(&thread.id).map(Shown::lines);
                        (thread, lines)
                    })
                    .collect()
            })
            .map_err(closed)?;
        let mut found = Vec::new();
        for (thread, lines) in threads {
            if found.len() == limit {
                break;
            }
            let snippet = match lines {
                Some(lines) => {
                    let needle = needle.clone();
                    cx.background_spawn(async move { find(&lines, &needle) })
                        .await
                }
                None => None,
            };
            if snippet.is_none() && !fold(&thread.title).contains(&needle) {
                continue;
            }
            let project = projects
                .iter()
                .find(|project| project.id == thread.project_id)
                .map(|project| project.name.clone())
                .unwrap_or_default();
            found.push(Found {
                archived: thread.archived_at.is_some(),
                thread_id: thread.id,
                title: thread.title,
                project_id: thread.project_id,
                project,
                snippet,
            });
        }
        Ok(OpResult::Found(found))
    })
}

fn find(lines: &Lines, needle: &str) -> Option<Snippet> {
    let kept = lines.kept();
    let screen = kept.on_screen;
    let history = kept.rows.saturating_sub(screen);
    let line = kept
        .lines
        .iter()
        .rev()
        .find(|line| fold(&line.text).contains(needle))?;
    // A line above the screen is scrolled to the middle of the view; one on the screen stays put.
    let scroll = if line.row >= history {
        0
    } else {
        (history + screen / 2 - line.row).min(history)
    };
    Some(Snippet {
        text: around(&line.text, needle),
        scroll: u32::try_from(scroll).unwrap_or(u32::MAX),
    })
}

// Lowercasing can change a character's length, so the match is found by character, not by byte.
fn around(row: &str, needle: &str) -> String {
    let chars: Vec<(usize, char)> = row.char_indices().collect();
    let at = chars
        .iter()
        .position(|&(byte, _)| starts_with(&row[byte..], needle))
        .unwrap_or(0);
    let from = at.saturating_sub(BEFORE);
    let to = (from + WIDTH).min(chars.len());
    let start = chars.get(from).map_or(row.len(), |&(byte, _)| byte);
    let end = chars.get(to).map_or(row.len(), |&(byte, _)| byte);
    let text = row[start..end].trim();
    let before = if from > 0 { "…" } else { "" };
    let after = if to < chars.len() { "…" } else { "" };
    format!("{before}{text}{after}")
}

// Folds one character at a time, as `starts_with` does, so a match and its place agree.
fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

fn starts_with(text: &str, needle: &str) -> bool {
    let mut lower = text.chars().flat_map(char::to_lowercase);
    needle.chars().all(|wanted| lower.next() == Some(wanted))
}
