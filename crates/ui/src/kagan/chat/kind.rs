use std::path::Path;
use std::time::Duration;

use gpui_kit::assets::IconName;
use kg_domain::{Timestamp, WorkKind};

use crate::kagan::chat::part::{ToolRow, ToolStage};

/// How a work row draws its kind: the icon, the words the window uses, and the row's own title and
/// meta. A plugin's page reads the same kind, which is why the kind lives in `kg_domain`.
pub(super) trait WorkKindExt {
    fn icon(&self) -> IconName;
    fn hint(&self) -> &'static str;
    fn label(&self, stage: ToolStage) -> String;
    fn badge(&self) -> Option<String>;
    fn mono_title(&self) -> bool;
    fn title(&self, tool: &ToolRow) -> Option<String>;
    fn meta(&self, tool: &ToolRow) -> Option<String>;
    fn has_body(&self, tool: &ToolRow) -> bool;
}

impl WorkKindExt for WorkKind {
    fn icon(&self) -> IconName {
        match self {
            Self::Read => IconName::FileText,
            Self::Search => IconName::Search,
            Self::Execute => IconName::Terminal,
            Self::Edit => IconName::Pencil,
            Self::Delete => IconName::Trash,
            Self::Move => IconName::Move,
            Self::Fetch => IconName::Globe,
            Self::Think => IconName::Brain,
            Self::Mcp { .. } => IconName::Puzzle,
            Self::Skill { .. } => IconName::BookOpen,
            Self::Other => IconName::Wrench,
        }
    }

    fn hint(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Search => "search",
            Self::Execute => "command",
            Self::Edit => "edit",
            Self::Delete => "delete",
            Self::Move => "move",
            Self::Fetch => "fetch",
            Self::Think => "think",
            Self::Mcp { .. } => "mcp",
            Self::Skill { .. } => "skill",
            Self::Other => "tool",
        }
    }

    fn label(&self, stage: ToolStage) -> String {
        let word = match self {
            Self::Read => tell(stage, "Waiting to read file", "Reading file", "Read file"),
            Self::Search => tell(
                stage,
                "Waiting to search files",
                "Searching files",
                "Searched files",
            ),
            Self::Execute => tell(
                stage,
                "Waiting to run command",
                "Running command",
                "Ran command",
            ),
            Self::Edit => tell(stage, "Waiting to edit file", "Editing file", "Edited file"),
            Self::Delete => tell(
                stage,
                "Waiting to delete file",
                "Deleting file",
                "Deleted file",
            ),
            Self::Move => tell(stage, "Waiting to move file", "Moving file", "Moved file"),
            Self::Fetch => tell(
                stage,
                "Waiting to fetch page",
                "Fetching page",
                "Fetched page",
            ),
            Self::Think => tell(stage, "Waiting to think", "Thinking", "Thought"),
            Self::Mcp { server, tool } => {
                return match stage {
                    ToolStage::Waiting => format!("Waiting to call {server}.{tool}"),
                    ToolStage::Running => format!("Calling {server}.{tool}"),
                    ToolStage::Settled => format!("Called {server}.{tool}"),
                };
            }
            Self::Skill { .. } => tell(
                stage,
                "Waiting to load skill",
                "Loading skill",
                "Loaded skill",
            ),
            Self::Other => tell(
                stage,
                "Waiting to use a tool",
                "Using a tool",
                "Used a tool",
            ),
        };
        word.to_owned()
    }

    fn badge(&self) -> Option<String> {
        match self {
            Self::Mcp { server, .. } => Some(server.clone()),
            Self::Skill { .. } => Some("skill".into()),
            _ => None,
        }
    }

    fn mono_title(&self) -> bool {
        match self {
            Self::Read
            | Self::Search
            | Self::Execute
            | Self::Edit
            | Self::Delete
            | Self::Move
            | Self::Fetch
            | Self::Mcp { .. } => true,
            Self::Think | Self::Skill { .. } | Self::Other => false,
        }
    }

    /// The most useful short thing about the row: a file name, a command, a query, a URL.
    fn title(&self, tool: &ToolRow) -> Option<String> {
        let probe = |keys: &[&str]| {
            keys.iter().find_map(|key| {
                tool.input
                    .as_ref()
                    .and_then(|input| input.get(*key))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
        };
        let named = match self {
            Self::Read => probe(&["file_path", "path"]),
            Self::Search => probe(&["pattern", "query", "glob"]),
            Self::Execute => probe(&["command", "cmd"]),
            Self::Edit | Self::Delete | Self::Move => probe(&["file_path", "path"]).or_else(|| {
                tool.diff
                    .first()
                    .map(|diff| file_name(&diff.path).to_owned())
            }),
            Self::Fetch => probe(&["url"]),
            Self::Skill { name } => Some(name.clone()),
            Self::Mcp { .. } | Self::Think | Self::Other => None,
        };
        named
            .or_else(|| tool.title.clone())
            .map(|title| cut(&title, 120))
    }

    /// The outcome, or the time it took, right-aligned and muted.
    fn meta(&self, tool: &ToolRow) -> Option<String> {
        let span = tool.span().map(span_text);
        let outcome = match self {
            Self::Read => tool.output.as_ref().map(|output| {
                let lines = output.lines().count();
                format!("{lines} {}", plural(lines, "line", "lines"))
            }),
            Self::Execute => tool.output.as_deref().and_then(exit_word),
            Self::Edit | Self::Delete | Self::Move => {
                let (more, less) = tool.counts;
                (more > 0 || less > 0).then(|| format!("+{more} −{less}"))
            }
            Self::Mcp { .. }
            | Self::Skill { .. }
            | Self::Search
            | Self::Fetch
            | Self::Think
            | Self::Other => None,
        };
        match (span, outcome) {
            (Some(span), Some(outcome)) => Some(format!("{span} · {outcome}")),
            (span, outcome) => outcome.or(span),
        }
    }

    /// Whether the row has a body, and so a chevron.
    fn has_body(&self, tool: &ToolRow) -> bool {
        match self {
            Self::Edit | Self::Delete | Self::Move => !tool.diff.is_empty(),
            _ => tool.input.is_some() || tool.output.is_some(),
        }
    }
}

/// The kind of one row: the rule lives in `kg_domain`, and the row holds its four fields.
pub(super) fn of(tool: &ToolRow) -> WorkKind {
    WorkKind::of(
        tool.name.as_deref(),
        tool.title.as_deref(),
        tool.kind,
        tool.input.as_ref(),
    )
}

/// What an approval card calls the tool it asks about.
pub(super) fn title_of(tool: &ToolRow) -> Option<String> {
    of(tool).title(tool)
}

pub(super) fn pick(done: bool, done_word: &'static str, now_word: &'static str) -> &'static str {
    if done { done_word } else { now_word }
}

