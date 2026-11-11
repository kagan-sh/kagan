use std::rc::Rc;

use crate::theme::{SpaceStep, TypeScale as _, space};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, IndexPath, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, ElementId, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, KeyBinding, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, Task, WeakEntity, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, SavedPrompt, SavedPromptId, SavedPromptOp};

use super::{
    CommandList, DeleteSavedPrompt, Mode, NewSavedPrompt, SaveSavedPrompt, StarSavedPrompt,
};
use crate::kagan::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) const CONTEXT: &str = "SavedPrompts";
const UNEXPECTED: &str = "Saved prompts gave an unexpected answer.";
const NO_THREAD: &str = "No thread is shown.";

#[derive(Clone)]
pub(super) struct Draft {
    title: Entity<InputState>,
    body: Entity<TextareaState>,
}

pub(crate) fn chords() -> [&'static str; 4] {
    if cfg!(target_os = "macos") {
        ["cmd-d", "cmd-alt-n", "cmd-alt-backspace", "cmd-alt-s"]
    } else {
        [
            "ctrl-shift-d",
            "ctrl-shift-alt-n",
            "ctrl-shift-alt-backspace",
            "ctrl-shift-alt-s",
        ]
    }
}

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let [star, new, delete, save] = chords();
    vec![
        KeyBinding::new(star, StarSavedPrompt, Some(CONTEXT)),
        KeyBinding::new(new, NewSavedPrompt, Some(CONTEXT)),
        KeyBinding::new(delete, DeleteSavedPrompt, Some(CONTEXT)),
        KeyBinding::new(save, SaveSavedPrompt, Some(CONTEXT)),
    ]
}

