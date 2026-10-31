use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, actions, div,
};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentId, EnvironmentOp, EnvironmentProvider, Op, OpResult, Thread, ThreadOp, Visibility,
};

use super::Kagan;
use super::shown::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

actions!(archived, [OpenArchivedRow]);

pub(super) enum List {
    Loading,
    Listed(Vec<Thread>),
}

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", OpenArchivedRow, Some("ArchivedRow")),
        KeyBinding::new("space", OpenArchivedRow, Some("ArchivedRow")),
    ]
}

pub(super) fn watch(query: &Entity<InputState>, cx: &mut Context<Kagan>) -> Subscription {
    cx.subscribe(query, |_, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
    })
}

impl Kagan {
    pub(super) fn show_archived(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen.remember_center(window, cx);
        self.cancel_plugin_start();
        self.dismiss_thread_menus(cx);
        self.close_rename();
        self.open_center(super::screen::Center::Archived(List::Loading), window, cx);
        self.load_retired(cx);
        window.focus(&self.archive_query.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn load_retired(&mut self, cx: &mut Context<Self>) {
        let listed = Workspace::dispatch(
            Op::Thread(ThreadOp::List {
                project_id: None,
                archived: Some(true),
                limit: None,
                before: None,
            }),
            cx,
        );
        self.retired_load = cx.spawn(async move |this, cx| {
            let Ok(OpResult::Threads(mut threads)) = listed.await else {
                return;
            };
            threads.retain(|thread| {
                thread.visibility == Visibility::Visible && thread.archived_at.is_some()
            });
            threads.sort_by(|left, right| {
                right
                    .created_at
                    .cmp(&left.created_at)
                    .then_with(|| right.id.cmp(&left.id))
            });
            _ = this.update(cx, |this, cx| {
                if !this.shows_archived() {
                    return;
                }
                this.screen.list_archived(threads);
                cx.notify();
            });
        });
    }

    pub(super) fn shows_archived(&self) -> bool {
        matches!(self.screen.current(), super::screen::Center::Archived(_))
    }

    fn close_archived(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shows_archived() {
            self.screen.close();
            self.return_focus(super::screen::FocusReturn::Center, window, cx);
            cx.notify();
        }
    }
}

pub(super) fn list(
    kagan: &Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> impl IntoElement {
    let query = kagan.archive_query.read(cx).value().trim().to_lowercase();
    let threads = match kagan.screen.current() {
        super::screen::Center::Archived(List::Listed(threads)) => threads.as_slice(),
        _ => &[],
    };
    let matched: Vec<&Thread> = threads
        .iter()
        .filter(|thread| query.is_empty() || thread.title.to_lowercase().contains(&query))
        .collect();
    let shown = kagan.shown_thread(cx);
    let rows: Vec<AnyElement> = matched
        .iter()
        .map(|thread| {
            retired_row(thread, shown.as_ref() == Some(&thread.id), window, cx).into_any_element()
        })
        .collect();
    let ready = matches!(
        kagan.screen.current(),
        super::screen::Center::Archived(List::Listed(_))
    );
    let body = if !ready {
        div().into_any_element()
    } else if threads.is_empty() {
        note(cx, "archived-empty", "No archived threads.")
    } else if matched.is_empty() {
        note(cx, "archived-empty", "No archived threads match.")
    } else {
        v_flex().px_4().gap_0p5().children(rows).into_any_element()
    };
    v_flex()
        .size_full()
        .min_h_0()
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.close_archived(window, cx);
        }))
        .child(
            h_flex().justify_end().px_4().pt_2().child(control(
                "archived-close",
                Facts::button("Close"),
                Button::new("archived-close-control")
                    .small()
                    .ghost()
                    .label("Close")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_archived(window, cx);
                    })),
            )),
        )
        .child(
            div()
                .id("archived-search")
                .ui_observe_as(Facts::input(
                    "Search archived threads",
                    &kagan.archive_query,
                    cx,
                ))
                .px_4()
                .pt_4()
                .pb_2()
                .max_w(gpui_kit::rems(40.))
                .child(
                    Input::new(&kagan.archive_query)
                        .text_size(gpui_kit::px(crate::theme::TYPE_BODY))
                        .prefix(IconName::Search),
                ),
        )
        .child(note(
            cx,
            "archived-note",
            "Restoring brings the thread back. The program stays stopped.",
        ))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .ui_container()
                .overflow_y_scrollbar()
                .id("archived-list")
                .py_2()
                .child(body),
        )
}

