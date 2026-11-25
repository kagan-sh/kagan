use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::{PlanEntry, PlanStatus, ToolDiff};

use crate::kagan::chat::ChatView;
use crate::kagan::chat::part::ToolRow;
use crate::theme::{Tone, TypeScale as _, added, terminal_color};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use kg_domain::WorkKind;

const OUTPUT_LINES: usize = 40;
const ARG_LINES: usize = 3;
const DIFF_LINES: usize = 120;
const BODY_MAX: f32 = 240.;

pub(super) fn work_body(
    row: u64,
    tool: &ToolRow,
    kind: &WorkKind,
    weak: &WeakEntity<ChatView>,
    cx: &mut App,
) -> impl IntoElement {
    let more = weak
        .read_with(cx, |this, _| this.more.contains(&row))
        .unwrap_or(false);
    let output = match kind {
        WorkKind::Execute => terminal_out(row, tool.output.as_deref(), tool.running(), cx),
        WorkKind::Edit | WorkKind::Delete | WorkKind::Move => {
            diff_out(row, &tool.diff, tool.output.as_deref(), cx)
        }
        WorkKind::Think => thought_text(row, tool.output.as_deref().unwrap_or_default(), cx),
        WorkKind::Mcp { .. } => mcp_out(row, tool, cx),
        _ => plain_out(row, tool.output.as_deref(), cx),
    };
    let link = match kind {
        WorkKind::Edit | WorkKind::Delete | WorkKind::Move => review_link(row, tool, weak, cx),
        WorkKind::Skill { .. } => Some(skill_link(row)),
        _ => None,
    };
    let args = matches!(
        kind,
        WorkKind::Read
            | WorkKind::Search
            | WorkKind::Fetch
            | WorkKind::Mcp { .. }
            | WorkKind::Skill { .. }
            | WorkKind::Think
            | WorkKind::Other
    )
    .then(|| args(tool, more, row, weak, cx))
    .flatten();
    div()
        .id(gpui_kit::ElementId::Name(
            format!("chat-work-body-{row}").into(),
        ))
        .py(px(2.))
        .gap(px(4.))
        .flex()
        .flex_col()
        .min_w_0()
        .children(args)
        .children(output)
        .children(link)
        .ui_container()
}

fn review_link(
    row: u64,
    tool: &ToolRow,
    weak: &WeakEntity<ChatView>,
    cx: &mut App,
) -> Option<AnyElement> {
    if !tool.diff.is_empty() && tool.running() {
        return None;
    }
    let path = tool.diff.first()?.path.clone();
    let kagan = weak.read_with(cx, |this, _| this.kagan.clone()).ok()?;
    Some(link(row, "review", "Open in review", move |window, cx| {
        _ = kagan.update(cx, |kagan, cx| {
            kagan.open_review(path.clone(), window, cx);
        });
    }))
}

fn skill_link(row: u64) -> AnyElement {
    link(row, "skill", "Open skill", |_, cx| {
        _ = crate::kagan::show_skills(cx);
    })
}

fn link(
    row: u64,
    name: &'static str,
    label: &'static str,
    action: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = format!("chat-link-{name}-{row}");
    div()
        .id(gpui_kit::ElementId::Name(id.clone().into()))
        .ui_observe_as(Facts::button(label))
        .child(
            Button::new(SharedString::from(format!("{id}-button")))
                .small()
                .ghost()
                .label(label)
                .on_click(move |_, window, cx| action(window, cx)),
        )
        .into_any_element()
}

fn mcp_out(row: u64, tool: &ToolRow, cx: &mut App) -> Option<AnyElement> {
    let output = tool.output.as_deref()?;
    let items = serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .and_then(|value| {
            let list = match &value {
                serde_json::Value::Array(items) => Some(items),
                serde_json::Value::Object(map) => {
                    map.get("items").and_then(serde_json::Value::as_array)
                }
                _ => None,
            };
            list.map(Vec::len)
        });
    let count = items.map(|items| {
        let id = format!("chat-items-{row}");
        div()
            .id(gpui_kit::ElementId::Name(id.into()))
            .ui_observe_as(Facts::button(format!(
                "{items} {}",
                if items == 1 { "item" } else { "items" }
            )))
            .type_caption(cx)
            .text_color(cx.theme().muted_foreground)
            .child(format!(
                "{items} {}",
                if items == 1 { "item" } else { "items" }
            ))
            .into_any_element()
    });
    let text = plain_out(row, Some(output), cx);
    match (count, text) {
        (None, None) => None,
        (count, text) => Some(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(count)
                .children(text)
                .into_any_element(),
        ),
    }
}

