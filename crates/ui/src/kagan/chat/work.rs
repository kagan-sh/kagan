use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::{PlanStatus, Timestamp, WorkKind};
use std::rc::Rc;

use crate::kagan::chat::ChatView;
use crate::kagan::chat::body;
use crate::kagan::chat::kind::{self, pick, plural, span_text};
use crate::kagan::chat::part::{Row, RowKey, RowKind, ThoughtRow, ToolRow, ToolStage};
use crate::theme::{Tone, TypeScale as _, row_height};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn named(part: &'static str, id: impl std::fmt::Display) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("chat-{part}-{id}").into())
}

pub(super) fn work_row(
    row: &Row,
    tool: &ToolRow,
    indented: bool,
    open: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let kind = kind::of(tool);
    let stage = tool.stage();
    let body = open
        .then(|| body::work_body(row.id, tool, &kind, weak, cx))
        .map(IntoElement::into_any_element);
    let face = face(
        Face {
            id: named("work", row.id),
            key: row.key(),
            icon: kind::icon(&kind),
            label: kind::label(&kind, stage),
            title: kind::title(&kind, tool),
            title_mono: kind::mono_title(&kind),
            badge: kind::badge(&kind),
            meta: kind::meta(&kind, tool),
            icons: Vec::new(),
            word: kind.word(),
            open,
            has_body: kind::has_body(&kind, tool),
            tone: match (tool.failed(), stage) {
                (true, _) => Tone::Failed,
                (false, ToolStage::Waiting) => Tone::Waiting,
                (false, ToolStage::Running) => Tone::Busy,
                (false, ToolStage::Settled) => Tone::Done,
            },
            indent: if indented { px(24.) } else { px(4.) },
            body,
        },
        weak,
        window,
        cx,
    );
    match live_preview(row.id, tool, cx) {
        Some(preview) => div().child(face).child(preview).into_any_element(),
        None => face,
    }
}

fn live_preview(row: u64, tool: &ToolRow, cx: &mut App) -> Option<AnyElement> {
    if !tool.running() || kind::of(tool) != WorkKind::Execute {
        return None;
    }
    let lines: Vec<&str> = tool.output.as_deref()?.lines().rev().take(3).collect();
    if lines.is_empty() {
        return None;
    }
    let mut lines = lines;
    lines.reverse();
    let text = lines.join("\n");
    Some(
        div()
            .id(gpui_kit::ElementId::Name(
                format!("chat-preview-{row}").into(),
            ))
            .ui_observe_as(Facts {
                role: gpui_kit::Role::Label,
                value: Some(text.clone().into()),
                ..Facts::button("Preview")
            })
            .pl(px(28.))
            .font_family(cx.theme().mono_font_family.clone())
            .type_caption(cx)
            .text_color(cx.theme().muted_foreground)
            .children(lines.into_iter().map(|line| {
                div()
                    .whitespace_nowrap()
                    .child(SharedString::from(line.to_owned()))
            }))
            .into_any_element(),
    )
}

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
        format!(
            "Thought for {}",
            span_text(kind::span_of(thought.from, thought.to))
        )
    };
    let body = open
        .then(|| body::thought_body(row.id, &thought.text, cx))
        .map(IntoElement::into_any_element);
    face(
        Face {
            id: named("thought", row.id),
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
        match kind::of(tool) {
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
        .map(|(from, to)| span_text(kind::span_of(from, to)));
    let icons: Vec<AnyElement> = members
        .iter()
        .filter_map(|member| match &member.kind {
            RowKind::Work(tool) => Some(
                Icon::new(kind::icon(&kind::of(tool)))
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
                        .read_with(cx, |this, _| this.transcript.member_open(member.key()))
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
            id: named("bundle", row.id),
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
            id: named("plan", row.id),
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

fn line(
    face: Face,
    focus: &gpui_kit::FocusHandle,
    focused: bool,
    on_key: Rc<dyn Fn(&mut App)>,
    on_click: Rc<dyn Fn(&mut App)>,
    cx: &mut App,
) -> impl IntoElement {
    // Nothing depends on colour: a running or failed row carries its status as a word too.
    let word = matches!(face.tone, Tone::Busy | Tone::Failed).then(|| face.tone.word());
    let name = face.title.as_ref().map_or_else(
        || face.label.clone(),
        |title| format!("{} {}", face.label, title),
    );
    let name = match word {
        Some(word) => format!("{name} {word}"),
        None => name,
    };
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
    let mut line = div().child(chevron).child(
        div()
            .w(px(16.))
            .flex()
            .justify_center()
            .child(Icon::new(face.icon).xsmall().text_color(muted)),
    );
    if matches!(face.tone, Tone::Busy | Tone::Waiting | Tone::Failed) {
        line = line.child(div().flex_none().child(face.tone.mark(cx)));
    }
    line = line.child(
        div()
            .type_small(cx)
            .whitespace_nowrap()
            .text_color(label_color)
            .child(face.label),
    );
    if matches!(face.tone, Tone::Busy | Tone::Failed) {
        line = line.child(
            div()
                .flex_none()
                .type_caption(cx)
                .text_color(face.tone.color(cx))
                .child(face.tone.word()),
        );
    }
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
