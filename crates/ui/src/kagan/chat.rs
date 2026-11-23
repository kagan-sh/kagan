use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{Event, EventKind, Op, Role as Speaker, ThreadId, ThreadOp};

use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// The keys the chat adds: Escape cancels the agent's reply.
pub(crate) const CONTEXT: &str = "Chat";

/// One part of the transcript, named so a plugin can replace it later. Slice 1 draws messages and
/// notices; tool calls, plans and approvals arrive with the events that carry them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Message,
    Notice,
}

impl Part {
    fn of(kind: &EventKind) -> Option<Self> {
        match kind {
            EventKind::Message { .. } => Some(Self::Message),
            EventKind::Notice { .. } => Some(Self::Notice),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Notice => "notice",
        }
    }
}

/// One run of the transcript: a person's prompt, the agent's reply, or a notice.
struct Block {
    part: Part,
    role: Option<Speaker>,
    message_id: Option<String>,
    text: String,
}

impl Block {
    /// Adds a chunk to the block it belongs to, so an agent's chunks read as one message.
    fn merge(&mut self, event: &Event) -> bool {
        let EventKind::Message {
            role,
            text,
            message_id,
            ..
        } = &event.kind
        else {
            return false;
        };
        let same_message = match (message_id, &self.message_id) {
            (Some(now), Some(seen)) => now == seen,
            (None, None) => true,
            _ => false,
        };
        if self.role != Some(*role) || !same_message {
            return false;
        }
        self.text.push_str(text);
        true
    }
}

/// An ACP thread's transcript and prompt box: what the agent said, in order, and the one box that
/// answers it. The thread's events are the transcript, so nothing is kept here that the log lacks.
pub(crate) struct ChatView {
    thread: ThreadId,
    blocks: Vec<Block>,
    /// Whether the last thing read was a message, so only consecutive messages become one block.
    joining: bool,
    prompt: gpui_kit::Entity<TextareaState>,
    scroll: gpui_kit::ScrollHandle,
    focus: FocusHandle,
    notice: Option<SharedString>,
    sending: bool,
    _events: Subscription,
    _prompt: Subscription,
}

