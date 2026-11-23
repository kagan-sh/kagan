use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::{PlanStatus, Timestamp, ToolDiff, ToolKind};

use crate::kagan::chat::ChatView;
use crate::kagan::chat::body;
use crate::kagan::chat::part::{Part, Row, RowKey, RowKind, ThoughtRow, ToolRow};
use crate::theme::{Tone, TypeScale as _, row_height};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// What a tool row is about, once kagan has read the agent's names and input. Presentation only:
/// nothing here decides whether the row draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WorkKind {
    Read,
    Search,
    Execute,
    Edit,
    Delete,
    Move,
    Fetch,
    Think,
    Mcp { server: String, tool: String },
    Skill { name: String },
    Other { name: Option<String> },
}

impl WorkKind {
    pub(super) fn of(tool: &ToolRow) -> Self {
        if let Some(rest) = tool
            .name
            .as_deref()
            .and_then(|name| name.strip_prefix("mcp__"))
            && let Some((server, called)) = rest.split_once("__")
        {
            return Self::Mcp {
                server: server.to_owned(),
                tool: called.to_owned(),
            };
        }
        // kagan writes its own MCP server as `kagan_<tool>`; any other agent's server is only
        // recognisable when it spells the name `mcp__<server>__<tool>`.
        if let Some(name) = tool.name.as_deref()
            && let Some((server, called)) = name.split_once('_')
            && server == "kagan"
        {
            return Self::Mcp {
                server: server.to_owned(),
                tool: called.to_owned(),
            };
        }
        if tool.name.as_deref() == Some("skill_load")
            || tool
                .title
                .as_deref()
                .is_some_and(|title| title.starts_with("load skill"))
                && tool.input.as_ref().is_some_and(|input| {
                    input.get("name").is_some_and(serde_json::Value::is_string)
                })
        {
            let name = tool
                .input
                .as_ref()
                .and_then(|input| input.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    tool.title
                        .as_deref()
                        .and_then(|title| title.strip_prefix("load skill "))
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "a skill".into());
            return Self::Skill { name };
        }
        match tool.kind {
            Some(ToolKind::Read) => Self::Read,
            Some(ToolKind::Edit) => Self::Edit,
            Some(ToolKind::Delete) => Self::Delete,
            Some(ToolKind::Move) => Self::Move,
            Some(ToolKind::Search) => Self::Search,
            Some(ToolKind::Execute) => Self::Execute,
            Some(ToolKind::Think) => Self::Think,
            Some(ToolKind::Fetch) => Self::Fetch,
            _ => Self::Other {
                name: tool.name.clone(),
            },
        }
    }

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
            Self::Other { .. } => IconName::Wrench,
        }
    }

    fn word(&self) -> &'static str {
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
            Self::Other { .. } => "tool",
        }
    }

    fn label(&self, done: bool) -> String {
        match self {
            Self::Read => pick(done, "Read file", "Reading file"),
            Self::Search => pick(done, "Searched files", "Searching files"),
            Self::Execute => pick(done, "Ran command", "Running command"),
            Self::Edit => pick(done, "Edited file", "Editing file"),
            Self::Delete => pick(done, "Deleted file", "Deleting file"),
            Self::Move => pick(done, "Moved file", "Moving file"),
            Self::Fetch => pick(done, "Fetched page", "Fetching page"),
            Self::Think => pick(done, "Thought", "Thinking"),
            Self::Mcp { .. } => pick(done, "Called", "Calling"),
            Self::Skill { .. } => pick(done, "Loaded skill", "Loading skill"),
            Self::Other { .. } => pick(done, "Used a tool", "Using a tool"),
        }
        .to_owned()
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
            Self::Think | Self::Skill { .. } | Self::Other { .. } => false,
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
            Self::Mcp { tool: called, .. } => Some(called.clone()),
            Self::Skill { name } => Some(name.clone()),
            Self::Think | Self::Other { .. } => None,
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
                let (more, less) = tool
                    .diff
                    .iter()
                    .fold((0, 0), |counts, diff| add(counts, diff_counts(diff)));
                (more > 0 || less > 0).then(|| format!("+{more} −{less}"))
            }
            Self::Mcp { .. }
            | Self::Skill { .. }
            | Self::Search
            | Self::Fetch
            | Self::Think
            | Self::Other { .. } => None,
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

