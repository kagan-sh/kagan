use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, Placement, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{PendingInteraction, ThreadStatus};

use super::ChatView;
use super::card;
use crate::kagan::SaveDiagnostics;
use crate::kagan::controls::{control, dispatch, menu_item};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) const COLUMN: f32 = 760.;

const ARCHIVED: &str = "This thread is archived";

struct Shown {
    turn: Turn,
    ready: bool,
    focused: bool,
    archived: bool,
}

#[derive(Clone, Copy)]
enum Turn {
    Waiting,
    Answering,
    Stopped,
}

impl Turn {
    fn of(answering: bool, running: bool) -> Self {
        if answering {
            Self::Answering
        } else if running {
            Self::Waiting
        } else {
            Self::Stopped
        }
    }

    fn running(self) -> bool {
        !matches!(self, Self::Stopped)
    }
}

pub(super) fn composer(
    view: &mut ChatView,
    answering: bool,
    running: bool,
    window: &mut Window,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let prompt = view.prompt.read(cx).value().to_string();
    let empty = prompt.trim().is_empty();
    let shown = Shown {
        turn: Turn::of(answering, running),
        ready: running && !empty && view.sending.is_none(),
        focused: view.prompt.read(cx).focus_handle(cx).is_focused(window),
        archived: view.archived(cx),
    };
    // The box's hint says what it takes, or how to get the agent back, so a stopped box never
    // looks like it can send. Read-only, the area keeps the draft and takes no typing.
    let hint = match (shown.archived, shown.turn.running()) {
        (true, _) => ARCHIVED.to_owned(),
        (false, true) => format!("Ask {}…", view.who(cx)),
        (false, false) => format!(
            "{} Start it again to send.",
            crate::kagan::shown::ended_text(&view.thread, cx)
        ),
    };
    let stale = view.prompt.read(cx).presentation().placeholder().as_ref() != hint.as_str();
    if stale {
        view.prompt.update(cx, |prompt, cx| {
            prompt.set_placeholder(hint.clone(), window, cx);
        });
    }
    let cards = super::above::stack(view, window, cx);
    let place = match view.asked(cx) {
        Some((asked, total)) => {
            view.card_shown(&asked, window, cx);
            asked_card(view, asked, total, window, cx)
        }
        None => boxed(view, &shown, &hint, cx),
    };
    v_flex()
        .flex_none()
        .px_4()
        .pb_4()
        .child(
            v_flex()
                .max_w(px(COLUMN))
                .mx_auto()
                .w_full()
                .gap_2()
                .children(cards)
                .child(place)
                .children(context_row(view, cx)),
        )
        .into_any_element()
}

// A permission takes the box's place: one thing to do, in one place, and the draft stays kept.
fn asked_card(
    view: &ChatView,
    mut asked: PendingInteraction,
    total: usize,
    window: &mut Window,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    if total > 1 {
        asked.title = format!("{} · 1 of {total}", asked.title);
    }
    let (title, worktree) = view.approval_detail(&asked.id, cx);
    card::approval_card(
        &asked,
        title.as_deref(),
        worktree,
        &cx.weak_entity(),
        window,
        cx,
    )
}

fn boxed(view: &mut ChatView, shown: &Shown, hint: &str, cx: &mut Context<ChatView>) -> AnyElement {
    let border = if shown.focused {
        cx.theme().accent
    } else {
        cx.theme().input
    };
    // An empty stopped box says how it ended in its placeholder, under its own words. The line
    // shows only when a draft hides that placeholder.
    let empty = view.prompt.read(cx).value().trim().is_empty();
    let stopped = (!shown.turn.running() && !empty).then(|| {
        if shown.archived {
            "This thread is archived.".to_owned()
        } else {
            crate::kagan::shown::ended_text(&view.thread, cx)
        }
    });
    let notice = view.notice.clone();
    v_flex()
        .id("chat-box")
        .ui_container()
        .w_full()
        .border_1()
        .border_color(border)
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .child(area(view, !shown.turn.running(), hint, cx))
        .children(stopped.map(|line| {
            div()
                .id("chat-stopped")
                .px_3()
                .pb_1()
                .w_full()
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .ui_observe_as(Facts::label("Stopped", line.clone()))
                .child(line)
        }))
        .children(notice.map(|notice| {
            div()
                .id("chat-notice")
                .px_3()
                .pb_1()
                .w_full()
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().danger)
                .child(notice)
        }))
        .child(toolbar(view, shown, cx))
        .into_any_element()
}

fn area(view: &ChatView, stopped: bool, hint: &str, cx: &App) -> impl IntoElement {
    let value = view.prompt.read(cx).value().to_string();
    div().relative().overflow_hidden().child(
        div()
            .id("chat-prompt")
            .ui_observe_as(Facts {
                role: Role::TextInput,
                name: "Prompt".into(),
                value: Some(value.into()),
                disabled: stopped,
                selected: None,
                focus: Some(view.prompt.read(cx).focus_handle(cx)),
                hint: Some(hint.into()),
            })
            .child(
                Textarea::new(&view.prompt)
                    .bordered(false)
                    .appearance(false)
                    .readonly(stopped)
                    .text_size(px(crate::theme::type_body(cx)))
                    .small(),
            ),
    )
}

