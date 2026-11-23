use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::{PlanEntry, PlanStatus, ToolDiff};

use crate::kagan::attention;
use crate::kagan::chat::ChatView;
use crate::kagan::chat::part::ToolRow;
use crate::kagan::chat::work::WorkKind;
use crate::kagan::question;
use crate::theme::{Tone, TypeScale as _, added, terminal_color};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// The most lines of output a body shows before it stops.
const OUTPUT_LINES: usize = 40;
/// The most argument lines a body shows before "Show more".
const ARG_LINES: usize = 3;
/// The most diff lines a body shows.
const DIFF_LINES: usize = 120;
/// The most height a streaming body takes before it scrolls.
const BODY_MAX: f32 = 240.;

/// What to show when a row is expanded.
pub(super) fn work_body(
    row: u64,
    tool: &ToolRow,
    kind: &WorkKind,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let _ = window;
    let more = weak
        .read_with(cx, |this, _| this.more_open(row))
        .unwrap_or(false);
    let output = match kind {
        WorkKind::Execute => terminal_out(row, tool.output.as_deref(), cx),
        WorkKind::Edit | WorkKind::Delete | WorkKind::Move => {
            diff_out(row, &tool.diff, tool.output.as_deref(), cx)
        }
        WorkKind::Think => thought_text(row, tool.output.as_deref().unwrap_or_default(), cx),
        _ => plain_out(row, tool.output.as_deref(), cx),
    };
    let args = matches!(
        kind,
        WorkKind::Read
            | WorkKind::Search
            | WorkKind::Fetch
            | WorkKind::Mcp { .. }
            | WorkKind::Skill { .. }
            | WorkKind::Think
            | WorkKind::Other { .. }
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
        .ui_container()
}

/// The reasoning, muted, markdown, capped and scrollable.
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

/// The plan's steps, one row each, with a mark and the status in words.
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

/// The approval card: what the agent wants to do, in one plain sentence, the command or file in
/// mono, and the agent's own options.
pub(super) fn approval_card(
    id: &kg_domain::PendingInteractionId,
    tool_title: Option<&str>,
    worktree: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let asked = attention::global(cx)
        .read(cx)
        .questions
        .iter()
        .find(|asked| &asked.id == id)
        .cloned();
    let Some(asked) = asked else {
        return div().into_any_element();
    };
    let detail = detail(id, tool_title, worktree, cx);
    let failed: question::Failed = {
        let weak = weak.clone();
        Rc::new(move |cx, text| {
            _ = weak.update(cx, |this, cx| this.say(text, cx));
        })
    };
    div()
        .id(crate::kagan::chat::work::named(
            crate::kagan::chat::part::Part::Approval,
            &id.0,
        ))
        .child(question::permission_card(
            &asked,
            Some(detail),
            &failed,
            window,
            cx,
        ))
        .ui_container()
        .into_any_element()
}

/// Whether the card says where the tool runs: only a command or an edit works in a folder.
pub(super) fn runs_in_worktree(kind: Option<kg_domain::ToolKind>, environment: bool) -> bool {
    environment
        && matches!(
            kind,
            Some(kg_domain::ToolKind::Execute | kg_domain::ToolKind::Edit)
        )
}

fn detail(
    id: &kg_domain::PendingInteractionId,
    tool_title: Option<&str>,
    worktree: bool,
    cx: &mut App,
) -> AnyElement {
    let mono = cx.theme().mono_font_family.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .when_some(tool_title.map(str::to_owned), |detail, title| {
            let tip = title.clone();
            let fact = Facts {
                role: gpui_kit::Role::Label,
                ..Facts::button(title.clone())
            };
            detail.child(
                div()
                    .id(gpui_kit::ElementId::Name(
                        format!("chat-approval-value-{}", id.0).into(),
                    ))
                    .ui_observe_as(fact)
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().input)
                    .font_family(mono)
                    .type_small(cx)
                    .text_color(cx.theme().foreground)
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(title),
            )
        })
        .when(worktree, |detail| {
            detail.child(
                div()
                    .id(gpui_kit::ElementId::Name(
                        format!("chat-approval-runs-{}", id.0).into(),
                    ))
                    .ui_observe_as(Facts {
                        role: gpui_kit::Role::Label,
                        ..Facts::button("It runs in this thread's worktree.")
                    })
                    .type_caption(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child("It runs in this thread's worktree."),
            )
        })
        .into_any_element()
}

/// `key: value` lines for the raw input, three of them until the person asks for more.
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

/// What `kg ui` reads of a body's output: the lines the block shows.
fn output_fact(lines: &[&str]) -> Facts {
    Facts {
        role: gpui_kit::Role::Label,
        value: Some(lines.join("\n").into()),
        ..Facts::button("Output")
    }
}

/// Plain output in a surface block, the first lines only.
fn plain_out(row: u64, output: Option<&str>, cx: &mut App) -> Option<AnyElement> {
    let output = output?;
    let lines: Vec<&str> = output.lines().take(OUTPUT_LINES).collect();
    let fact = output_fact(&lines);
    Some(
        div()
            .id(gpui_kit::ElementId::Name(
                format!("chat-output-{row}").into(),
            ))
            .ui_observe_as(fact)
            .max_h(px(BODY_MAX))
            .overflow_y_scroll()
            .px(px(8.))
            .py(px(6.))
            .mb(px(4.))
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .font_family(cx.theme().mono_font_family.clone())
            .type_small(cx)
            .children(lines.into_iter().map(|line| {
                div()
                    .whitespace_nowrap()
                    .child(SharedString::from(line.to_owned()))
            }))
            .into_any_element(),
    )
}

/// A command's output in the terminal colours, capped and scrollable.
fn terminal_out(row: u64, output: Option<&str>, cx: &mut App) -> Option<AnyElement> {
    let output = output?;
    let background = terminal_color(kg_core::Color::Background, cx);
    let foreground = terminal_color(kg_core::Color::Foreground, cx);
    let lines: Vec<&str> = output.lines().take(OUTPUT_LINES).collect();
    let fact = output_fact(&lines);
    Some(
        div()
            .id(gpui_kit::ElementId::Name(
                format!("chat-terminal-{row}").into(),
            ))
            .ui_observe_as(fact)
            .max_h(px(BODY_MAX))
            .overflow_y_scroll()
            .px(px(8.))
            .py(px(6.))
            .mb(px(4.))
            .rounded(cx.theme().radius)
            .bg(background)
            .text_color(foreground)
            .font_family(cx.theme().mono_font_family.clone())
            .type_small(cx)
            .children(lines.into_iter().map(|line| {
                div()
                    .whitespace_nowrap()
                    .child(SharedString::from(line.to_owned()))
            }))
            .into_any_element(),
    )
}

/// The reasoning a tool named, muted.
fn thought_text(row: u64, text: &str, cx: &mut App) -> Option<AnyElement> {
    (!text.trim().is_empty()).then(|| {
        div()
            .id(gpui_kit::ElementId::Name(
                format!("chat-reason-{row}").into(),
            ))
            .max_h(px(BODY_MAX))
            .overflow_y_scroll()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(text.to_owned()))
            .into_any_element()
    })
}

