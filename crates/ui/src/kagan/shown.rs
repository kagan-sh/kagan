use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, AppContext as _, Context, DismissEvent, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
};
use kg_core::{Color, Workspace};
use kg_domain::{Op, Thread, ThreadId, ThreadNotify, ThreadOp};
use kg_plugins::RunCommand;

use super::controls::{bound_keys, control, dispatch, menu_item, menu_popup, state_mark, tip};
use super::{
    AddProject, Kagan, NewThread, RenameThread, SaveDiagnostics, StartAgain, StopThread,
    ThreadActions,
};
use crate::terminal::TerminalView;
use crate::theme::terminal_color;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const WELCOME_MARK_FADE: f32 = 0.3;

fn ended_text(id: &ThreadId, cx: &gpui_kit::App) -> String {
    let workspace = Workspace::global(cx);
    let workspace = workspace.read(cx);
    let mut line = workspace
        .end_line(id)
        .unwrap_or_else(|| "The program is not running.".to_owned());
    if let Some(note) = workspace.screen_note(id) {
        line.push(' ');
        line.push_str(note);
    }
    line
}

#[derive(Default)]
pub(super) struct TitleState {
    pub(super) menu: Option<(Entity<PopupMenu>, Subscription)>,
    pub(super) rename: Option<Entity<InputState>>,
    pub(super) rename_watch: Option<Subscription>,
}

#[derive(Clone, Copy)]
pub(super) enum ThreadPick {
    Pin,
    Notify(ThreadNotify),
    Archive,
    Rename,
    SaveDiagnostics,
}

const NOTIFY_CHOICES: [(ThreadNotify, &str, &str); 4] = [
    (ThreadNotify::Default, "thread-notify-default", "Default"),
    (ThreadNotify::All, "thread-notify-all", "All"),
    (
        ThreadNotify::Asks,
        "thread-notify-asks",
        "Only when it asks me",
    ),
    (ThreadNotify::Muted, "thread-notify-muted", "Muted"),
];

// The title bar and a sidebar row show the same menu. `tail` adds items after Rename.
pub(super) fn thread_menu(
    thread: &Thread,
    on_pick: impl Fn(ThreadPick, &mut Window, &mut App) + Clone + 'static,
    tail: impl FnOnce(PopupMenu) -> PopupMenu + 'static,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PopupMenu> {
    let pin = if thread.pinned { "Unpin" } else { "Pin" };
    let notify = thread.notify;
    let on_notify = on_pick.clone();
    let item = move |pick: ThreadPick, id: &'static str, label: &'static str| {
        let on_pick = on_pick.clone();
        PopupMenuItem::element(move |_, _| menu_item(id, label))
            .on_click(move |_, window, cx| on_pick(pick, window, cx))
    };
    PopupMenu::build(window, cx, move |menu, window, cx| {
        let menu = menu
            .item(item(ThreadPick::Pin, "thread-pin", pin))
            .separator()
            .item(item(ThreadPick::Archive, "thread-archive", "Archive"))
            .separator()
            .item(item(ThreadPick::Rename, "thread-rename", "Rename…"))
            .separator()
            .submenu("Notifications", window, cx, move |menu, _, _| {
                NOTIFY_CHOICES
                    .into_iter()
                    .fold(menu, |menu, (level, id, label)| {
                        let on_pick = on_notify.clone();
                        menu.item(
                            PopupMenuItem::element(move |_, _| menu_item(id, label))
                                .checked(level == notify)
                                .on_click(move |_, window, cx| {
                                    on_pick(ThreadPick::Notify(level), window, cx);
                                }),
                        )
                    })
            });
        tail(menu).separator().item(item(
            ThreadPick::SaveDiagnostics,
            "save-diagnostics",
            "Save diagnostics",
        ))
    })
}

impl Kagan {
    pub(super) fn shown_open_thread(&self, cx: &App) -> Option<Thread> {
        let id = self.shown_thread(cx)?;
        let thread = self.record(&id, cx)?.clone();
        thread.archived_at.is_none().then_some(thread)
    }

