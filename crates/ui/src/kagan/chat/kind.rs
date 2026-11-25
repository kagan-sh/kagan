use std::path::Path;
use std::time::Duration;

use gpui_kit::assets::IconName;
use kg_domain::{Timestamp, WorkKind};

use crate::kagan::chat::part::{ToolRow, ToolStage};

pub(super) fn icon(kind: &WorkKind) -> IconName {
    match kind {
        WorkKind::Read => IconName::FileText,
        WorkKind::Search => IconName::Search,
        WorkKind::Execute => IconName::Terminal,
        WorkKind::Edit => IconName::Pencil,
        WorkKind::Delete => IconName::Trash,
        WorkKind::Move => IconName::Move,
        WorkKind::Fetch => IconName::Globe,
        WorkKind::Think => IconName::Brain,
        WorkKind::Mcp { .. } => IconName::Puzzle,
        WorkKind::Skill { .. } => IconName::BookOpen,
        WorkKind::Other => IconName::Wrench,
    }
}

pub(super) fn hint(kind: &WorkKind) -> &'static str {
    match kind {
        WorkKind::Read => "read",
        WorkKind::Search => "search",
        WorkKind::Execute => "command",
        WorkKind::Edit => "edit",
        WorkKind::Delete => "delete",
        WorkKind::Move => "move",
        WorkKind::Fetch => "fetch",
        WorkKind::Think => "think",
        WorkKind::Mcp { .. } => "mcp",
        WorkKind::Skill { .. } => "skill",
        WorkKind::Other => "tool",
    }
}

pub(super) fn label(kind: &WorkKind, stage: ToolStage) -> String {
    let word = match kind {
        WorkKind::Read => tell(stage, "Waiting to read file", "Reading file", "Read file"),
        WorkKind::Search => tell(
            stage,
            "Waiting to search files",
            "Searching files",
            "Searched files",
        ),
        WorkKind::Execute => tell(
            stage,
            "Waiting to run command",
            "Running command",
            "Ran command",
        ),
        WorkKind::Edit => tell(stage, "Waiting to edit file", "Editing file", "Edited file"),
        WorkKind::Delete => tell(
            stage,
            "Waiting to delete file",
            "Deleting file",
            "Deleted file",
        ),
        WorkKind::Move => tell(stage, "Waiting to move file", "Moving file", "Moved file"),
        WorkKind::Fetch => tell(
            stage,
            "Waiting to fetch page",
            "Fetching page",
            "Fetched page",
        ),
        WorkKind::Think => tell(stage, "Waiting to think", "Thinking", "Thought"),
        WorkKind::Mcp { server, tool } => {
            return match stage {
                ToolStage::Waiting => format!("Waiting to call {server}.{tool}"),
                ToolStage::Running => format!("Calling {server}.{tool}"),
                ToolStage::Settled => format!("Called {server}.{tool}"),
            };
        }
        WorkKind::Skill { .. } => tell(
            stage,
            "Waiting to load skill",
            "Loading skill",
            "Loaded skill",
        ),
        WorkKind::Other => tell(
            stage,
            "Waiting to use a tool",
            "Using a tool",
            "Used a tool",
        ),
    };
    word.to_owned()
}

pub(super) fn badge(kind: &WorkKind) -> Option<String> {
    match kind {
        WorkKind::Mcp { server, .. } => Some(server.clone()),
        WorkKind::Skill { .. } => Some("skill".into()),
        _ => None,
    }
}

pub(super) fn mono_title(kind: &WorkKind) -> bool {
    match kind {
        WorkKind::Read
        | WorkKind::Search
        | WorkKind::Execute
        | WorkKind::Edit
        | WorkKind::Delete
        | WorkKind::Move
        | WorkKind::Fetch
        | WorkKind::Mcp { .. } => true,
        WorkKind::Think | WorkKind::Skill { .. } | WorkKind::Other => false,
    }
}