fn pick(done: bool, done_word: &'static str, now_word: &'static str) -> &'static str {
    if done { done_word } else { now_word }
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

/// The lines a diff adds and removes.
fn diff_counts(diff: &ToolDiff) -> (usize, usize) {
    let old = diff.old.as_deref().unwrap_or_default();
    similar::TextDiff::from_lines(old, &diff.new)
        .iter_all_changes()
        .fold((0, 0), |(more, less), change| match change.tag() {
            similar::ChangeTag::Insert => (more + 1, less),
            similar::ChangeTag::Delete => (more, less + 1),
            similar::ChangeTag::Equal => (more, less),
        })
}

fn add((more, less): (usize, usize), (delta_more, delta_less): (usize, usize)) -> (usize, usize) {
    (more + delta_more, less + delta_less)
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

pub(super) fn row_id(part: Part, id: u64) -> gpui_kit::ElementId {
    named(part, id)
}

/// The element id a part's row is found by: `chat-<part>-<id>`, stable across appends.
pub(super) fn named(part: Part, id: impl std::fmt::Display) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("chat-{}-{id}", part.name()).into())
}

/// What an approval card calls the tool it asks about.
pub(super) fn title_of(tool: &ToolRow) -> Option<String> {
    WorkKind::of(tool).title(tool)
}

/// One work row: `[chevron] [icon] label · title ……… meta`.
pub(super) fn work_row(
    row: &Row,
    tool: &ToolRow,
    indented: bool,
    open: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let kind = WorkKind::of(tool);
    let done = !tool.running();
    let body = open
        .then(|| body::work_body(row.id, tool, &kind, weak, window, cx))
        .map(IntoElement::into_any_element);
    face(
        Face {
            id: row_id(Part::Work, row.id),
            key: row.key(),
            icon: kind.icon(),
            label: kind.label(done),
            title: kind.title(tool),
            title_mono: kind.mono_title(),
            badge: kind.badge(),
            meta: kind.meta(tool),
            icons: Vec::new(),
            word: kind.word(),
            open,
            has_body: kind.has_body(tool),
            tone: if tool.failed() {
                Tone::Failed
            } else if tool.running() {
                Tone::Busy
            } else {
                Tone::Done
            },
            indent: if indented { px(24.) } else { px(4.) },
            body,
        },
        weak,
        window,
        cx,
    )
}

/// A thought row: the reasoning, one row, streamed into its body.
pub(super) fn thought_row(
    row: &Row,
    thought: &ThoughtRow,
    live: bool,
    open: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let label = if live {
        "Thinking…".to_owned()
    } else {
        let span = thought.to.0.saturating_sub(thought.from.0);
        format!(
            "Thought for {}",
            span_text(Duration::from_millis(
                u64::try_from(span.max(0)).unwrap_or(0)
            ))
        )
    };
    let body = open
        .then(|| body::thought_body(row.id, &thought.text, cx))
        .map(IntoElement::into_any_element);
    face(
        Face {
            id: row_id(Part::Thought, row.id),
            key: row.key(),
            icon: IconName::Brain,
            label,
            title: None,
            title_mono: false,
            badge: None,
            meta: None,
            icons: Vec::new(),
            word: "thought",
            open,
            has_body: !thought.text.is_empty(),
            tone: if live { Tone::Busy } else { Tone::Done },
            indent: px(4.),
            body,
        },
        weak,
        window,
        cx,
    )
}