pub(super) fn thought_body(row: u64, text: &str, cx: &mut App) -> impl IntoElement {
    div()
        .id(gpui_kit::ElementId::Name(
            format!("chat-thought-body-{row}").into(),
        ))
        .max_h(px(BODY_MAX))
        .overflow_y_scroll()
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .child(gpui_kit::component::text::TextView::markdown(
            SharedString::from(format!("chat-thought-md-{row}")),
            text.to_owned(),
        ))
        .ui_container()
}

pub(super) fn plan_body(row: u64, entries: &[PlanEntry], cx: &mut App) -> impl IntoElement {
    let mut steps: Vec<AnyElement> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let (icon, color, word) = match entry.status {
            PlanStatus::Completed => (
                gpui_kit::assets::IconName::CircleCheck,
                Tone::Done.color(cx),
                "done",
            ),
            PlanStatus::InProgress => (
                gpui_kit::assets::IconName::CircleDot,
                Tone::Busy.color(cx),
                "in progress",
            ),
            PlanStatus::Pending => (
                gpui_kit::assets::IconName::Circle,
                cx.theme().muted_foreground,
                "pending",
            ),
        };
        let mut text = div()
            .type_small(cx)
            .text_color(cx.theme().foreground)
            .child(entry.text.clone());
        if entry.status == PlanStatus::Completed {
            text = text.line_through().text_color(cx.theme().muted_foreground);
        }
        steps.push(
            div()
                .id(gpui_kit::ElementId::Name(
                    format!("chat-plan-{row}-{index}").into(),
                ))
                .h(px(24.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(Icon::new(icon).xsmall().text_color(color))
                .child(text)
                .child(
                    div()
                        .ml_auto()
                        .type_caption(cx)
                        .text_color(cx.theme().muted_foreground)
                        .child(word),
                )
                .into_any_element(),
        );
    }
    div()
        .id(gpui_kit::ElementId::Name(
            format!("chat-plan-body-{row}").into(),
        ))
        .py(px(2.))
        .children(steps)
        .ui_container()
}

fn args(
    tool: &ToolRow,
    more: bool,
    row: u64,
    weak: &WeakEntity<ChatView>,
    cx: &mut App,
) -> Option<AnyElement> {
    let entries: Vec<(String, String)> = match tool.input.as_ref()? {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| (key.clone(), value_text(value)))
            .collect(),
        other => vec![("input".into(), value_text(other))],
    };
    if entries.is_empty() {
        return None;
    }
    let hidden = entries.len().saturating_sub(ARG_LINES);
    let shown = if more {
        entries.len()
    } else {
        ARG_LINES.min(entries.len())
    };
    let mono = cx.theme().mono_font_family.clone();
    let lines: Vec<AnyElement> = entries
        .iter()
        .take(shown)
        .map(|(key, value)| {
            h_flex()
                .gap(px(6.))
                .type_small(cx)
                .font_family(mono.clone())
                .child(
                    div()
                        .flex_none()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{key}:")),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(cx.theme().foreground)
                        .child(value.clone()),
                )
                .into_any_element()
        })
        .collect();
    let button = (hidden > 0).then(|| {
        let label = if more {
            "Show less".to_owned()
        } else {
            format!("Show {hidden} more")
        };
        let id = format!("chat-more-{row}");
        let weak = weak.clone();
        let click = move |cx: &mut App| {
            _ = weak.update(cx, |this, cx| this.toggle_more(row, cx));
        };
        div()
            .id(gpui_kit::ElementId::Name(id.clone().into()))
            .ui_observe_as(Facts::button(label.clone()))
            .child(
                Button::new(SharedString::from(format!("{id}-button")))
                    .small()
                    .ghost()
                    .label(label)
                    .on_click(move |_, _, cx| click(cx)),
            )
            .into_any_element()
    });
    Some(
        div()
            .id(gpui_kit::ElementId::Name(format!("chat-args-{row}").into()))
            .py(px(2.))
            .flex()
            .flex_col()
            .children(lines)
            .children(button)
            .into_any_element(),
    )
}

fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn output_fact(lines: &[&str]) -> Facts {
    Facts {
        role: gpui_kit::Role::Label,
        value: Some(lines.join("\n").into()),
        ..Facts::button("Output")
    }
}

fn output_block(
    name: String,
    lines: Vec<AnyElement>,
    fact: Option<Facts>,
    terminal: bool,
    cx: &mut App,
) -> AnyElement {
    let block = div()
        .id(gpui_kit::ElementId::Name(name.into()))
        .max_h(px(BODY_MAX))
        .overflow_y_scroll()
        .px(px(8.))
        .py(px(6.))
        .mb(px(4.))
        .rounded(cx.theme().radius)
        .font_family(cx.theme().mono_font_family.clone())
        .type_small(cx);
    let block = if terminal {
        block
            .bg(terminal_color(kg_core::Color::Background, cx))
            .text_color(terminal_color(kg_core::Color::Foreground, cx))
    } else {
        block
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
    }
    .children(lines);
    match fact {
        Some(fact) => block.ui_observe_as(fact).into_any_element(),
        None => block.into_any_element(),
    }
}

fn output_line(line: &str) -> gpui_kit::Div {
    div()
        .whitespace_nowrap()
        .child(SharedString::from(line.to_owned()))
}

fn plain_out(row: u64, output: Option<&str>, cx: &mut App) -> Option<AnyElement> {
    let output = output?;
    let lines: Vec<&str> = output.lines().take(OUTPUT_LINES).collect();
    let fact = output_fact(&lines);
    let block = output_block(
        format!("chat-output-{row}"),
        lines
            .into_iter()
            .map(|line| output_line(line).into_any_element())
            .collect(),
        Some(fact),
        false,
        cx,
    );
    Some(block)
}

fn terminal_out(row: u64, output: Option<&str>, live: bool, cx: &mut App) -> Option<AnyElement> {
    let output = output?;
    // A running command shows its newest lines, so the body stays at the bottom as it streams.
    let mut lines: Vec<&str> = if live {
        output.lines().rev().take(OUTPUT_LINES).collect()
    } else {
        output.lines().take(OUTPUT_LINES).collect()
    };
    if live {
        lines.reverse();
    }
    let fact = output_fact(&lines);
    let block = output_block(
        format!("chat-terminal-{row}"),
        lines
            .into_iter()
            .map(|line| output_line(line).into_any_element())
            .collect(),
        Some(fact),
        true,
        cx,
    );
    Some(block)
}

fn thought_text(row: u64, text: &str, cx: &mut App) -> Option<AnyElement> {
    (!text.trim().is_empty()).then(|| thought_body(row, text, cx).into_any_element())
}

fn diff_out(
    row: u64,
    diffs: &[ToolDiff],
    output: Option<&str>,
    cx: &mut App,
) -> Option<AnyElement> {
    if diffs.is_empty() {
        return plain_out(row, output, cx);
    }
    let foreground = terminal_color(kg_core::Color::Foreground, cx);
    let mut lines: Vec<AnyElement> = Vec::new();
    for diff in diffs {
        let text =
            similar::TextDiff::from_lines(diff.old.as_deref().unwrap_or_default(), &diff.new)
                .unified_diff()
                .context_radius(3)
                .header(&diff.path, &diff.path)
                .to_string();
        for line in text.lines().take(DIFF_LINES.saturating_sub(lines.len())) {
            let color =
                if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
                    cx.theme().muted_foreground
                } else if line.starts_with('+') {
                    added(cx)
                } else if line.starts_with('-') {
                    Tone::Failed.color(cx)
                } else {
                    foreground
                };
            lines.push(output_line(line).text_color(color).into_any_element());
        }
        if lines.len() >= DIFF_LINES {
            lines.push(
                div()
                    .type_caption(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child("… (truncated)")
                    .into_any_element(),
            );
            break;
        }
    }
    Some(output_block(
        format!("chat-diff-{row}"),
        lines,
        None,
        true,
        cx,
    ))
}
