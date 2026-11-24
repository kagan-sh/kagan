use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Focusable as _, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, Styled as _, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{ThreadId, ThreadStatus};

use super::ChatView;
use crate::kagan::controls::{control, dispatch};
use crate::kagan::{SaveDiagnostics, StartAgain};
use crate::theme::{Tone, TypeScale as _, control_height, radius_xs};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// The one column the transcript, the cards and the prompt box share.
pub(super) const COLUMN: f32 = 760.;

/// What an empty prompt box says: how to send, or how to get back to sending.
const ASK: &str = "Ask the agent… Enter sends, Shift+Enter adds a line";
const STOPPED: &str = "Start the agent again to send";
const ARCHIVED: &str = "This thread is archived";

/// What the box draws this frame, read once so every part of it agrees.
struct Shown {
    turn: Turn,
    ready: bool,
    focused: bool,
    archived: bool,
}

/// Where the shown run is: waiting for a prompt, answering one, or stopped.
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

/// The prompt box: the cards over it, the box itself, and one quiet row under it that says where
/// this runs and what the agent is doing.
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
        ready: running && !empty && !view.sending,
        focused: view.prompt.read(cx).focus_handle(cx).is_focused(window),
        archived: view.archived(cx),
    };
    // The box's hint says what it takes, or how to get the agent back, so a stopped box never
    // looks like it can send. Read-only, the area keeps the draft and takes no typing.
    let hint = match (shown.turn.running(), shown.archived) {
        (true, _) => ASK,
        (false, true) => ARCHIVED,
        (false, false) => STOPPED,
    };
    let stale = view.prompt.read(cx).presentation().placeholder().as_ref() != hint;
    if stale {
        view.prompt
            .update(cx, |prompt, cx| prompt.set_placeholder(hint, window, cx));
    }
    let cards = super::above::stack(view, window, cx);
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
                .child(boxed(view, &shown, cx))
                .child(context_row(view, answering, cx)),
        )
        .into_any_element()
}