impl ChatView {
    pub(crate) fn new(thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 8));
        let prompt_watch = cx.subscribe_in(&prompt, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter {
                shift: false,
                secondary: false,
            } = event
            {
                this.submit(window, cx);
            }
        });
        let events = cx.subscribe(&Workspace::global(cx), |this, _, event, cx| {
            if let WorkspaceEvent::Event { event, .. } = event
                && event.thread_id == this.thread
            {
                this.absorb(event, cx);
            }
        });
        let mut this = Self {
            thread,
            blocks: Vec::new(),
            joining: false,
            prompt,
            scroll: gpui_kit::ScrollHandle::new(),
            focus: cx.focus_handle(),
            notice: None,
            sending: false,
            _events: events,
            _prompt: prompt_watch,
        };
        this.load(cx);
        this
    }

    pub(crate) fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Reads what the thread has said so far. The log is the transcript, so the view holds no
    /// second copy of it across a restart.
    fn load(&mut self, cx: &mut Context<Self>) {
        let read = Workspace::dispatch(
            Op::Thread(ThreadOp::Events {
                id: self.thread.clone(),
                after_seq: None,
                limit: None,
                order: None,
            }),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let Ok(kg_domain::OpResult::Events(events)) = read.await else {
                return;
            };
            _ = this.update(cx, |this, cx| {
                for event in events {
                    this.absorb(&event, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn absorb(&mut self, event: &Event, cx: &mut Context<Self>) {
        let Some(part) = Part::of(&event.kind) else {
            self.joining = false;
            return;
        };
        let merged = self.joining
            && self
                .blocks
                .last_mut()
                .is_some_and(|last| last.part == part && last.merge(event));
        if !merged {
            match &event.kind {
                EventKind::Message {
                    role,
                    text,
                    message_id,
                    ..
                } => self.blocks.push(Block {
                    part,
                    role: Some(*role),
                    message_id: message_id.clone(),
                    text: text.clone(),
                }),
                EventKind::Notice { text, .. } => self.blocks.push(Block {
                    part,
                    role: None,
                    message_id: None,
                    text: text.clone(),
                }),
                _ => return,
            }
        }
        self.joining = true;
        self.announce(cx);
    }

    fn announce(&mut self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        let text = self.prompt.read(cx).value().trim().to_owned();
        if text.is_empty() {
            self.notice = Some("Type a prompt.".into());
            cx.notify();
            return;
        }
        self.sending = true;
        let send = Workspace::dispatch(
            Op::Thread(ThreadOp::Input {
                id: self.thread.clone(),
                terminal: None,
                text,
            }),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = send.await;
            _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Ok(_) => {
                        this.notice = None;
                        this.prompt
                            .update(cx, |prompt, cx| prompt.set_value("", window, cx));
                    }
                    Err(error) => this.notice = Some(format!("{error}").into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.sending = true;
        let cancel = Workspace::dispatch(
            Op::Thread(ThreadOp::Cancel {
                id: self.thread.clone(),
            }),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = cancel.await;
            _ = this.update_in(cx, |this, _window, cx| {
                this.sending = false;
                if let Err(error) = result {
                    this.notice = Some(format!("{error}").into());
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn answering(&self, cx: &App) -> bool {
        self.thread_state(cx)
            .is_some_and(|(status, busy)| status.running() && busy)
    }

    fn running(&self, cx: &App) -> bool {
        self.thread_state(cx)
            .is_some_and(|(status, _)| status.running())
    }

    fn thread_state(&self, cx: &App) -> Option<(kg_domain::ThreadStatus, bool)> {
        Workspace::global(cx)
            .read(cx)
            .thread(&self.thread)
            .map(|thread| (thread.read(cx).record().status, thread.read(cx).busy()))
    }

    /// Who the agent's blocks belong to: the program the thread runs.
    fn who(&self, cx: &App) -> SharedString {
        Workspace::global(cx)
            .read(cx)
            .thread(&self.thread)
            .map_or_else(
                || "Agent".into(),
                |thread| thread.read(cx).record().provider.as_str().into(),
            )
    }

    fn hint(answering: bool, running: bool) -> SharedString {
        if answering {
            "The agent is answering. Cancel it to send something else.".into()
        } else if !running {
            "The agent is not running. Start it again below.".into()
        } else {
            "Enter sends, Shift+Enter adds a line.".into()
        }
    }
}

impl Focusable for ChatView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ChatView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let answering = self.answering(cx);
        let running = self.running(cx);
        let who = self.who(cx);
        v_flex()
            .size_full()
            .min_h_0()
            .key_context(CONTEXT)
            .child(self.transcript(&who, cx))
            .child(self.composer(answering, running, cx))
    }
}

impl ChatView {
    /// Everything the thread has said, read top to bottom.
    fn transcript(&self, who: &SharedString, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .id("chat-transcript")
            .ui_container()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px_4()
            .py_4()
            .child(
                v_flex()
                    .max_w(px(760.))
                    .mx_auto()
                    .w_full()
                    .gap_4()
                    .children(
                        self.blocks
                            .iter()
                            .enumerate()
                            .map(|(index, block)| part_view(block.part, index, block, who, cx)),
                    )
                    .when(self.blocks.is_empty(), |shown| {
                        shown.child(
                            div()
                                .id("chat-empty")
                                .type_body(cx)
                                .text_color(muted)
                                .child("Nothing said yet. Type a prompt below."),
                        )
                    }),
            )
    }

    /// The prompt box, its hint, and the one filled button this state offers.
    fn composer(&self, answering: bool, running: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let prompt = self.prompt.read(cx).value().to_string();
        let ready = running && !prompt.trim().is_empty() && !self.sending;
        v_flex().flex_none().px_4().pb_4().child(
            v_flex()
                .max_w(px(760.))
                .mx_auto()
                .w_full()
                .gap_2()
                .child(
                    div()
                        .id("chat-prompt")
                        .ui_observe_as(Facts {
                            role: Role::TextInput,
                            name: "Prompt".into(),
                            value: Some(prompt.into()),
                            disabled: answering,
                            selected: None,
                            focus: Some(self.prompt.read(cx).focus_handle(cx)),
                            hint: None,
                        })
                        .child(
                            Textarea::new(&self.prompt)
                                .text_size(px(crate::theme::type_body(cx)))
                                .small(),
                        ),
                )
                .children(self.notice.clone().map(|notice| {
                    div()
                        .id("chat-notice")
                        .type_small(cx)
                        .text_color(cx.theme().danger)
                        .child(notice)
                }))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .id("chat-hint")
                                .flex_1()
                                .min_w_0()
                                .type_small(cx)
                                .text_color(muted)
                                .child(Self::hint(answering, running)),
                        )
                        .when(answering, |row| {
                            row.child(control(
                                "chat-cancel",
                                Facts::button("Cancel"),
                                Button::new("chat-cancel-button")
                                    .small()
                                    .primary()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cancel(window, cx);
                                    })),
                            ))
                        })
                        .when(!answering, |row| {
                            row.child(control(
                                "chat-send",
                                Facts {
                                    disabled: !ready,
                                    ..Facts::button("Send")
                                },
                                Button::new("chat-send-button")
                                    .small()
                                    .primary()
                                    .label("Send")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit(window, cx);
                                    })),
                            ))
                        }),
                ),
        )
    }
}

/// Draws one part. This is the table a plugin will replace a part through, one arm per name.
fn part_view(part: Part, index: usize, block: &Block, who: &str, cx: &App) -> AnyElement {
    match part {
        Part::Message => message_view(index, block, who, cx),
        Part::Notice => notice_view(index, block, cx),
    }
}

fn message_view(index: usize, block: &Block, who: &str, cx: &App) -> AnyElement {
    let user = block.role == Some(Speaker::User);
    let spine = if user {
        cx.theme().accent
    } else {
        cx.theme().border
    };
    let label = if user {
        "You".to_owned()
    } else {
        who.to_owned()
    };
    let id = part_id(Part::Message, index);
    div()
        .id(id.clone())
        .ui_observe_as(Facts {
            role: Role::Label,
            name: format!("{label}: {}", block.text).into(),
            value: Some(block.text.clone().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: Some(label.clone().into()),
        })
        .child(
            v_flex()
                .gap_1()
                .pl_3()
                .border_l_2()
                .border_color(spine)
                .child(
                    div()
                        .type_caption(cx)
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                )
                .child(
                    div()
                        .type_body(cx)
                        .text_color(cx.theme().foreground)
                        .child(TextView::markdown(id, block.text.clone())),
                ),
        )
        .into_any_element()
}

fn notice_view(index: usize, block: &Block, cx: &App) -> AnyElement {
    div()
        .id(part_id(Part::Notice, index))
        .ui_observe_as(Facts {
            role: Role::Label,
            name: block.text.clone().into(),
            value: Some(block.text.clone().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        })
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .child(block.text.clone())
        .into_any_element()
}

fn part_id(part: Part, index: usize) -> ElementId {
    ElementId::Name(format!("chat-{}-{index}", part.name()).into())
}