    pub(super) fn open_thread_actions(
        &mut self,
        _: &ThreadActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.title.menu.is_some() {
            return;
        }
        // Settings covers the terminal, so a thread only has a menu when it is visibly shown.
        let Some(thread) = self.shown_open_thread(cx) else {
            return;
        };
        self.sidebar
            .update(cx, |sidebar, _| sidebar.close_row_menu());
        let kagan = cx.entity().downgrade();
        let id = thread.id.clone();
        let focus = self.focus.clone();
        let plugin_commands = super::header::menu(cx);
        let menu = thread_menu(
            &thread,
            move |pick, window, cx| {
                _ = kagan.update(cx, |kagan, cx| kagan.thread_pick(&id, pick, window, cx));
            },
            move |menu| {
                plugin_commands
                    .into_iter()
                    .fold(menu.action_context(focus), |menu, command| {
                        let run = RunCommand::new(command.plugin.clone(), command.id.clone());
                        let row = super::header::menu_id_for(&command.plugin, &command.id);
                        let title = command.title;
                        menu.separator().menu_element(Box::new(run), move |_, _| {
                            control(
                                row.clone(),
                                Facts {
                                    role: Role::MenuItem,
                                    ..Facts::button(title.clone())
                                },
                                title.clone(),
                            )
                        })
                    })
            },
            window,
            cx,
        );
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.close_thread_actions(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.title.menu = Some((menu, dismiss));
        cx.notify();
    }

    pub(super) fn dismiss_thread_menus(&mut self, cx: &mut Context<Self>) {
        self.title.menu = None;
        self.sidebar
            .update(cx, |sidebar, _| sidebar.close_row_menu());
    }

    pub(super) fn close_thread_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.title.menu = None;
        self.focus_work(window, cx);
        cx.notify();
    }