impl Kagan {
    pub(super) fn search_saved(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(list) = self.commands.as_mut() else {
            return;
        };
        if list.mode != Mode::Saved {
            return;
        }
        list.failed = None;
        list.state
            .update(cx, |state, cx| state.set_loading(true, window, cx));
        let text = query.to_owned();
        let task = Workspace::dispatch(Op::SavedPrompt(SavedPromptOp::Search { text }), cx);
        list.search = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                let Some(list) = this.commands.as_mut() else {
                    return;
                };
                if list.mode != Mode::Saved {
                    return;
                }
                match result {
                    Ok(OpResult::SavedPrompts(prompts)) => {
                        list.prompts = prompts;
                        list.failed = None;
                    }
                    Ok(_) => list.failed = Some(UNEXPECTED.into()),
                    Err(error) => list.failed = Some(error.to_string().into()),
                }
                list.state
                    .update(cx, |state, cx| state.set_loading(false, window, cx));
                cx.notify();
            });
        });
        cx.notify();
    }

    // Escape closes only a draft when one is open, so typed text is not lost by accident.
    pub(super) fn back_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .commands
            .as_ref()
            .is_some_and(|list| list.draft.is_some())
        {
            self.cancel_draft(window, cx);
        } else {
            self.close_commands(window, cx);
        }
    }

    pub(super) fn new_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .commands
            .as_ref()
            .is_some_and(|list| list.mode == Mode::Saved && list.draft.is_none())
        {
            self.begin_draft(window, cx);
        }
    }

    pub(super) fn star_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.selected_prompt(cx) else {
            return;
        };
        self.set_star(prompt.id, !prompt.starred, window, cx);
    }

    pub(super) fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.selected_prompt(cx) else {
            return;
        };
        self.delete_saved(prompt.id, window, cx);
    }

    fn selected_prompt(&self, cx: &App) -> Option<SavedPrompt> {
        let list = self
            .commands
            .as_ref()
            .filter(|list| list.mode == Mode::Saved && list.draft.is_none())?;
        let row = list.state.read(cx).selected_index()?.row;
        list.prompts.get(row).cloned()
    }

    fn begin_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title"));
        let body = cx.new(|cx| TextareaState::new(window, cx));
        let Some(list) = self.commands.as_mut() else {
            return;
        };
        list.failed = None;
        list.draft = Some(Draft { title, body });
        let handle = list
            .draft
            .as_ref()
            .map(|draft| draft.title.read(cx).focus_handle(cx));
        if let Some(handle) = handle {
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(list) = self.commands.as_mut() else {
            return;
        };
        list.draft = None;
        list.failed = None;
        list.state.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    pub(super) fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.commands.as_ref().and_then(|list| list.draft.clone()) else {
            return;
        };
        let title = draft.title.read(cx).value().to_string();
        let body = draft.body.read(cx).value().to_string();
        let task = Workspace::dispatch(Op::SavedPrompt(SavedPromptOp::Add { title, body }), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                let Some(list) = this.commands.as_mut() else {
                    return;
                };
                match result {
                    Ok(OpResult::SavedPrompt(_)) => {
                        list.draft = None;
                        list.failed = None;
                        let query = list.state.read(cx).query(cx).to_string();
                        this.search_saved(&query, window, cx);
                    }
                    Ok(_) => list.failed = Some(UNEXPECTED.into()),
                    Err(error) => list.failed = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_star(
        &mut self,
        id: SavedPromptId,
        starred: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = Workspace::dispatch(Op::SavedPrompt(SavedPromptOp::Star { id, starred }), cx);
        self.after_saved(task, window, cx);
    }

    fn delete_saved(&mut self, id: SavedPromptId, window: &mut Window, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(Op::SavedPrompt(SavedPromptOp::Delete { id }), cx);
        self.after_saved(task, window, cx);
    }

    fn after_saved(
        &mut self,
        task: Task<std::result::Result<OpResult, OpError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self
            .commands
            .as_ref()
            .map(|list| list.state.read(cx).query(cx).to_string())
            .unwrap_or_default();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) = &result
                    && let Some(list) = this.commands.as_mut()
                {
                    list.failed = Some(error.to_string().into());
                    cx.notify();
                    return;
                }
                this.search_saved(&query, window, cx);
            });
        })
        .detach();
    }

    fn send_saved(&mut self, id: SavedPromptId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread_id) = self.shown_thread(cx) else {
            if let Some(list) = self.commands.as_mut() {
                list.failed = Some(NO_THREAD.into());
            }
            cx.notify();
            return;
        };
        let task = Workspace::dispatch(Op::SavedPrompt(SavedPromptOp::Send { id, thread_id }), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => {
                    this.close_commands(window, cx);
                }
                Err(error) => {
                    if let Some(list) = this.commands.as_mut() {
                        list.failed = Some(error.to_string().into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

pub(super) fn view(list: &CommandList, this: &WeakEntity<Kagan>) -> Command {
    if let Some(draft) = list.draft.as_ref() {
        return draft_form(list, draft, this);
    }
    prompt_list(list, this)
}

fn prompt_list(list: &CommandList, this: &WeakEntity<Kagan>) -> Command {
    let prompts = Rc::new(list.prompts.clone());
    let opened = prompts.clone();
    let sending = this.clone();
    let searched = this.clone();
    let adding = this.clone();
    let empty = list
        .failed
        .clone()
        .unwrap_or_else(|| "No saved prompts.".into());
    let notice = list.failed.clone();
    let mut command = Command::new(&list.state)
        .filterable(false)
        .placeholder("Search saved prompts")
        .header(move |_, _, cx| notice_line(notice.clone(), cx))
        .empty(move |_, _, cx| {
            div()
                .id("saved-prompts-empty")
                .ui_container()
                .aria_label(empty.clone())
                .p(px(space(SpaceStep::Sm, cx)))
                .text_color(cx.theme().muted_foreground)
                .child(empty.clone())
        })
        .on_query(move |query, window, cx| {
            _ = searched.update(cx, |this, cx| this.search_saved(query, window, cx));
        })
        .on_confirm(move |index: IndexPath, window, cx| {
            let Some(prompt) = opened.get(index.row).cloned() else {
                return;
            };
            _ = sending.update(cx, |this, cx| this.send_saved(prompt.id, window, cx));
        })
        .footer(move |_, _, cx| {
            let owner = adding.clone();
            div().p(px(space(SpaceStep::Sm, cx))).child(
                div()
                    .id("saved-prompt-new")
                    .ui_observe_as(Facts::button("New saved prompt"))
                    .child(
                        Button::new("saved-prompt-new-button")
                            .small()
                            .label("New saved prompt")
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                _ = owner.update(cx, |this, cx| this.begin_draft(window, cx));
                            }),
                    ),
            )
        });
    command = command.group(
        CommandGroup::new()
            .label("Saved prompts")
            .items(prompts.iter().cloned().map(|prompt| row(prompt, this))),
    );
    command
}

fn row(prompt: SavedPrompt, this: &WeakEntity<Kagan>) -> CommandItem {
    let id = prompt.id.clone();
    let star_id = id.clone();
    let delete_id = id.clone();
    let starring = this.clone();
    let deleting = this.clone();
    let starred = prompt.starred;
    CommandItem::new()
        .label(prompt.title.clone())
        .child(move |_, cx| {
            let gap = px(space(SpaceStep::Sm, cx));
            let muted = cx.theme().muted_foreground;
            let star_name = format!("saved-prompt-star-{}", star_id.0);
            let delete_name = format!("saved-prompt-delete-{}", delete_id.0);
            let star_label: SharedString = (if starred { "Unstar" } else { "Star" }).into();
            let star_value: SharedString = (if starred { "starred" } else { "unstarred" }).into();
            h_flex()
                .id(ElementId::Name(format!("saved-prompt-{}", id.0).into()))
                .ui_observe_as(Facts {
                    value: Some(prompt.body.clone().into()),
                    ..Facts::button(prompt.title.clone())
                })
                .w_full()
                .min_w_0()
                .gap(gap)
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .child(div().text_ellipsis().child(prompt.title.clone()))
                        .child(
                            div()
                                .type_small(cx)
                                .text_color(muted)
                                .text_ellipsis()
                                .child(prompt.body.clone()),
                        ),
                )
                .child(
                    h_flex()
                        .gap(gap)
                        .flex_shrink_0()
                        .child(mark(
                            &star_name,
                            star_label,
                            star_value,
                            star_id.clone(),
                            starring.clone(),
                            !starred,
                        ))
                        .child(remove_button(
                            &delete_name,
                            delete_id.clone(),
                            deleting.clone(),
                        )),
                )
        })
}

fn mark(
    name: &str,
    label: SharedString,
    value: SharedString,
    id: SavedPromptId,
    this: WeakEntity<Kagan>,
    next: bool,
) -> gpui_kit::AnyElement {
    div()
        .id(ElementId::Name(name.to_owned().into()))
        .ui_observe_as(Facts {
            value: Some(value),
            ..Facts::button(label.clone())
        })
        .child(
            Button::new(format!("{name}-button"))
                .small()
                .ghost()
                .label(label)
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    let id = id.clone();
                    _ = this.update(cx, |this, cx| this.set_star(id, next, window, cx));
                }),
        )
        .into_any_element()
}

