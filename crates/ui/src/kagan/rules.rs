//! The agent rules editor. Settings → General holds the global rules; a project's rules open in
//! a dialog from its row menu. Both read and save through `Op::Instructions`.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Task, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{INSTRUCTIONS_MAX, InstructionsOp, Op, OpResult, ProjectId};

use crate::kagan::controls::control;
use crate::theme::{SpaceStep, TypeScale as _, space, type_body};
use crate::ui_registry::Facts;

const PROGRAMS: &str =
    "Claude Code, Codex, Cursor Agent and Pi read these rules. Kimi and opencode do not.";

pub(super) struct RulesEditor {
    project: Option<ProjectId>,
    text: Entity<TextareaState>,
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
        let watch = cx.subscribe(&text, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut editor = Self {
            project,
            text,
            notice: None,
            load: None,
            save: None,
            _watch: watch,
        };
        editor.load(window, cx);
        editor
    }

    // Reads the file again, so a change made from the CLI shows when the page opens.
    pub(super) fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    Ok(OpResult::Text(saved)) => editor.text.update(cx, |state, cx| {
                        state.set_value(saved, window, cx);
                    }),
                    Ok(_) => editor.notice = Some("The rules did not load.".into()),
                    Err(error) => editor.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.text.read(cx).value().to_string();
        if text.len() > INSTRUCTIONS_MAX {
            return;
        }
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
                editor.notice = match result {
                    Ok(_) => Some("Saved.".into()),
                    Err(error) => Some(error.to_string()),
                };
                cx.notify();
            });
        }));
    }
}

impl Render for RulesEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bytes = self.text.read(cx).value().len();
        let over = bytes.saturating_sub(INSTRUCTIONS_MAX);
        let (status, status_color) = if over > 0 {
            (
                format!(
                    "Too long. Agents read only the first {} bytes. Remove {} bytes to save.",
                    count(INSTRUCTIONS_MAX),
                    count(over)
                ),
                cx.theme().danger,
            )
        } else {
            (
                format!("{} of {} bytes", count(bytes), count(INSTRUCTIONS_MAX)),
                cx.theme().muted_foreground,
            )
        };
        let save = control(
            "agent-rules-save",
            Facts::button("Save agent rules"),
            Button::new("agent-rules-save-button")
                .small()
                .primary()
                .label("Save")
                .disabled(over > 0)
                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
        );
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap(px(space(SpaceStep::Sm, cx)))
            .child(
                Textarea::new(&self.text)
                    .text_size(px(type_body(cx)))
                    .small(),
            )
            .child(div().type_small(cx).text_color(muted).child(PROGRAMS))
            .child(
                h_flex()
                    .gap(px(space(SpaceStep::Sm, cx)))
                    .justify_between()
                    .items_center()
                    .child(div().type_small(cx).text_color(status_color).child(status))
                    .child(save),
            )
            .when_some(self.notice.clone(), |column, notice| {
                column.child(div().type_small(cx).text_color(muted).child(notice))
            })
    }
}

/// Opens one project's rules in a dialog. Escape closes it, as with every dialog.
pub(super) fn open_project(project: ProjectId, window: &mut Window, cx: &mut gpui_kit::App) {
    let editor = cx.new(|cx| RulesEditor::new(Some(project), window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title("Agent rules").child(editor.clone())
    });
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