    pub(super) fn begin_rename(
        &mut self,
        _: &RenameThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.shown_open_thread(cx) else {
            return;
        };
        self.dismiss_thread_menus(cx);
        let title = thread.title.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        input.update(cx, |input, cx| input.select_all(window, cx));
        let watch = cx.subscribe_in(
            &input,
            window,
            |this, input, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_rename(input, window, cx),
                InputEvent::Blur => {
                    cx.spawn(async move |this, cx| {
                        _ = this.update(cx, Kagan::cancel_rename);
                    })
                    .detach();
                }
                InputEvent::Change | InputEvent::Focus => {}
            },
        );
        window.focus(&input.focus_handle(cx), cx);
        self.title.rename = Some(input);
        self.title.rename_watch = Some(watch);
        cx.notify();
    }

    fn commit_rename(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.shown_thread(cx) else {
            return;
        };
        let title = input.read(cx).value().to_string();
        self.title.rename = None;
        self.focus_work(window, cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            _ = this.update(cx, |this, _| this.title.rename_watch = None);
        })
        .detach();
        self.run_then(
            Op::Thread(ThreadOp::Rename { id, title }),
            cx,
            |this, result, _, cx| {
                this.note = match result {
                    Ok(_) => None,
                    Err(error) => Some(format!("Could not rename the thread: {error}").into()),
                };
                cx.notify();
            },
        );
    }

    pub(super) fn close_rename(&mut self) {
        self.title.rename = None;
        self.title.rename_watch = None;
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        if self.title.rename.is_none() {
            return;
        }
        self.close_rename();
        cx.notify();
    }

    fn cancel_rename_to_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.title.rename.is_none() {
            return;
        }
        self.close_rename();
        self.focus_work(window, cx);
        cx.notify();
    }

    // A click on the title does not start a rename.
    pub(super) fn title_line(&self, thread: &Thread, cx: &mut Context<Self>) -> AnyElement {
        let Some(input) = &self.title.rename else {
            let title = div()
                .id("thread-title")
                .ui_container()
                .flex_initial()
                .min_w_0()
                .text_ellipsis()
                .type_body(cx)
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .child(thread.title.clone());
            if thread.notify != ThreadNotify::Muted {
                return title.into_any_element();
            }
            return h_flex()
                .flex_initial()
                .min_w_0()
                .gap_1()
                .child(title)
                .child(state_mark(
                    "title-muted".into(),
                    gpui_kit::assets::IconName::BellMinus,
                    "Notifications muted",
                    cx,
                ))
                .into_any_element();
        };
        div()
            .id("thread-title-input")
            .flex_1()
            .min_w_0()
            .key_context("ThreadTitle")
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.cancel_rename_to_terminal(window, cx);
            }))
            .ui_observe_as(Facts::input("Thread title", input, cx))
            .child(Input::new(input).text_size(gpui_kit::px(crate::theme::type_body(cx))))
            .into_any_element()
    }

    fn actions_trigger(&self, cx: &App) -> impl IntoElement {
        div()
            .relative()
            .child(control(
                "thread-actions-trigger",
                Facts::button("Thread actions"),
                Button::new("thread-actions-control")
                    .small()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::Ellipsis)
                    .accessibility_label("Thread actions")
                    .tooltip(tip("Thread actions", &ThreadActions, cx))
                    .on_click(dispatch(ThreadActions)),
            ))
            .when_some(self.title.menu.as_ref(), |wrap, (menu, _)| {
                wrap.child(menu_popup(
                    menu,
                    "thread-menu-layer",
                    "popup-menu",
                    "Thread actions",
                ))
            })
    }

    fn ended_notice(id: &ThreadId, archived: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let line = ended_text(id, cx);
        let recovery = Workspace::global(cx).read(cx).recovery(id);
        // Offered only where the new-thread screen can run in the folder, by the same test.
        let reusable = !archived && crate::kagan::start::Reuse::of(id, cx).is_some();
        h_flex()
            .id("ended")
            .w_full()
            .bg(terminal_color(Color::Background, cx))
            .gap_3()
            .px_4()
            .h_9()
            .type_body(cx)
            .text_color(cx.theme().foreground)
            .ui_observe_as(Facts {
                role: Role::Label,
                value: Some(line.clone().into()),
                ..Facts::button("Ended")
            })
            .child(div().min_w_0().flex_1().text_ellipsis().child(line))
            .when(!archived, |row| {
                row.child(control(
                    "start-again",
                    Facts::button("Start again"),
                    Button::new("start-again-button")
                        .small()
                        .primary()
                        .label("Start again")
                        .on_click(dispatch(StartAgain)),
                ))
            })
            .when(reusable, |row| {
                let thread = id.clone();
                row.child(control(
                    "new-thread-here",
                    Facts::button("New thread here…"),
                    Button::new("new-thread-here-button")
                        .small()
                        .outline()
                        .label("New thread here…")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.new_thread_here(&thread, window, cx);
                        })),
                ))
            })
            .when(recovery, |row| {
                row.child(control(
                    "save-diagnostics-ended",
                    Facts::button("Save diagnostics"),
                    Button::new("save-diagnostics-ended-button")
                        .small()
                        .outline()
                        .label("Save diagnostics")
                        .on_click(dispatch(SaveDiagnostics)),
                ))
            })
    }

    pub(super) fn thread_controls(&self, thread: &Thread, cx: &mut Context<Self>) -> AnyElement {
        let archived = thread.archived_at.is_some();
        let live = thread.status.running();
        h_flex()
            .gap_2()
            .children(super::header::buttons(cx))
            .when(live, |header| {
                let tip = super::controls::tip("Stop", &StopThread, cx);
                header.child(control(
                    "stop",
                    Facts::button("Stop"),
                    Button::new("stop-button")
                        .small()
                        .ghost()
                        .label("Stop")
                        .tooltip(tip)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.stop(&StopThread, window, cx)),
                        ),
                ))
            })
            .when(!archived, |header| header.child(self.actions_trigger(cx)))
            .when(archived, |header| {
                header.child(control(
                    "restore",
                    Facts::button("Restore"),
                    Button::new("restore-button")
                        .small()
                        .ghost()
                        .label("Restore")
                        .on_click(cx.listener(|this, _, window, cx| this.restore(window, cx))),
                ))
            })
            .into_any_element()
    }

    pub(super) fn render_thread(
        &self,
        view: &Entity<TerminalView>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = view.read(cx).thread().clone();
        let Some(thread) = self.record(&id, cx).cloned() else {
            return Self::render_empty("This thread is gone.", cx);
        };
        let archived = thread.archived_at.is_some();
        let live = thread.status.running();
        // The terminal stays on screen after its program ends, so the last output stays readable.
        let strip = self.render_strip(cx);
        let ended = (!live).then(|| Self::ended_notice(&id, archived, cx));
        v_flex()
            .size_full()
            .children(strip)
            .child(div().flex_1().min_h_0().child(view.clone()))
            .children(ended)
            .into_any_element()
    }

    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .pb(gpui_kit::px(80.))
            .gap_3()
            .child(
                div()
                    .id("welcome-mark")
                    .ui_container()
                    .mb_2()
                    .child(crate::brand::mark(
                        cx.theme().muted_foreground.opacity(WELCOME_MARK_FADE),
                        64.,
                    )),
            )
            .child(
                div()
                    .id("welcome-title")
                    .ui_container()
                    .type_title(cx)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child("Run your agent in a thread"),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(control(
                        "new-personal-thread",
                        Facts::button("New Personal thread"),
                        Button::new("new-personal-thread-button")
                            .primary()
                            .label("New Personal thread")
                            .child(welcome_keycap("welcome-new-keycap", &NewThread, cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.new_personal_thread(window, cx);
                            })),
                    ))
                    .child(control(
                        "welcome-add-project",
                        Facts::button("Add project…"),
                        Button::new("welcome-add-project-button")
                            .outline()
                            .label("Add project…")
                            .child(welcome_keycap("welcome-add-keycap", &AddProject, cx))
                            .on_click(dispatch(AddProject)),
                    )),
            )
            .children(super::setup_guide::panel(self, cx))
            .into_any_element()
    }

    pub(super) fn render_empty(text: &'static str, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .type_body(cx)
            .text_color(cx.theme().muted_foreground)
            .child(crate::brand::mark(crate::theme::brand_color(cx), 40.))
            .child(text)
            .into_any_element()
    }
}

fn welcome_keycap(id: &'static str, action: &dyn Action, cx: &App) -> impl IntoElement {
    let text = bound_keys(action, cx);
    div()
        .id(id)
        .ui_container()
        .aria_label(SharedString::from(text.clone()))
        .text_color(cx.theme().muted_foreground)
        .child(text)
}
