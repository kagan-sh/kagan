use std::collections::{BTreeMap, BTreeSet};

use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, ElementId, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    Styled as _, Subscription, Window, div, px,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{Event, Op, Role as Speaker, ThreadId, ThreadOp, Timestamp, program_label};

use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

mod body;
mod part;
mod work;

/// The keys the chat adds: Escape cancels the agent's reply.
pub(crate) const CONTEXT: &str = "Chat";

/// An ACP thread's transcript and prompt box: what the agent said, in order, and the one box that
/// answers it. The thread's events are the transcript, so nothing is kept here that the log lacks.
pub(crate) struct ChatView {
    thread: ThreadId,
    transcript: part::Transcript,
    /// Whether the load has finished. Live events wait in `pending` until it has.
    loaded: bool,
    pending: Vec<Event>,
    prompt: gpui_kit::Entity<TextareaState>,
    scroller: gpui_kit::Entity<MessageScrollerState>,
    /// The person's own expand and collapse choices: they always win over the rules.
    open: BTreeMap<u64, bool>,
    /// The rows whose arguments show every line.
    more: BTreeSet<u64>,
    /// Whether the trailing "Working…" row is drawn.
    working: bool,
    notice: Option<SharedString>,
    sending: bool,
    _events: Subscription,
    _prompt: Subscription,
    _scroller: Subscription,
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
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        // The list lives inside this view's element tree, so a scroll that changes what the
        // scroller draws (the jump button, the bottom fade) has to bring the view back.
        let scroller_watch = cx.observe(&scroller, |_, _, cx| cx.notify());
        let events = cx.subscribe(&Workspace::global(cx), |this, _, event, cx| {
            if let WorkspaceEvent::Event { event, .. } = event
                && event.thread_id == this.thread
            {
                this.absorb(event, cx);
            }
            this.sync(cx);
        });
        let mut this = Self {
            thread,
            transcript: part::Transcript::default(),
            loaded: false,
            pending: Vec::new(),
            prompt,
            scroller,
            open: BTreeMap::new(),
            more: BTreeSet::new(),
            working: false,
            notice: None,
            sending: false,
            _events: events,
            _prompt: prompt_watch,
            _scroller: scroller_watch,
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
            let read = read.await;
            _ = this.update(cx, |this, cx| {
                // A load that fails still ends the wait: the live events behind it are drawn.
                this.loaded = true;
                if let Ok(kg_domain::OpResult::Events(events)) = read {
                    for event in events {
                        this.absorb(&event, cx);
                    }
                }
                let pending = std::mem::take(&mut this.pending);
                for event in &pending {
                    this.absorb(event, cx);
                }
                this.sync(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn absorb(&mut self, event: &Event, cx: &mut Context<Self>) {
        if !self.loaded {
            self.pending.push(event.clone());
            return;
        }
        match self.transcript.absorb(event) {
            part::Fold::Ignored => {}
            part::Fold::Appended(index) => {
                _ = self.scroller.update(cx, |state, cx| state.append(1, cx));
                let _ = self.row_changed(index, cx);
            }
            part::Fold::Changed(index) => {
                let _ = self.row_changed(index, cx);
            }
        }
        cx.notify();
    }

    /// Keeps the virtual list's row count in step with the fold, and its measured heights with the
    /// row that changed.
    fn sync(&mut self, cx: &mut Context<Self>) {
        self.working = self.answering(cx) && !self.transcript.live_last();
        let desired = self.transcript.rows.len() + usize::from(self.working);
        let current = self.scroller.read(cx).item_count();
        if desired > current {
            _ = self
                .scroller
                .update(cx, |state, cx| state.append(desired - current, cx));
        } else if desired < current {
            _ = self
                .scroller
                .update(cx, |state, cx| state.splice(desired..current, 0, cx));
        }
        cx.notify();
    }

    fn row_changed(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        self.scroller
            .update(cx, |state, cx| state.remeasure_items(index..index + 1, cx))
    }

    fn toggle(&mut self, row: u64, cx: &mut Context<Self>) {
        let Some(index) = self.transcript.rows.iter().position(|line| line.id == row) else {
            return;
        };
        let open = self.is_open(index, cx);
        self.open.insert(row, !open);
        self.row_changed(index, cx);
        cx.notify();
    }

    fn is_open(&self, index: usize, cx: &App) -> bool {
        let Some(row) = self.transcript.rows.get(index) else {
            return false;
        };
        self.open
            .get(&row.id)
            .copied()
            .unwrap_or_else(|| self.auto_open(index, cx))
    }

    /// Recomputed on every event, never stored, so it cannot fight the log.
    fn auto_open(&self, index: usize, cx: &App) -> bool {
        let Some(row) = self.transcript.rows.get(index) else {
            return false;
        };
        let last = index + 1 == self.transcript.rows.len();
        let failed_turn = self.transcript.last_outcome == Some(kg_domain::TurnOutcome::Failed);
        match &row.kind {
            part::RowKind::Plan(_) => true,
            part::RowKind::Thought(thought) => last && !thought.text.is_empty(),
            part::RowKind::Bundle(_) => last && self.answering(cx),
            part::RowKind::Work(tool) => {
                (last && tool.live_body()) || (last && failed_turn && tool.failed())
            }
            _ => false,
        }
    }

    fn more_open(&self, row: u64) -> bool {
        self.more.contains(&row)
    }

    fn toggle_more(&mut self, row: u64, cx: &mut Context<Self>) {
        if !self.more.remove(&row) {
            self.more.insert(row);
        }
        if let Some(index) = self.transcript.rows.iter().position(|line| line.id == row) {
            self.row_changed(index, cx);
        }
        cx.notify();
    }

    fn say(&mut self, text: String, cx: &mut Context<Self>) {
        self.notice = Some(text.into());
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

    /// Who the agent's blocks belong to: the program the thread runs, by its short name.
    fn who(&self, cx: &App) -> SharedString {
        Workspace::global(cx)
            .read(cx)
            .thread(&self.thread)
            .map_or_else(
                || "Agent".into(),
                |thread| program_label(&thread.read(cx).record().provider).into(),
            )
    }

    /// What to do now, in this state, in one line.
    fn hint(&self, cx: &App) -> SharedString {
        if self.sending {
            "Sending…".into()
        } else if self.answering(cx) {
            "The agent is answering. Cancel it to send something else.".into()
        } else if !self.running(cx) {
            "The agent is not running. Start it again below.".into()
        } else if self.prompt.read(cx).value().trim().is_empty() {
            "Type a prompt to send it.".into()
        } else {
            "Enter sends, Shift+Enter adds a line.".into()
        }
    }
}

impl Focusable for ChatView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.prompt.read(cx).focus_handle(cx)
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let answering = self.answering(cx);
        let running = self.running(cx);
        let who = self.who(cx);
        v_flex()
            .size_full()
            .min_h_0()
            .key_context(CONTEXT)
            .child(self.transcript(&who, window, cx))
            .child(self.composer(answering, running, cx))
    }
}

impl ChatView {
    /// Everything the thread has said, read top to bottom in one column.
    fn transcript(
        &mut self,
        _who: &SharedString,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let weak = cx.weak_entity();
        let scroller = self.scroller.clone();
        let usage = self
            .transcript
            .usage
            .map(|(used, size)| (Self::compact(used), Self::compact(size)));
        let muted = cx.theme().muted_foreground;
        let list = MessageScroller::new("chat-rows", scroller, move |index, window, cx| {
            row_at(&weak, index, window, cx)
        })
        .with_bottom_fade(cx.theme().background)
        .with_jump_button_label("Jump to latest");
        v_flex()
            .id("chat-transcript")
            .ui_container()
            .flex_1()
            .min_h_0()
            .px_4()
            .gap_1()
            .children(usage.map(|(used, size)| {
                div()
                    .id("chat-usage")
                    .type_caption(cx)
                    .text_color(muted)
                    .child(format!("{used} of {size} tokens in context"))
            }))
            .child(div().flex_1().min_h_0().child(list))
            .when(self.transcript.rows.is_empty(), |shown| {
                shown.child(
                    div()
                        .id("chat-empty")
                        .type_body(cx)
                        .text_color(muted)
                        .child("Nothing said yet. Type a prompt below."),
                )
            })
    }

    /// Thousands as "12K", so the context line never eats the width.
    fn compact(tokens: u64) -> String {
        if tokens >= 1_000_000 {
            format!("{}M", tokens / 1_000_000)
        } else if tokens >= 1_000 {
            format!("{}K", tokens / 1_000)
        } else {
            tokens.to_string()
        }
    }

    /// The prompt box, its one line of guidance, and the one filled button this state offers.
    fn composer(&self, answering: bool, running: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let prompt = self.prompt.read(cx).value().to_string();
        let ready = running && !prompt.trim().is_empty() && !self.sending;
        let hint = self.hint(cx);
        let button = if answering {
            control(
                "chat-cancel",
                Facts::button("Cancel"),
                Button::new("chat-cancel-button")
                    .small()
                    .primary()
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))),
            )
        } else {
            control(
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
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
        };
        v_flex().flex_none().px_4().pb_4().child(
            v_flex()
                .max_w(px(760.))
                .mx_auto()
                .w_full()
                .gap_2()
                .child(
                    v_flex()
                        .border_1()
                        .border_color(cx.theme().input)
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().secondary)
                        .child(
                            div()
                                .id("chat-prompt")
                                .ui_observe_as(Facts {
                                    role: Role::TextInput,
                                    name: "Prompt".into(),
                                    value: Some(prompt.into()),
                                    disabled: false,
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
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .px_2()
                                .pb_2()
                                .child(div().flex_1())
                                .child(button),
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
                    div()
                        .id("chat-hint")
                        .type_caption(cx)
                        .text_color(muted)
                        .ui_observe_as(Facts {
                            role: Role::Label,
                            name: "Hint".into(),
                            value: Some(hint.clone()),
                            disabled: false,
                            selected: None,
                            focus: None,
                            hint: None,
                        })
                        .child(hint),
                ),
        )
    }
}

/// Draws one row, at the index the virtual list asks for.
fn row_at(
    weak: &gpui_kit::WeakEntity<ChatView>,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Some(row) = weak
        .read_with(cx, |this, _| this.transcript.rows.get(index).cloned())
        .ok()
        .flatten()
    else {
        return working_row(weak, window, cx);
    };
    let open = weak
        .read_with(cx, |this, cx| this.is_open(index, cx))
        .unwrap_or(false);
    let live = weak
        .read_with(cx, |this, cx| {
            this.answering(cx) && index + 1 == this.transcript.rows.len()
        })
        .unwrap_or(false);
    match row.kind.clone() {
        part::RowKind::Message { role, text, at, .. } => {
            let who = weak
                .read_with(cx, ChatView::who)
                .unwrap_or_else(|_| "Agent".into());
            message_view(
                row.id,
                role,
                &text,
                at,
                if role == Speaker::User { "You" } else { &who },
                cx,
            )
        }
        part::RowKind::Thought(thought) => {
            work::thought_row(&row, &thought, live, open, weak, window, cx)
        }
        part::RowKind::Work(tool) => work::work_row(&row, &tool, false, open, weak, window, cx),
        part::RowKind::Bundle(members) => work::bundle_row(&row, &members, open, weak, window, cx),
        part::RowKind::Plan(entries) => work::plan_row(&row, &entries, open, weak, window, cx),
        part::RowKind::Approval(id) => {
            let title = weak
                .read_with(cx, |this, _| this.approval_title(index))
                .ok()
                .flatten();
            body::approval_card(&id, title.as_deref(), weak, window, cx)
        }
        part::RowKind::Notice(text) => notice_view(row.id, &text, cx),
        part::RowKind::Divider { from, to, outcome } => divider_view(row.id, from, to, outcome, cx),
    }
}

impl ChatView {
    /// The title of the tool row an approval card follows.
    fn approval_title(&self, index: usize) -> Option<String> {
        let previous = index.checked_sub(1)?;
        match &self.transcript.rows.get(previous)?.kind {
            part::RowKind::Work(tool) => work::title_of(tool),
            _ => None,
        }
    }
}

/// The trailing row that says the agent is working and nothing else is live.
fn working_row(
    _weak: &gpui_kit::WeakEntity<ChatView>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    div()
        .id("chat-working")
        .ui_observe_as(Facts {
            role: Role::Label,
            name: "Working".into(),
            value: Some("working".into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        })
        .h(px(crate::theme::row_height(cx)))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_color(muted)
        .type_small(cx)
        .child(crate::theme::Tone::Busy.mark(cx))
        .child("Working…")
        .into_any_element()
}

/// A person's message is a raised block with an accent spine; the agent's is its name, the time and
/// the words, with Copy on hover.
fn message_view(
    id: u64,
    role: Speaker,
    text: &str,
    at: Timestamp,
    who: &str,
    cx: &mut App,
) -> AnyElement {
    let user = role == Speaker::User;
    let element = work::row_id(part::Part::Message, id);
    let label = who.to_owned();
    let copy = text.to_owned();
    let row = h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .type_caption(cx)
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .child(label.clone()),
        )
        .child(
            div()
                .type_caption(cx)
                .text_color(cx.theme().muted_foreground)
                .child(clock(at)),
        )
        .when(!user, |row| {
            row.child(div().flex_1()).child(
                div()
                    .invisible()
                    .group_hover("chat-message", gpui_kit::Styled::visible)
                    .child(control(
                        format!("chat-copy-{id}"),
                        Facts::button("Copy"),
                        Button::new(SharedString::from(format!("chat-copy-{id}-button")))
                            .small()
                            .ghost()
                            .icon(gpui_kit::assets::IconName::Copy)
                            .tooltip("Copy the message")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                            }),
                    )),
            )
        });
    let body =
        v_flex()
            .gap_1()
            .when(user, |body| {
                body.bg(cx.theme().secondary)
                    .rounded(cx.theme().radius)
                    .border_l_2()
                    .border_color(cx.theme().accent)
                    .px_3()
                    .py_2()
            })
            .when(!user, gpui_kit::Styled::pl_2)
            .child(row)
            .child(div().type_body(cx).text_color(cx.theme().foreground).child(
                TextView::markdown(
                    SharedString::from(format!("chat-message-md-{id}")),
                    text.to_owned(),
                ),
            ));
    div()
        .id(element)
        .group("chat-message")
        .ui_observe_as(Facts {
            role: Role::Label,
            name: format!("{label}, {}", clock(at)).into(),
            value: Some(text.to_owned().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        })
        .child(body)
        .into_any_element()
}

fn notice_view(id: u64, text: &str, cx: &mut App) -> AnyElement {
    div()
        .id(work::row_id(part::Part::Notice, id))
        .ui_observe_as(Facts {
            role: Role::Label,
            name: text.to_owned().into(),
            value: Some(text.to_owned().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        })
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}

/// The quiet line between turns: how long the agent worked.
fn divider_view(
    id: u64,
    from: Timestamp,
    to: Timestamp,
    outcome: kg_domain::TurnOutcome,
    cx: &mut App,
) -> AnyElement {
    let span = work::span_text(std::time::Duration::from_millis(
        u64::try_from(to.0.saturating_sub(from.0).max(0)).unwrap_or(0),
    ));
    let text = if outcome == kg_domain::TurnOutcome::Failed {
        format!("Worked for {span} · failed")
    } else {
        format!("Worked for {span}")
    };
    div()
        .id(ElementId::Name(format!("chat-divider-{id}").into()))
        .ui_observe_as(Facts {
            role: Role::Label,
            name: text.clone().into(),
            value: Some(text.clone().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        })
        .py_1()
        .type_caption(cx)
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

/// The wall clock time of an event, in the person's own zone.
fn clock(at: Timestamp) -> SharedString {
    jiff::Timestamp::from_millisecond(at.0).map_or_else(
        |_| SharedString::default(),
        |time| {
            SharedString::from(
                time.to_zoned_datetime(jiff::tz::TimeZone::system())
                    .strftime("%H:%M")
                    .to_string(),
            )
        },
    )
}