/// The bundle of finished looking work: the kinds' icons, a sentence, and the rows inside.
pub(super) fn bundle_row(
    row: &Row,
    members: &[Row],
    open: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (files, searches, pages) = members.iter().fold((0, 0, 0), |counts, member| {
        let RowKind::Work(tool) = &member.kind else {
            return counts;
        };
        match WorkKind::of(tool) {
            WorkKind::Read => (counts.0 + 1, counts.1, counts.2),
            WorkKind::Search => (counts.0, counts.1 + 1, counts.2),
            WorkKind::Fetch => (counts.0, counts.1, counts.2 + 1),
            _ => counts,
        }
    });
    let mut words = Vec::new();
    if files > 0 {
        words.push(format!(
            "Explored {files} {}",
            plural(files, "file", "files")
        ));
    }
    if searches > 0 {
        words.push(format!(
            "searched {searches} {}",
            plural(searches, "time", "times")
        ));
    }
    if pages > 0 {
        words.push(format!(
            "fetched {pages} {}",
            plural(pages, "page", "pages")
        ));
    }
    let span = members
        .iter()
        .filter_map(|member| match &member.kind {
            RowKind::Work(tool) => Some((tool.from, tool.to)),
            _ => None,
        })
        .fold(None::<(Timestamp, Timestamp)>, |seen, (from, to)| {
            Some(match seen {
                Some((start, end)) => (start.min(from), end.max(to)),
                None => (from, to),
            })
        })
        .map(|(from, to)| span_text(span_of(from, to)));
    let icons: Vec<AnyElement> = members
        .iter()
        .filter_map(|member| match &member.kind {
            RowKind::Work(tool) => Some(
                Icon::new(WorkKind::of(tool).icon())
                    .xsmall()
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element(),
            ),
            _ => None,
        })
        .collect();
    let inner = open.then(|| {
        let children: Vec<AnyElement> = members
            .iter()
            .filter_map(|member| match &member.kind {
                RowKind::Work(tool) => {
                    let open = weak
                        .read_with(cx, |this, _| this.member_open(member.key()))
                        .unwrap_or(false);
                    Some(work_row(member, tool, true, open, weak, window, cx))
                }
                _ => None,
            })
            .collect();
        div()
            .ml(px(22.))
            .pl(px(10.))
            .border_l_1()
            .border_color(cx.theme().border)
            .children(children)
            .into_any_element()
    });
    face(
        Face {
            id: row_id(Part::Bundle, row.id),
            key: row.key(),
            icon: IconName::Layers,
            label: words.join(", "),
            title: None,
            title_mono: false,
            badge: None,
            meta: span,
            icons,
            word: "bundle",
            open,
            has_body: !members.is_empty(),
            tone: Tone::Done,
            indent: px(4.),
            body: inner,
        },
        weak,
        window,
        cx,
    )
}

/// The plan: its steps and how far along they are.
pub(super) fn plan_row(
    row: &Row,
    entries: &[kg_domain::PlanEntry],
    open: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let done = entries
        .iter()
        .filter(|entry| entry.status == PlanStatus::Completed)
        .count();
    let body = open
        .then(|| body::plan_body(row.id, entries, cx))
        .map(IntoElement::into_any_element);
    face(
        Face {
            id: row_id(Part::Plan, row.id),
            key: row.key(),
            icon: IconName::ListTodo,
            label: "Plan".into(),
            title: None,
            title_mono: false,
            badge: None,
            meta: Some(format!("{done} of {} done", entries.len())),
            icons: Vec::new(),
            word: "plan",
            open,
            has_body: !entries.is_empty(),
            tone: Tone::Done,
            indent: px(4.),
            body,
        },
        weak,
        window,
        cx,
    )
}

fn span_of(from: Timestamp, to: Timestamp) -> Duration {
    Duration::from_millis(u64::try_from(to.0.saturating_sub(from.0).max(0)).unwrap_or(0))
}

struct Face {
    id: gpui_kit::ElementId,
    key: RowKey,
    icon: IconName,
    label: String,
    title: Option<String>,
    title_mono: bool,
    badge: Option<String>,
    meta: Option<String>,
    icons: Vec<AnyElement>,
    word: &'static str,
    open: bool,
    has_body: bool,
    tone: Tone,
    indent: gpui_kit::Pixels,
    body: Option<AnyElement>,
}

/// The one line every expandable row shares: chevron, icon, label, title, badge, meta, and the
/// body under it when it is open.
fn face(
    mut face: Face,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let focus = crate::kagan::tab_stop(window, cx, face.id.clone());
    let focused = focus.is_focused(window);
    let toggle: Rc<dyn Fn(&mut App)> = {
        let weak = weak.clone();
        let key = face.key;
        Rc::new(move |cx| {
            _ = weak.update(cx, |this, cx| this.toggle(key, cx));
        })
    };
    let (on_key, on_click) = (toggle.clone(), toggle);
    let body = face.body.take();
    let line = line(face, &focus, focused, on_key, on_click, cx);
    div()
        .child(line)
        .when_some(body, |row, body| {
            row.child(
                div()
                    .ml(px(22.))
                    .pl(px(10.))
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .child(body),
            )
        })
        .into_any_element()
}