/// The word a row uses at its stage: before it starts, while it moves, and once it settles.
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

/// Cuts text at a token boundary, so a title never ends mid-word.
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

/// "6 s", or "1 min 12 s" once it is worth the word.
pub(super) fn span_text(span: Duration) -> String {
    let secs = span.as_secs().max(1);
    if secs >= 60 {
        format!("{} min {} s", secs / 60, secs % 60)
    } else {
        format!("{secs} s")
    }
}

/// The exit code a command's output ends with, when its last line names one.
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

/// How long a row took, from the two event times.
pub(super) fn span_of(from: Timestamp, to: Timestamp) -> Duration {
    Duration::from_millis(u64::try_from(to.0.saturating_sub(from.0).max(0)).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kg_domain::{Timestamp, ToolKind, ToolStatus};

    fn tool(name: Option<&str>, kind: ToolKind, title: &str, input: serde_json::Value) -> ToolRow {
        ToolRow {
            id: "t1".into(),
            name: name.map(str::to_owned),
            kind: Some(kind),
            title: Some(title.into()),
            status: Some(ToolStatus::Completed),
            input: Some(input),
            output: None,
            diff: Vec::new(),
            counts: (0, 0),
            locations: Vec::new(),
            approval: None,
            from: Timestamp(0),
            to: Timestamp(1000),
        }
    }

    #[test]
    fn an_mcp_name_reads_as_its_server_and_tool() {
        assert_eq!(
            of(&tool(
                Some("mcp__github__create_issue"),
                ToolKind::Other,
                "Create issue",
                serde_json::json!({}),
            )),
            WorkKind::Mcp {
                server: "github".into(),
                tool: "create_issue".into(),
            }
        );
    }

    #[test]
    fn kagans_own_server_is_recognised_by_its_name() {
        assert_eq!(
            of(&tool(
                Some("kagan_open_pull_request"),
                ToolKind::Other,
                "Open PR",
                serde_json::json!({}),
            )),
            WorkKind::Mcp {
                server: "kagan".into(),
                tool: "open_pull_request".into(),
            }
        );
    }

    #[test]
    fn a_skill_load_is_named_by_its_input() {
        let kind = of(&tool(
            Some("skill_load"),
            ToolKind::Other,
            "Load a skill",
            serde_json::json!({"name": "rust-review"}),
        ));
        assert_eq!(
            kind,
            WorkKind::Skill {
                name: "rust-review".into()
            }
        );
        let title_only = of(&tool(
            None,
            ToolKind::Other,
            "load skill playwright-debugging",
            serde_json::json!({"name": "playwright-debugging"}),
        ));
        assert_eq!(
            title_only,
            WorkKind::Skill {
                name: "playwright-debugging".into()
            }
        );
    }

    #[test]
    fn an_unknown_agent_tool_keeps_its_kind_and_title() {
        let kind = of(&tool(
            Some("some_agent_tool"),
            ToolKind::Read,
            "Read file",
            serde_json::json!({"path": "a.ts"}),
        ));
        assert_eq!(kind, WorkKind::Read);
        assert_eq!(
            kind.title(&tool(
                Some("some_agent_tool"),
                ToolKind::Read,
                "Read file",
                serde_json::json!({"path": "a.ts"}),
            )),
            Some("a.ts".into())
        );
    }

    #[test]
    fn a_row_waits_until_the_agent_starts_it() {
        assert_eq!(
            WorkKind::Execute.label(ToolStage::Waiting),
            "Waiting to run command"
        );
        assert_eq!(
            WorkKind::Execute.label(ToolStage::Running),
            "Running command"
        );
        assert_eq!(WorkKind::Execute.label(ToolStage::Settled), "Ran command");
    }

    #[test]
    fn a_title_is_cut_at_a_word() {
        assert_eq!(cut("one two three", 60), "one two three");
        assert_eq!(cut("one two three four", 9), "one two…");
    }

    #[test]
    fn a_span_reads_in_seconds_then_minutes() {
        assert_eq!(span_text(Duration::from_millis(300)), "1 s");
        assert_eq!(span_text(Duration::from_secs(18)), "18 s");
        assert_eq!(span_text(Duration::from_secs(72)), "1 min 12 s");
    }
}