/// The diff, hunked, with `+` in the added colour and `−` in the failed one.
fn diff_out(
    row: u64,
    diffs: &[ToolDiff],
    output: Option<&str>,
    cx: &mut App,
) -> Option<AnyElement> {
    if diffs.is_empty() {
        return plain_out(row, output, cx);
    }
    let background = terminal_color(kg_core::Color::Background, cx);
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
            let (color, mark) =
                if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
                    (cx.theme().muted_foreground, "")
                } else if line.starts_with('+') {
                    (added(cx), "")
                } else if line.starts_with('-') {
                    (Tone::Failed.color(cx), "")
                } else {
                    (foreground, "")
                };
            lines.push(
                div()
                    .whitespace_nowrap()
                    .text_color(color)
                    .child(SharedString::from(format!("{mark}{line}")))
                    .into_any_element(),
            );
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
    Some(
        div()
            .id(gpui_kit::ElementId::Name(format!("chat-diff-{row}").into()))
            .max_h(px(BODY_MAX))
            .overflow_y_scroll()
            .px(px(8.))
            .py(px(6.))
            .mb(px(4.))
            .rounded(cx.theme().radius)
            .bg(background)
            .font_family(cx.theme().mono_font_family.clone())
            .type_small(cx)
            .children(lines)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_command_or_an_edit_in_a_folder_says_where_it_runs() {
        assert!(runs_in_worktree(Some(kg_domain::ToolKind::Execute), true));
        assert!(runs_in_worktree(Some(kg_domain::ToolKind::Edit), true));
        assert!(!runs_in_worktree(Some(kg_domain::ToolKind::Read), true));
        assert!(!runs_in_worktree(Some(kg_domain::ToolKind::Execute), false));
    }
}
