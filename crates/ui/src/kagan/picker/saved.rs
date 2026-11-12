// Saved programs in the new thread picker: a custom command saved under a name, and a saved
// program's pin, rename and delete. The rows themselves come from `Discover`.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, Context, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, WeakEntity, Window, div,
};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, Program, ProgramOp, ProgramSource};

use super::list::custom_command;
use super::{Mode, Picker};
use crate::kagan::SaveSavedProgram;
use crate::kagan::controls::{control, dispatch, program_key};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl Picker {
    // Saves the custom command under the typed name, or renames the saved program being edited.
    pub(crate) fn save_program(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Custom {
            return;
        }
        let name = self.name.read(cx).value().trim().to_owned();
        let op = if let Some(from) = self.editing.clone() {
            ProgramOp::Rename {
                name: from,
                to: name,
            }
        } else {
            let program = self.program.read(cx).value().trim().to_owned();
            let arguments = self.arguments.read(cx).value().trim().to_owned();
            match custom_command(&program, &arguments) {
                Ok(line) => ProgramOp::Save { name, line },
                Err((_, message)) => {
                    self.fail(message, cx);
                    return;
                }
            }
        };
        let task = Workspace::dispatch(Op::Program(op), cx);
        self.after_change(task, window, cx);
    }

    // The name field becomes the only field, and Save becomes Rename, until the form closes.
    pub(super) fn begin_rename(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(Mode::Custom, cx);
        self.editing = Some(name.to_owned());
        self.notice = None;
        self.name
            .update(cx, |state, cx| state.set_value(name, window, cx));
        window.focus(&self.name.focus_handle(cx), cx);
        cx.notify();
    }

    // The keyboard path to the buttons: each acts on the selected row, and only a saved program has
    // one.
    pub(crate) fn pin_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(program) = self.selected_saved() {
            self.set_pinned(program.label, !program.pinned, window, cx);
        }
    }

    pub(crate) fn rename_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(program) = self.selected_saved() {
            self.begin_rename(&program.label, window, cx);
        }
    }

    pub(crate) fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(program) = self.selected_saved() {
            self.remove_saved(program.label, window, cx);
        }
    }

    fn selected_saved(&self) -> Option<Program> {
        self.cards
            .selected()
            .filter(|program| program.source == ProgramSource::Saved)
            .cloned()
    }

    pub(super) fn set_pinned(
        &mut self,
        name: String,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = Workspace::dispatch(Op::Program(ProgramOp::Pin { name, pinned }), cx);
        self.after_change(task, window, cx);
    }

    pub(super) fn remove_saved(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = Workspace::dispatch(Op::Program(ProgramOp::Remove { name }), cx);
        self.after_change(task, window, cx);
    }

    // A change that worked reads the list again, which also closes the form. One that failed
    // keeps the form and says why.
    fn after_change(
        &mut self,
        task: Task<Result<OpResult, OpError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notice = None;
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => {
                    this.name
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    let project = this.project.clone();
                    this.load(project, window, cx);
                }
                Err(error) => this.fail(error.to_string(), cx),
            });
        })
        .detach();
    }

    fn fail(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.notice = Some(message.into());
        cx.notify();
    }
}

// Save, or Rename while a saved program is being renamed. Its key is the same action.
pub(super) fn save_control(editing: bool) -> impl IntoElement {
    let label = if editing { "Rename" } else { "Save program" };
    control(
        "saved-program-save",
        Facts::button(label),
        Button::new("saved-program-save-button")
            .small()
            .outline()
            .label(label)
            .on_click(dispatch(SaveSavedProgram)),
    )
}

pub(super) fn notice(text: SharedString, cx: &gpui_kit::App) -> impl IntoElement {
    div()
        .id("saved-program-notice")
        .ui_container()
        .type_small(cx)
        .text_color(cx.theme().danger)
        .aria_label(text.clone())
        .child(text)
}

// A saved program's card buttons: pin or unpin, rename, and delete. Other cards have none. They are
// icons, always shown, so the card keeps its size.
pub(super) fn card_actions(program: &Program, picker: &WeakEntity<Picker>) -> Option<AnyElement> {
    if program.source != ProgramSource::Saved {
        return None;
    }
    let picker = picker.clone();
    let key = program_key(program);
    let name = program.label.clone();
    let (pin_label, pinned) = if program.pinned {
        ("Unpin", false)
    } else {
        ("Pin", true)
    };
    let (pin_name, rename_name, delete_name) = (name.clone(), name.clone(), name.clone());
    Some(
        h_flex()
            .gap_1()
            .flex_shrink_0()
            .child(card_button(
                format!("program-pin-{key}"),
                pin_label,
                Some(IconName::Pin),
                &name,
                picker.clone(),
                move |this, window, cx| this.set_pinned(pin_name.clone(), pinned, window, cx),
            ))
            .child(card_button(
                format!("program-rename-{key}"),
                "Rename",
                None,
                &name,
                picker.clone(),
                move |this, window, cx| this.begin_rename(&rename_name, window, cx),
            ))
            .child(card_button(
                format!("program-delete-{key}"),
                "Delete",
                None,
                &name,
                picker,
                move |this, window, cx| this.remove_saved(delete_name.clone(), window, cx),
            ))
            .into_any_element(),
    )
}

fn card_button(
    id: String,
    label: &'static str,
    icon: Option<IconName>,
    subject: &str,
    picker: WeakEntity<Picker>,
    run: impl Fn(&mut Picker, &mut Window, &mut Context<Picker>) + 'static,
) -> AnyElement {
    let button = format!("{id}-button");
    div()
        .id(id)
        .ui_observe_as(Facts::button(format!("{label} {subject}")))
        .child(
            match icon {
                Some(icon) => Button::new(button).small().ghost().icon(icon),
                None => Button::new(button).small().ghost().label(label),
            }
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                _ = picker.update(cx, |this, cx| run(this, window, cx));
            }),
        )
        .into_any_element()
}
