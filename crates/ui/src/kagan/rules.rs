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
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{INSTRUCTIONS_MAX, InstructionsOp, Op, OpError, OpResult, ProjectId};

use crate::kagan::controls::control;
use crate::theme::{SpaceStep, TypeScale as _, space, type_body};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const CHANGED: &str =
    "The rules changed elsewhere. Your text is kept. Save will ask once before it replaces them.";
const REFUSED: &str =
    "Not saved: the rules changed elsewhere. Press Save again to replace them with your text.";
const SHOWN: &str = "The rules changed elsewhere, so this shows them.";
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
    _stored: Subscription,
}

impl RulesEditor {
    pub(super) fn new(
        project: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 16));
        let watch =
            cx.subscribe_in(
                &text,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        this.notice = None;
                        cx.notify();
                    }
                    InputEvent::PressEnter {
                        secondary: true, ..
                    } => this.save(window, cx),
                    _ => {}
                },
            );
        let stored = cx.subscribe_in(
            &Workspace::global(cx),
            window,
            |this, _, event: &WorkspaceEvent, window, cx| {
                if matches!(event, WorkspaceEvent::Changed) {
                    this.refresh(window, cx);
                }
            },
        );
        let mut editor = Self {
            project,
            text,
            state: Loaded::Pending,
            notice: None,
            load: None,
            save: None,
            _watch: watch,
            _stored: stored,
        };
        editor.refresh(window, cx);
        editor
    }

    /// Reads the stored rules. A box the person has not edited follows them; a draft is kept, and
    /// the next Save is refused until the person saves again.
    pub(super) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.read(false, window, cx);
    }

    fn read(&mut self, after_conflict: bool, window: &mut Window, cx: &mut Context<Self>) {
        let read = Workspace::dispatch(
            Op::Instructions(InstructionsOp::Get {
                project: self.project.clone(),
            }),
            cx,
        );
        self.load = Some(cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            _ = this.update_in(cx, |editor, window, cx| {
                editor.adopt(result, after_conflict, window, cx);
                cx.notify();
            });
        }));
    }

    fn adopt(
        &mut self,
        result: Result<OpResult, OpError>,
        after_conflict: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stored = match result {
            Ok(OpResult::Text(stored)) => stored,
            Ok(_) => {
                self.state = Loaded::Failed;
                self.notice = Some("The rules did not load.".into());
                return;
            }
            Err(error) => {
                self.state = Loaded::Failed;
                self.notice = Some(error.to_string());
                return;
            }
        };
        let draft = self.text.read(cx).value().to_string();
        match &self.state {
            Loaded::Ready { saved } if *saved == stored => {}
            Loaded::Ready { saved } if *saved != draft => {
                // The box holds the person's own text, so it stays. Until a Save has been refused,
                // the basis stays what they read, so the first Save is refused and says so.
                self.notice = Some(if after_conflict { REFUSED } else { CHANGED }.into());
                if after_conflict {
                    self.state = Loaded::Ready { saved: stored };
                }
            }
            _ => {
                self.text.update(cx, |state, cx| {
                    state.set_value(stored.clone(), window, cx);
                });
                self.notice = after_conflict.then(|| SHOWN.into());
                self.state = Loaded::Ready { saved: stored };
            }
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Loaded::Ready { saved } = &self.state else {
            return;
        };
        // Cmd+Enter inserts a newline before it saves, so trailing whitespace is not kept.
        let text = self.text.read(cx).value().trim_end().to_owned();
        if text.len() > INSTRUCTIONS_MAX {
            return;
        }
        let expected = Some(saved.clone());
        let project = self.project.clone();
        let op = if text.trim().is_empty() {
            InstructionsOp::Clear { project, expected }
        } else {
            InstructionsOp::Set {
                text: Some(text.clone()),
                file: None,
                project,
                expected,
            }
        };
        let written = Workspace::dispatch(Op::Instructions(op), cx);
        self.save = Some(cx.spawn_in(window, async move |this, cx| {
            let result = written.await;
            _ = this.update_in(cx, |editor, window, cx| match result {
                Ok(_) => {
                    editor.state = Loaded::Ready { saved: text };
                    editor.notice = Some("Saved.".into());
                    cx.notify();
                }
                Err(OpError::Changed(_)) => editor.read(true, window, cx),
                Err(error) => {
                    editor.notice = Some(error.to_string());
                    cx.notify();
                }
            });
        }));
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
                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
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
                column.child(
                    div()
                        .id("agent-rules-notice")
                        .ui_container()
                        .aria_label(notice.clone())
                        .type_small(cx)
                        .text_color(muted)
                        .child(notice),
                )
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
