use std::collections::BTreeSet;

use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, ElementId, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div, px,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{Event, Op, Role as Speaker, ThreadId, ThreadOp, Timestamp, program_label};

use crate::kagan::Kagan;
use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

mod above;
mod body;
mod card;
mod composer;
mod kind;
mod part;
mod work;

/// The keys the chat adds: Escape cancels the agent's reply.
pub(crate) const CONTEXT: &str = "Chat";

/// An ACP thread's transcript and prompt box: what the agent said, in order, and the one box that
/// answers it. The thread's events are the transcript, so nothing is kept here that the log lacks.
pub(crate) struct ChatView {
    thread: ThreadId,
    /// The window's own view, so a row's link can open the review or the skills it names.
    kagan: gpui_kit::WeakEntity<Kagan>,
    transcript: part::Transcript,
    /// Whether the load has finished. Live events wait in `pending` until it has.
    loaded: bool,
    pending: Vec<Event>,
    prompt: gpui_kit::Entity<TextareaState>,
    scroller: gpui_kit::Entity<MessageScrollerState>,
    /// The rows whose arguments show every line.
    more: BTreeSet<u64>,
    /// Whether the trailing "Working…" row is drawn.
    working: bool,
    notice: Option<SharedString>,
    sending: bool,
    /// The cards above the prompt box, as the person left them.
    cards: above::Cards,
    /// The child threads that run now, read when the workspace says it changed.
    children: Vec<(ThreadId, SharedString)>,
    _events: Subscription,
    _prompt: Subscription,
    _scroller: Subscription,
}