pub(super) fn title(kind: &WorkKind, tool: &ToolRow) -> Option<String> {
    let probe = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            tool.input
                .as_ref()
                .and_then(|input| input.get(*key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
    };
    let named = match kind {
        WorkKind::Read => probe(&["file_path", "path"]),
        WorkKind::Search => probe(&["pattern", "query", "glob"]),
        WorkKind::Execute => probe(&["command", "cmd"]),
        WorkKind::Edit | WorkKind::Delete | WorkKind::Move => probe(&["file_path", "path"])
            .or_else(|| {
                tool.diff
                    .first()
                    .map(|diff| file_name(&diff.path).to_owned())
            }),
        WorkKind::Fetch => probe(&["url"]),
        WorkKind::Skill { name } => Some(name.clone()),
        WorkKind::Mcp { .. } | WorkKind::Think | WorkKind::Other => None,
    };
    named
        .or_else(|| tool.title.clone())
        .map(|title| cut(&title, 120))
}

pub(super) fn meta(kind: &WorkKind, tool: &ToolRow) -> Option<String> {
    let span = tool.span().map(span_text);
    let outcome = match kind {
        WorkKind::Read => tool.output.as_ref().map(|output| {
            let lines = output.lines().count();
            format!("{lines} {}", plural(lines, "line", "lines"))
        }),
        WorkKind::Execute => tool.output.as_deref().and_then(exit_word),
        WorkKind::Edit | WorkKind::Delete | WorkKind::Move => {
            let (more, less) = tool.counts;
            (more > 0 || less > 0).then(|| format!("+{more} −{less}"))
        }
        WorkKind::Mcp { .. }
        | WorkKind::Skill { .. }
        | WorkKind::Search
        | WorkKind::Fetch
        | WorkKind::Think
        | WorkKind::Other => None,
    };
    match (span, outcome) {
        (Some(span), Some(outcome)) => Some(format!("{span} · {outcome}")),
        (span, outcome) => outcome.or(span),
    }
}

pub(super) fn has_body(kind: &WorkKind, tool: &ToolRow) -> bool {
    match kind {
        WorkKind::Edit | WorkKind::Delete | WorkKind::Move => !tool.diff.is_empty(),
        _ => tool.input.is_some() || tool.output.is_some(),
    }
}

pub(super) fn of(tool: &ToolRow) -> WorkKind {
    WorkKind::of(
        tool.name.as_deref(),
        tool.title.as_deref(),
        tool.kind,
        tool.input.as_ref(),
    )
}

pub(super) fn title_of(tool: &ToolRow) -> Option<String> {
    title(&of(tool), tool)
}

pub(super) fn pick(done: bool, done_word: &'static str, now_word: &'static str) -> &'static str {
    if done { done_word } else { now_word }
}

pub(super) fn tell(
    stage: ToolStage,
    waiting: &'static str,
    now_word: &'static str,
    done_word: &'static str,
) -> &'static str {
    match stage {
        ToolStage::Waiting => waiting,
        ToolStage::Running => now_word,
        ToolStage::Settled => done_word,
    }
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut end = 0;
    for (index, character) in text.char_indices() {
        if index > max {
            break;
        }
        if character.is_whitespace() {
            end = index;
        }
    }
    if end == 0 {
        return text.chars().take(max).collect();
    }
    format!("{}…", text[..end].trim_end())
}

pub(super) fn span_text(span: Duration) -> String {
    let secs = span.as_secs().max(1);
    if secs >= 60 {
        format!("{} min {} s", secs / 60, secs % 60)
    } else {
        format!("{secs} s")
    }
}

fn exit_word(output: &str) -> Option<String> {
    let line = output.lines().rev().find(|line| !line.trim().is_empty())?;
    let mut words = line.split_whitespace();
    while let Some(word) = words.next() {
        let names_code = word.eq_ignore_ascii_case("exit") || word.eq_ignore_ascii_case("code");
        if names_code
            && let Some(code) = words.next().and_then(|word| {
                word.trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<u32>()
                    .ok()
            })
        {
            return Some(format!("exit {code}"));
        }
    }
    None
}

pub(super) fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

pub(super) fn span_of(from: Timestamp, to: Timestamp) -> Duration {
    Duration::from_millis(u64::try_from(to.0.saturating_sub(from.0).max(0)).unwrap_or(0))
}

#[cfg(test)]
#[path = "kind_test.rs"]
mod tests;