fn remove_button(name: &str, id: SavedPromptId, this: WeakEntity<Kagan>) -> gpui_kit::AnyElement {
    div()
        .id(ElementId::Name(name.to_owned().into()))
        .ui_observe_as(Facts::button("Delete"))
        .child(
            Button::new(format!("{name}-button"))
                .small()
                .ghost()
                .label("Delete")
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    let id = id.clone();
                    _ = this.update(cx, |this, cx| this.delete_saved(id, window, cx));
                }),
        )
        .into_any_element()
}

fn draft_form(list: &CommandList, draft: &Draft, this: &WeakEntity<Kagan>) -> Command {
    let title = draft.title.clone();
    let body = draft.body.clone();
    let notice = list.failed.clone();
    let saving = this.clone();
    let cancelling = this.clone();
    Command::new(&list.state)
        .searchable(false)
        .header(move |_, _, cx| {
            let gap = px(space(SpaceStep::Sm, cx));
            let title = title.clone();
            let body = body.clone();
            let body_text = body.read(cx).value().to_string();
            v_flex()
                .gap(gap)
                .p(gap)
                .child(notice_line(notice.clone(), cx))
                .child(
                    div()
                        .id("saved-prompt-title")
                        .ui_observe_as(Facts::input("Saved prompt title", &title, cx))
                        .child(Input::new(&title).small()),
                )
                .child(
                    div()
                        .id("saved-prompt-body")
                        .ui_observe_as(Facts {
                            role: gpui_kit::Role::TextInput,
                            name: "Saved prompt text".into(),
                            value: Some(body_text.into()),
                            disabled: false,
                            selected: None,
                            focus: Some(body.read(cx).focus_handle(cx)),
                            hint: None,
                        })
                        .h(px(space(SpaceStep::Xl, cx) * 5.))
                        .child(Textarea::new(&body).small()),
                )
        })
        .footer(move |_, _, cx| {
            let gap = px(space(SpaceStep::Sm, cx));
            let saving = saving.clone();
            let cancelling = cancelling.clone();
            h_flex()
                .gap(gap)
                .p(gap)
                .child(
                    div()
                        .id("saved-prompt-save")
                        .ui_observe_as(Facts::button("Save saved prompt"))
                        .child(
                            Button::new("saved-prompt-save-button")
                                .small()
                                .primary()
                                .label("Save")
                                .on_click(move |_, window, cx| {
                                    _ = saving.update(cx, |this, cx| this.save_draft(window, cx));
                                }),
                        ),
                )
                .child(
                    div()
                        .id("saved-prompt-cancel")
                        .ui_observe_as(Facts::button("Cancel saved prompt"))
                        .child(
                            Button::new("saved-prompt-cancel-button")
                                .small()
                                .ghost()
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    _ = cancelling.update(cx, |this, cx| {
                                        this.cancel_draft(window, cx);
                                    });
                                }),
                        ),
                )
        })
}

fn notice_line(notice: Option<SharedString>, cx: &App) -> gpui_kit::AnyElement {
    let Some(notice) = notice else {
        return div().into_any_element();
    };
    div()
        .id("saved-prompt-notice")
        .ui_observe_as(Facts {
            role: gpui_kit::Role::Status,
            name: "Saved prompt notice".into(),
            value: Some(notice.clone()),
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        })
        .text_color(cx.theme().danger)
        .child(notice)
        .into_any_element()
}