impl ChatView {
    pub(crate) fn new(
        thread: ThreadId,
        kagan: gpui_kit::WeakEntity<Kagan>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            // Another thread's events are not this transcript's work: the fold meets the log
            // once per event that is this thread's, and the state's own changes.
            let sync = match event {
                WorkspaceEvent::Event { event, .. } if event.thread_id == this.thread => {
                    this.absorb(event, cx);
                    false
                }
                WorkspaceEvent::Changed => {
                    this.read_children(cx);
                    true
                }
                WorkspaceEvent::Waiting(id) | WorkspaceEvent::Finished(id) => *id == this.thread,
                _ => false,
            };
            if sync {
                this.sync(cx);
            }
        });
        let mut this = Self {
            thread,
            kagan,
            transcript: part::Transcript::default(),
            loaded: false,
            pending: Vec::new(),
            prompt,
            scroller,
            more: BTreeSet::new(),
            working: false,
            notice: None,
            sending: false,
            cards: above::Cards::default(),
            children: Vec::new(),
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

    /// The prompt box's text, as the person left it.
    pub(crate) fn draft(&self, cx: &App) -> String {
        self.prompt.read(cx).value().to_string()
    }

    pub(crate) fn set_draft(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt
            .update(cx, |prompt, cx| prompt.set_value(text, window, cx));
        cx.notify();
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
                this.read_children(cx);
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
        if let part::Fold::Dirty(from) = self.transcript.absorb(event) {
            self.sync(cx);
            let count = self.scroller.read(cx).item_count();
            let _ = self.scroller.update(cx, |state, cx| {
                state.remeasure_items(from.min(count)..count, cx)
            });
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

    /// The child threads that are running now, which the card over the box counts and lists. The
    /// parent is the thread itself, so a thread an agent started is already one of these.
    fn read_children(&mut self, cx: &App) {
        let workspace = Workspace::global(cx);
        let workspace = workspace.read(cx);
        self.children = workspace
            .thread_records(cx)
            .into_iter()
            .filter(|thread| {
                thread.parent_thread_id.as_ref() == Some(&self.thread) && thread.status.running()
            })
            .map(|thread| (thread.id, SharedString::from(thread.title)))
            .collect();
    }

    fn toggle(&mut self, key: part::RowKey, cx: &mut Context<Self>) {
        let answering = self.answering(cx);
        if let Some(index) = self.transcript.toggle(key, answering) {
            self.row_changed(index, cx);
        }
        cx.notify();
    }

    fn is_open(&self, index: usize, cx: &App) -> bool {
        self.transcript.is_open(index, self.answering(cx))
    }

    fn member_open(&self, key: part::RowKey) -> bool {
        self.transcript.member_open(key)
    }

    pub(super) fn kagan(&self) -> gpui_kit::WeakEntity<Kagan> {
        self.kagan.clone()
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

    /// Whether the thread is archived, so no action in the box could run.
    fn archived(&self, cx: &App) -> bool {
        Workspace::global(cx)
            .read(cx)
            .thread(&self.thread)
            .is_some_and(|thread| thread.read(cx).record().archived_at.is_some())
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
}

impl Focusable for ChatView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.prompt.read(cx).focus_handle(cx)
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_with(None, window, cx)
    }
}

impl ChatView {
    /// The whole chat: the body, which a plugin may draw, and the prompt box, which it may not.
    pub(crate) fn render_with(
        &mut self,
        transcript: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let answering = self.answering(cx);
        let running = self.running(cx);
        let body = match transcript {
            Some(transcript) => transcript,
            None => self.transcript(cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .min_h_0()
            .key_context(CONTEXT)
            .child(body)
            .child(composer::composer(self, answering, running, window, cx))
            .into_any_element()
    }
}

impl ChatView {
    /// Everything the thread has said, in the one column the prompt box shares with it. An empty
    /// transcript centers its one quiet line in that column instead of hanging off the left.
    fn transcript(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let scroller = self.scroller.clone();
        let usage = self
            .transcript
            .usage
            .map(|(used, size)| (Self::compact(used), Self::compact(size)));
        let muted = cx.theme().muted_foreground;
        let empty = self.transcript.rows.is_empty();
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
            .child(
                v_flex()
                    .id("chat-column")
                    .ui_container()
                    .max_w(px(composer::COLUMN))
                    .mx_auto()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .gap_1()
                    .children(usage.map(|(used, size)| {
                        div()
                            .id("chat-usage")
                            .type_caption(cx)
                            .text_color(muted)
                            .child(format!("{used} of {size} tokens in context"))
                    }))
                    .when(empty, |column| {
                        column.items_center().justify_center().child(
                            div()
                                .id("chat-empty")
                                .ui_container()
                                .type_body(cx)
                                .text_color(muted)
                                .child("Nothing said yet. Type a prompt below."),
                        )
                    })
                    .when(!empty, |column| {
                        column.child(div().flex_1().min_h_0().child(list))
                    }),
            )
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
        return working_row(cx);
    };
    let open = weak
        .read_with(cx, |this, cx| this.is_open(index, cx))
        .unwrap_or(false);
    let live = weak
        .read_with(cx, |this, cx| {
            this.answering(cx) && index + 1 == this.transcript.rows.len()
        })
        .unwrap_or(false);
    match &row.kind {
        part::RowKind::Message { role, text, at, .. } => {
            let who = weak
                .read_with(cx, ChatView::who)
                .unwrap_or_else(|_| "Agent".into());
            message_view(
                row.id,
                *role,
                text,
                *at,
                if *role == Speaker::User { "You" } else { &who },
                cx,
            )
        }
        part::RowKind::Thought(thought) => {
            work::thought_row(&row, thought, live, open, weak, window, cx)
        }
        part::RowKind::Work(tool) => work::work_row(&row, tool, false, open, weak, window, cx),
        part::RowKind::Bundle(members) => work::bundle_row(&row, members, open, weak, window, cx),
        part::RowKind::Plan(entries) => work::plan_row(&row, entries, open, weak, window, cx),
        part::RowKind::Approval(id) => {
            let (title, worktree) = weak
                .read_with(cx, |this, cx| this.approval_detail(index, cx))
                .unwrap_or_default();
            card::approval_card(id, title.as_deref(), worktree, weak, window, cx)
        }
        part::RowKind::Notice(text) => notice_view(row.id, text, cx),
        part::RowKind::Divider { from, to, outcome } => {
            divider_view(row.id, *from, *to, *outcome, cx)
        }
    }
}

impl ChatView {
    /// What an approval card says about the tool row it follows: its title, and whether the tool
    /// works in the thread's own folder.
    fn approval_detail(&self, index: usize, cx: &App) -> (Option<String>, bool) {
        let tool = index.checked_sub(1).and_then(|previous| {
            match &self.transcript.rows.get(previous)?.kind {
                part::RowKind::Work(tool) => Some(tool.as_ref()),
                _ => None,
            }
        });
        let Some(tool) = tool else {
            return (None, false);
        };
        let environment = Workspace::global(cx)
            .read(cx)
            .thread(&self.thread)
            .is_some_and(|thread| thread.read(cx).record().environment_id.is_some());
        (
            kind::title_of(tool),
            card::runs_in_worktree(tool.kind, environment),
        )
    }
}

/// The trailing row that says the agent is working and nothing else is live.
fn working_row(cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    div()
        .id("chat-working")
        .ui_observe_as(Facts::label("Working", "working"))
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
    let element = work::named(part::Part::Message, id);
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
        .ui_observe_as(Facts::label(
            format!("{label}, {}", clock(at)),
            text.to_owned(),
        ))
        .child(body)
        .into_any_element()
}

fn notice_view(id: u64, text: &str, cx: &mut App) -> AnyElement {
    div()
        .id(work::named(part::Part::Notice, id))
        .ui_observe_as(Facts::label(text.to_owned(), text.to_owned()))
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
    let span = kind::span_text(kind::span_of(from, to));
    let text = if outcome == kg_domain::TurnOutcome::Failed {
        format!("Worked for {span} · failed")
    } else {
        format!("Worked for {span}")
    };
    div()
        .id(ElementId::Name(format!("chat-divider-{id}").into()))
        .ui_observe_as(Facts::label(text.clone(), text.clone()))
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