fn toolbar(view: &mut ChatView, shown: &Shown, cx: &mut Context<ChatView>) -> AnyElement {
    let menu = if shown.turn.running() || shown.archived {
        None
    } else {
        more(view, cx)
    };
    let action = action(view, shown, cx);
    h_flex()
        .items_center()
        .gap_2()
        .px_2()
        .pb_2()
        .children(crate::kagan::header::prompt(cx))
        .children(menu)
        .child(div().flex_1())
        .children(action)
        .into_any_element()
}

fn more(view: &ChatView, cx: &App) -> Option<AnyElement> {
    let thread = view.thread.clone();
    let reuse = crate::kagan::start::Reuse::of(&thread, cx).is_some();
    let recovery = Workspace::global(cx).read(cx).recovery(&thread);
    if !reuse && !recovery {
        return None;
    }
    let kagan = view.kagan.clone();
    Some(
        control(
            "chat-more",
            Facts::button("More"),
            Button::new("chat-more-button")
                .small()
                .ghost()
                .icon(IconName::Ellipsis)
                .accessibility_label("More")
                .tooltip("More")
                .tooltip_placement(Placement::Right)
                .dropdown_menu(move |menu, _, _| {
                    let menu = if reuse {
                        let thread = thread.clone();
                        let kagan = kagan.clone();
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                menu_item("chat-new-thread", "New thread here…")
                            })
                            .on_click(move |_, window, cx| {
                                let thread = thread.clone();
                                _ = kagan.update(cx, |kagan, cx| {
                                    kagan.new_thread_here(&thread, window, cx);
                                });
                            }),
                        )
                    } else {
                        menu
                    };
                    if recovery {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                menu_item("chat-save-diagnostics", "Save diagnostics")
                            })
                            .on_click(dispatch(SaveDiagnostics)),
                        )
                    } else {
                        menu
                    }
                }),
        )
        .into_any_element(),
    )
}

fn action(view: &mut ChatView, shown: &Shown, cx: &mut Context<ChatView>) -> Option<AnyElement> {
    if shown.archived {
        return None;
    }
    // A stop is on its way; the action place says so rather than repeating the state word.
    if matches!(view.thread_state(cx), Some((ThreadStatus::Stopping, _))) {
        return Some(
            control(
                "chat-stopping",
                Facts {
                    disabled: true,
                    ..Facts::button("Stopping…")
                },
                Button::new("chat-stopping-button")
                    .small()
                    .label("Stopping…")
                    .disabled(true),
            )
            .into_any_element(),
        );
    }
    if matches!(shown.turn, Turn::Answering) {
        return Some(
            control(
                "chat-cancel",
                Facts::button("Cancel"),
                Button::new("chat-cancel-button")
                    .small()
                    .ghost()
                    .icon(IconName::CircleStop)
                    .rounded(px(999.))
                    .accessibility_label("Cancel")
                    .tooltip("Cancel the agent's reply (Esc)")
                    .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))),
            )
            .into_any_element(),
        );
    }
    if !shown.turn.running() {
        return Some(
            control(
                "chat-start-again",
                Facts::button("Start again"),
                Button::new("chat-start-again-button")
                    .small()
                    .primary()
                    .label("Start again")
                    .tooltip("Start the agent again in this thread")
                    .on_click(cx.listener(|this, _, window, cx| this.start_again(window, cx))),
            )
            .into_any_element(),
        );
    }
    Some(
        control(
            "chat-send",
            Facts {
                disabled: !shown.ready,
                ..Facts::button("Send")
            },
            Button::new("chat-send-button")
                .small()
                .primary()
                .icon(IconName::ArrowUp)
                .rounded(px(999.))
                .accessibility_label("Send")
                .tooltip("Send (Enter). Shift+Enter adds a line.")
                .loading(view.sending.is_some())
                .disabled(!shown.ready)
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
        )
        .into_any_element(),
    )
}

// The facts under the box: where the agent writes, and what its context costs. Nothing here
// repeats the header, the box's own action, or each other, so no bar ever draws empty.
fn context_row(view: &ChatView, cx: &App) -> Option<AnyElement> {
    let branch = view.branch(cx);
    let usage = view.transcript.usage;
    if branch.is_none() && usage.is_none() {
        return None;
    }
    Some(
        h_flex()
            .id("chat-context")
            .ui_container()
            .items_center()
            .gap_2()
            .px_1()
            .type_caption(cx)
            .text_color(cx.theme().muted_foreground)
            .children(branch.map(|branch| {
                h_flex()
                    .id("chat-branch")
                    .ui_observe_as(Facts::label("Branch", branch.clone()))
                    .items_center()
                    .gap_1()
                    .child(Icon::new(IconName::GitBranch).xsmall())
                    .child(div().max_w(px(240.)).text_ellipsis().child(branch))
                    .into_any_element()
            }))
            .child(div().flex_1())
            .children(usage.map(|(used, size)| usage_gauge(used, size)))
            .into_any_element(),
    )
}

fn usage_gauge(used: u64, size: u64) -> AnyElement {
    let text = format!("{} / {}", ChatView::compact(used), ChatView::compact(size));
    let sentence = format!(
        "{} of {} tokens in context",
        ChatView::compact(used),
        ChatView::compact(size)
    );
    let tip = sentence.clone();
    div()
        .id("chat-usage")
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .ui_observe_as(Facts::label(sentence, text.clone()))
        .child(text)
        .into_any_element()
}