fn note(cx: &App, id: &'static str, text: &'static str) -> AnyElement {
    div()
        .id(id)
        .ui_container()
        .aria_label(text)
        .px_4()
        .py_2()
        .type_small()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

fn retired_row(
    thread: &Thread,
    selected: bool,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> impl IntoElement {
    let id = thread.id.clone();
    let open = id.clone();
    let restore = id.clone();
    let chosen = id.clone();
    let title: SharedString = thread.title.clone().into();
    let focus = super::tab_stop(window, cx, SharedString::from(format!("archived-row-{id}")));
    let removal = removal_of(thread, cx);
    v_flex()
        .w_full()
        .gap_1()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id(ElementId::Name(format!("thread-{id}").into()))
                        .key_context("ArchivedRow")
                        .role(Role::TreeItem)
                        .aria_label(title.clone())
                        .aria_selected(selected)
                        .flex_1()
                        .min_w_0()
                        .h_7()
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded(cx.theme().radius)
                        .type_body()
                        .text_ellipsis()
                        .when(selected, |row| {
                            row.font_medium().bg(cx.theme().sidebar_accent)
                        })
                        .when(!selected, |row| {
                            row.hover(|row| row.bg(cx.theme().list_hover))
                        })
                        .focus(|row| row.border_1().border_color(cx.theme().ring))
                        .ui_observe_as(Facts {
                            role: Role::TreeItem,
                            selected: Some(selected),
                            focus: Some(focus),
                            ..Facts::button(title.clone())
                        })
                        .on_action(cx.listener(move |this, _: &OpenArchivedRow, window, cx| {
                            this.select(open.clone(), window, cx);
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select(chosen.clone(), window, cx);
                        }))
                        .child(title),
                )
                .child(control(
                    ElementId::Name(format!("restore-{restore}").into()),
                    Facts::button("Restore"),
                    Button::new(SharedString::from(format!("restore-control-{restore}")))
                        .small()
                        .ghost()
                        .label("Restore")
                        .on_click(cx.listener(move |_this, _, window, cx| {
                            Kagan::run(
                                ThreadOp::Unarchive {
                                    id: restore.clone(),
                                },
                                "restore the thread",
                                window,
                                cx,
                            );
                        })),
                )),
        )
        .when_some(removal, |row, (environment, message)| {
            row.child(removal_line(&environment, &id, message, cx))
        })
}

fn removal_of(thread: &Thread, cx: &App) -> Option<(EnvironmentId, String)> {
    let id = thread.environment_id.as_ref()?;
    let environment = Workspace::global(cx).read(cx).environment(id)?.clone();
    let record = environment.read(cx).record();
    if record.provider != EnvironmentProvider::Worktree {
        return None;
    }
    record.error.clone().map(|error| (record.id.clone(), error))
}

fn removal_line(
    environment: &EnvironmentId,
    thread: &kg_domain::ThreadId,
    message: String,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let retry = environment.clone();
    v_flex()
        .w_full()
        .gap_1()
        .px_2()
        .items_start()
        .child(
            div()
                .id(ElementId::Name(format!("removal-{thread}").into()))
                .ui_container()
                .aria_label(message.clone())
                .w_full()
                .type_body()
                .text_color(cx.theme().danger)
                .child(message),
        )
        .child(control(
            ElementId::Name(format!("remove-again-{environment}").into()),
            Facts::button("Remove again"),
            Button::new(SharedString::from(format!(
                "remove-again-control-{environment}"
            )))
            .small()
            .outline()
            .label("Remove again")
            .on_click(cx.listener(move |_this, _, window, cx| {
                remove_again(retry.clone(), window, cx);
            })),
        ))
        .into_any_element()
}

fn remove_again(id: EnvironmentId, window: &mut Window, cx: &mut Context<Kagan>) {
    let task = Workspace::dispatch(Op::Environment(EnvironmentOp::Remove { id }), cx);
    let window = window.window_handle();
    cx.spawn(async move |this, cx| {
        let result = task.await;
        _ = cx.update_window(window, |_, window, cx| {
            _ = this.update(cx, |this, cx| match result {
                Ok(_) => this.refresh(window, cx),
                Err(error) => {
                    this.note = Some(format!("Could not remove the worktree: {error}").into());
                    cx.notify();
                }
            });
        });
    })
    .detach();
}