/// The one rounded box: the text area, what happened when it stopped, and the bar under it.
fn boxed(view: &mut ChatView, shown: &Shown, cx: &mut Context<ChatView>) -> AnyElement {
    let border = if shown.focused {
        cx.theme().accent
    } else {
        cx.theme().input
    };
    let stopped = (!shown.turn.running()).then(|| {
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
        .child(area(view, !shown.turn.running(), cx))
        .children(stopped.map(|line| {
            div()
                .id("chat-stopped")
                .px_3()
                .pb_1()
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
                .type_small(cx)
                .text_color(cx.theme().danger)
                .child(notice)
        }))
        .child(toolbar(view, shown, cx))
        .into_any_element()
}

/// The text area. What to do sits in its own placeholder, so the box draws one thing.
fn area(view: &ChatView, stopped: bool, cx: &App) -> impl IntoElement {
    let value = view.prompt.read(cx).value().to_string();
    div().relative().child(
        div()
            .id("chat-prompt")
            .ui_observe_as(Facts {
                role: Role::TextInput,
                name: "Prompt".into(),
                value: Some(value.into()),
                disabled: stopped,
                selected: None,
                focus: Some(view.prompt.read(cx).focus_handle(cx)),
                hint: None,
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

/// The bar inside the box: what a plugin adds, the way on that stays quiet, and the one action
/// this state offers.
fn toolbar(view: &mut ChatView, shown: &Shown, cx: &mut Context<ChatView>) -> AnyElement {
    let quiet = if shown.turn.running() || shown.archived {
        Vec::new()
    } else {
        stopped_actions(view, cx)
    };
    let action = action(view, shown, cx);
    h_flex()
        .items_center()
        .gap_2()
        .px_2()
        .pb_2()
        .children(crate::kagan::header::prompt(cx))
        .children(quiet)
        .child(div().flex_1())
        .children(action)
        .into_any_element()
}

/// Ways on that stay available but quiet, beside the one obvious action.
fn stopped_actions(view: &ChatView, cx: &App) -> Vec<AnyElement> {
    let mut actions = Vec::new();
    let thread = view.thread.clone();
    if crate::kagan::start::Reuse::of(&thread, cx).is_some() {
        let kagan = view.kagan.clone();
        actions.push(
            control(
                "chat-new-thread",
                Facts::button("New thread here…"),
                Button::new("chat-new-thread-button")
                    .small()
                    .ghost()
                    .label("New thread here…")
                    .tooltip("Start a new thread in this folder")
                    .on_click(move |_, window, cx| {
                        let thread = thread.clone();
                        _ = kagan.update(cx, |kagan, cx| {
                            kagan.new_thread_here(&thread, window, cx);
                        });
                    }),
            )
            .into_any_element(),
        );
    }
    if Workspace::global(cx).read(cx).recovery(&view.thread) {
        actions.push(
            control(
                "chat-save-diagnostics",
                Facts::button("Save diagnostics"),
                Button::new("chat-save-diagnostics-button")
                    .small()
                    .ghost()
                    .label("Save diagnostics")
                    .tooltip("Save what this run printed, for a bug report")
                    .on_click(dispatch(SaveDiagnostics)),
            )
            .into_any_element(),
        );
    }
    actions
}

/// The one action, where Send sits: Send while the agent waits, a round stop button while it
/// answers, Start again after it stops, so the hand learns one place.
fn action(view: &mut ChatView, shown: &Shown, cx: &mut Context<ChatView>) -> Option<AnyElement> {
    if shown.archived {
        return None;
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
        let kagan = view.kagan.clone();
        return Some(
            control(
                "chat-start-again",
                Facts::button("Start again"),
                Button::new("chat-start-again-button")
                    .small()
                    .primary()
                    .label("Start again")
                    .tooltip("Start the agent again in this thread")
                    .on_click(move |_, window, cx| {
                        _ = kagan.update(cx, |kagan, cx| {
                            kagan.start_again(&StartAgain, window, cx);
                        });
                    }),
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
                .tooltip("Send this prompt (Enter)")
                .disabled(!shown.ready)
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
        )
        .into_any_element(),
    )
}

/// Under the box, like bb's footer: where this runs and what the agent is doing, in one quiet row.
fn context_row(view: &ChatView, answering: bool, cx: &App) -> impl IntoElement {
    let (project, folder) = context(&view.thread, cx);
    let (state, tone) = state_word(view, answering, cx);
    h_flex()
        .id("chat-context")
        .ui_container()
        .items_center()
        .gap_2()
        .px_1()
        .type_caption(cx)
        .text_color(cx.theme().muted_foreground)
        .children(project.map(|project| {
            chip(
                "chat-context-project",
                "Project",
                IconName::Folder,
                project,
                cx,
            )
        }))
        .children(
            folder.map(|(icon, folder)| chip("chat-context-folder", "Folder", icon, folder, cx)),
        )
        .child(div().flex_1())
        .child(
            div()
                .id("chat-state")
                .ui_observe_as(Facts::label("State", state.clone()))
                .text_color(tone)
                .child(state),
        )
}

/// One quiet chip. It states a fact, so it takes no focus.
fn chip(id: &'static str, name: &str, icon: IconName, text: SharedString, cx: &App) -> AnyElement {
    h_flex()
        .id(id)
        .h(px(control_height(cx)))
        .px_2()
        .gap_1()
        .items_center()
        .rounded(px(radius_xs(cx)))
        .border_1()
        .border_color(cx.theme().border)
        .text_color(cx.theme().muted_foreground)
        .ui_observe_as(Facts::label(name, text.clone()))
        .child(Icon::new(icon).xsmall())
        .child(div().max_w(px(240.)).text_ellipsis().child(text))
        .into_any_element()
}

/// The project's name, and the folder the run writes to: a worktree's branch, or the project's own.
fn context(
    thread: &ThreadId,
    cx: &App,
) -> (Option<SharedString>, Option<(IconName, SharedString)>) {
    let workspace = Workspace::global(cx);
    let workspace = workspace.read(cx);
    let Some(record) = workspace
        .thread(thread)
        .map(|thread| thread.read(cx).record())
    else {
        return (None, None);
    };
    let project = workspace
        .project(&record.project_id)
        .map(|project| SharedString::from(project.read(cx).record().name.clone()));
    let folder = match record.environment_id.as_ref() {
        Some(id) => workspace.environment(id).map(|environment| {
            let environment = environment.read(cx).record();
            let name = environment.branch.clone().unwrap_or_else(|| {
                environment
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            (IconName::GitBranch, SharedString::from(name))
        }),
        None => Some((IconName::HardDrive, SharedString::from("Project folder"))),
    };
    (project, folder)
}

/// What the agent is doing, in the words the thread row uses, with the tone that says it.
fn state_word(view: &ChatView, answering: bool, cx: &App) -> (SharedString, Hsla) {
    let muted = cx.theme().muted_foreground;
    if view.sending {
        return ("Sending…".into(), muted);
    }
    if answering {
        return ("Working".into(), Tone::Busy.color(cx));
    }
    match view.thread_state(cx) {
        Some((ThreadStatus::Stopping, _)) => ("Stopping".into(), Tone::Busy.color(cx)),
        Some((status, _)) if status.running() => ("Waiting for you".into(), muted),
        _ => ("Not running".into(), muted),
    }
}