/// The row's one line: what a reader sees before expanding it.
fn line(
    face: Face,
    focus: &gpui_kit::FocusHandle,
    focused: bool,
    on_key: Rc<dyn Fn(&mut App)>,
    on_click: Rc<dyn Fn(&mut App)>,
    cx: &mut App,
) -> impl IntoElement {
    let name = face.title.as_ref().map_or_else(
        || face.label.clone(),
        |title| format!("{} {}", face.label, title),
    );
    let value = format!(
        "{} · {}",
        pick(face.open, "expanded", "collapsed"),
        face.tone.word()
    );
    let tooltip = face.title.clone();
    let id = face.id.clone();
    let indent = face.indent;
    let expandable = face.has_body;
    let hint = SharedString::from(face.word);
    content(face, cx)
        .id(id)
        .ui_observe_as(Facts {
            role: gpui_kit::Role::Button,
            name: name.into(),
            value: Some(value.into()),
            disabled: false,
            selected: None,
            focus: Some(focus.clone()),
            hint: Some(hint),
        })
        .track_focus(focus)
        .tab_stop(expandable)
        .h(px(row_height(cx)))
        .flex()
        .items_center()
        .gap(px(6.))
        .pr(px(8.))
        .pl(indent)
        .rounded(cx.theme().radius)
        .hover(|line| line.bg(cx.theme().list_hover))
        .when(focused, |line| {
            line.border_1().border_color(cx.theme().ring)
        })
        .when_some(tooltip, |line, tip| {
            line.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        })
        .when(expandable, |line| {
            line.on_click(move |_, _, cx| on_click(cx))
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        on_key(cx);
                    }
                })
        })
}

/// The line's children: chevron, icon, label, title, badge, icons and meta.
fn content(face: Face, cx: &mut App) -> gpui_kit::Div {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let label_color = match (face.tone, face.open) {
        (Tone::Failed, _) => Tone::Failed.color(cx),
        (Tone::Busy, _) => Tone::Busy.color(cx),
        (_, true) => foreground,
        _ => muted,
    };
    let chevron = if face.has_body {
        let icon = if face.open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        Icon::new(icon)
            .xsmall()
            .text_color(muted)
            .into_any_element()
    } else {
        div().w(px(12.)).flex_none().into_any_element()
    };
    let mut line = div()
        .child(chevron)
        .child(
            div()
                .w(px(16.))
                .flex()
                .justify_center()
                .child(Icon::new(face.icon).xsmall().text_color(muted)),
        )
        .child(
            div()
                .type_small(cx)
                .whitespace_nowrap()
                .text_color(label_color)
                .child(face.label),
        );
    if let Some(text) = face.title {
        let title = div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_color(foreground)
            .child(text);
        let title = if face.title_mono {
            title
                .font_family(cx.theme().mono_font_family.clone())
                .type_small(cx)
        } else {
            title.type_small(cx)
        };
        line = line.child(title);
    }
    if let Some(badge) = face.badge {
        line = line.child(
            div()
                .flex_none()
                .h(px(16.))
                .px(px(4.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().border)
                .type_caption(cx)
                .text_color(muted)
                .child(badge),
        );
    }
    if !face.icons.is_empty() {
        line = line.child(div().flex().flex_none().gap(px(2.)).children(face.icons));
    }
    if let Some(meta) = face.meta {
        line = line.child(
            div()
                .ml_auto()
                .pl(px(12.))
                .flex_none()
                .whitespace_nowrap()
                .type_caption(cx)
                .text_color(muted)
                .child(meta),
        );
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: Option<&str>, kind: ToolKind, title: &str, input: serde_json::Value) -> ToolRow {
        ToolRow {
            id: "t1".into(),
            name: name.map(str::to_owned),
            kind: Some(kind),
            title: Some(title.into()),
            status: Some(kg_domain::ToolStatus::Completed),
            input: Some(input),
            output: None,
            diff: Vec::new(),
            locations: Vec::new(),
            approval: None,
            from: Timestamp(0),
            to: Timestamp(1000),
        }
    }

    #[test]
    fn an_mcp_name_reads_as_its_server_and_tool() {
        assert_eq!(
            WorkKind::of(&tool(
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
            WorkKind::of(&tool(
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
        let kind = WorkKind::of(&tool(
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
        let title_only = WorkKind::of(&tool(
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
        let kind = WorkKind::of(&tool(
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
