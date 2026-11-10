//! The agent rules editor. Settings → General holds the global rules; a project's rules open in
//! a dialog from its row menu. Both read and save through `Op::Instructions`.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription,
    Task, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{INSTRUCTIONS_MAX, InstructionsOp, Op, OpResult, ProjectId};

use crate::kagan::controls::control;
use crate::theme::{SpaceStep, TypeScale as _, space, type_body};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const PROGRAMS: &str =
    "Claude Code, Codex, Cursor Agent and Pi read these rules. Kimi and opencode do not.";

/// What the box holds against the file. Save writes only in `Ready`, so a failed or slow read
/// can never turn into a Clear of a file the person did not see.
enum Loaded {
    Pending,
    Ready { saved: String },
    Failed,
}

pub(super) struct RulesEditor {
    project: Option<ProjectId>,
    text: Entity<TextareaState>,
    state: Loaded,
    notice: Option<String>,
    load: Option<Task<()>>,
    save: Option<Task<()>>,
    _watch: Subscription,
}

impl RulesEditor {
    pub(super) fn new(
        project: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 16));
        let watch = cx.subscribe(&text, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                this.notice = None;
                cx.notify();
            }
            InputEvent::PressEnter {
                secondary: true, ..
            } => this.save(cx),
            _ => {}
        });
        let mut editor = Self {
            project,
            text,
            state: Loaded::Pending,
            notice: None,
            load: None,
            save: None,
            _watch: watch,
        };
        editor.load(window, cx);
        editor
    }

    /// Reads the file again when the page opens, so a change made from the CLI shows. A draft
    /// the person typed is kept: only a box that still matches the file is replaced.
    pub(super) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Loaded::Ready { saved } = &self.state
            && self.text.read(cx).value().as_ref() != saved.as_str()
        {
            return;
        }
        self.load(window, cx);
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state = Loaded::Pending;
        let read = Workspace::dispatch(
            Op::Instructions(InstructionsOp::Get {
                project: self.project.clone(),
            }),
            cx,
        );
        self.load = Some(cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            _ = this.update_in(cx, |editor, window, cx| {
                match result {
                    Ok(OpResult::Text(saved)) => {
                        editor.text.update(cx, |state, cx| {
                            state.set_value(saved.clone(), window, cx);
                        });
                        editor.state = Loaded::Ready { saved };
                        editor.notice = None;
                    }
                    Ok(_) => {
                        editor.state = Loaded::Failed;
                        editor.notice = Some("The rules did not load.".into());
                    }
                    Err(error) => {
                        editor.state = Loaded::Failed;
                        editor.notice = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.state, Loaded::Ready { .. }) {
            return;
        }
        // Cmd+Enter inserts a newline before it saves, so trailing whitespace is not kept.
        let text = self.text.read(cx).value().trim_end().to_owned();
        if text.len() > INSTRUCTIONS_MAX {
            return;
        }
        let saved = text.clone();
        let op = if text.trim().is_empty() {
            InstructionsOp::Clear {
                project: self.project.clone(),
            }
        } else {
            InstructionsOp::Set {
                text: Some(text),
                file: None,
                project: self.project.clone(),
            }
        };
        let written = Workspace::dispatch(Op::Instructions(op), cx);
        self.save = Some(cx.spawn(async move |this, cx| {
            let result = written.await;
            _ = this.update(cx, |editor, cx| {
                match result {
                    Ok(_) => {
                        editor.state = Loaded::Ready { saved };
                        editor.notice = Some("Saved.".into());
                    }
                    Err(error) => editor.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = self.text.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }
}

impl Render for RulesEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.text.read(cx).value().to_string();
        let bytes = text.len();
        let over = bytes.saturating_sub(INSTRUCTIONS_MAX);
        let on_disk = matches!(&self.state, Loaded::Ready { saved } if *saved == text);
        let can_save = matches!(self.state, Loaded::Ready { .. }) && over == 0;
        let (status, status_color) = if over == 0 {
            (
                format!("{} of {} bytes", count(bytes), count(INSTRUCTIONS_MAX)),
                cx.theme().muted_foreground,
            )
        } else if on_disk {
            (
                format!(
                    "This file is over the limit, so agents read only the first {} bytes. Remove {} bytes to save.",
                    count(INSTRUCTIONS_MAX),
                    count(over)
                ),
                cx.theme().danger,
            )
        } else {
            (
                format!("{} bytes over the limit. Remove them to save.", count(over)),
                cx.theme().danger,
            )
        };
        let focus = self.text.read(cx).focus_handle(cx);
        let observed = Facts {
            role: Role::TextInput,
            name: "Agent rules".into(),
            value: Some(text.into()),
            disabled: false,
            selected: None,
            focus: Some(focus),
            hint: None,
        };
        let save = control(
            "agent-rules-save",
            Facts {
                disabled: !can_save,
                ..Facts::button("Save agent rules")
            },
            Button::new("agent-rules-save-button")
                .small()
                .primary()
                .label("Save")
                .disabled(!can_save)
                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
        );
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap(px(space(SpaceStep::Sm, cx)))
            .child(
                div().id("agent-rules-text").ui_observe_as(observed).child(
                    Textarea::new(&self.text)
                        .text_size(px(type_body(cx)))
                        .small(),
                ),
            )
            .child(div().type_small(cx).text_color(muted).child(PROGRAMS))
            .child(
                h_flex()
                    .gap(px(space(SpaceStep::Sm, cx)))
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .id("agent-rules-status")
                            .ui_container()
                            .aria_label(status.clone())
                            .type_small(cx)
                            .text_color(status_color)
                            .child(status),
                    )
                    .child(save),
            )
            .when_some(self.notice.clone(), |column, notice| {
                column.child(div().type_small(cx).text_color(muted).child(notice))
            })
    }
}

/// Opens one project's rules in a dialog. The box takes focus, so typing goes straight in.
pub(super) fn open_project(project: ProjectId, window: &mut Window, cx: &mut App) {
    let editor = cx.new(|cx| RulesEditor::new(Some(project), window, cx));
    let shown = editor.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title("Agent rules").child(shown.clone())
    });
    let handle = editor.read(cx).text.read(cx).focus_handle(cx);
    window.focus(&handle, cx);
}

fn count(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn count_groups_thousands() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(16_384), "16,384");
        assert_eq!(count(1_234_567), "1,234,567");
    }
}
