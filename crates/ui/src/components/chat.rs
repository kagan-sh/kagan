use std::sync::Arc;

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, div,
};
use gpui_shell::{
    ComponentArgument, ComponentDescriptor, ComponentMaterializer, ComponentPayload,
    ComponentRegistry, ConstructorDescriptor, MaterializeRequest, RegistryError, anyhow,
};
use kg_core::{Thread, Workspace, WorkspaceEvent};
use kg_domain::{Event, EventKind, Link, Op, Order, Role, ThreadId, ThreadOp, Visibility};
use kg_plugins::PluginHost;

use super::component_error::ComponentError;
use super::{apply, plain_method};
use crate::kagan::chat::{ChatView, Place, cut, step};
use crate::theme::Tone;

const NOT_A_CHAT: &str = "This thread is not a chat.";
const ARCHIVED: &str = "This thread is archived.";
const GONE: &str = "This thread is gone.";
const NEEDS_CHAT: &str = "ThreadChat needs \"chat\" in powers.areas.";

// A compact card keeps at most this many of the thread's newest events, so a board of cards never
// reads a whole log.
const EVENTS: u32 = 50;

struct ThreadChat;

impl ComponentMaterializer for ThreadChat {
    fn materialize(&self, mut request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let id = request
            .payload()
            .downcast_ref::<ThreadId>()
            .ok_or_else(|| ComponentError::new("ThreadChat received an incompatible thread id"))?
            .clone();
        let compact = apply::<bool>(&request);
        let key = request.element_id().clone();
        let body = request.with_window_app(|window, cx| {
            let Some((_, kagan)) = crate::kagan::front(cx) else {
                return Err(ComponentError::new("ThreadChat needs kagan's window").into());
            };
            if !allowed(window, cx) {
                return Ok(refused(NEEDS_CHAT, cx));
            }
            let Some(thread) = Workspace::global(cx).read(cx).thread(&id).cloned() else {
                return Ok(refused(GONE, cx));
            };
            let record = thread.read(cx).record();
            if record.archived_at.is_some() {
                return Ok(refused(ARCHIVED, cx));
            }
            if record.visibility != Visibility::Visible {
                return Ok(refused(GONE, cx));
            }
            if record.link != Link::Acp {
                return Ok(refused(NOT_A_CHAT, cx));
            }
            let state = SharedString::from(format!("kagan-thread-chat:{key:?}:{}:{compact}", id.0));
            if compact {
                let card = window.use_keyed_state(state, cx, |_, cx| Card::new(thread, cx));
                return Ok(div().child(card).into_any_element());
            }
            let chat = window.use_keyed_state(state, cx, |window, cx| {
                ChatView::new(id.clone(), kagan.downgrade(), Place::Embedded, window, cx)
            });
            Ok(div().size_full().min_h_0().child(chat).into_any_element())
        })?;
        // The full chat fills its panel; a card keeps its two lines' own height.
        let frame = if compact {
            div()
        } else {
            div().size_full().min_h_0()
        };
        request.finish(frame.child(body))
    }
}

// A thread this component cannot draw answers with one line, the way `kagan/chat` refuses it.
fn refused(text: &'static str, cx: &App) -> AnyElement {
    div()
        .p_3()
        .child(
            div()
                .id("thread-chat-refused")
                .ui_observe_as(Facts::label(text, text))
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(text),
        )
        .into_any_element()
}

// The same grant `kagan/chat` needs: the component draws the transcript that module reads.
fn allowed(window: &Window, cx: &App) -> bool {
    let Some(host) = PluginHost::try_global(cx) else {
        return false;
    };
    let owner = host
        .read(cx)
        .view_fetch(window.current_view())
        .map(|(_, plugin, _)| plugin);
    owner.is_some_and(|plugin| host.read(cx).allows_chat(&plugin))
}

// Two lines and Open: the tone and the live step or the end, then the last answer's first line.
struct Card {
    id: ThreadId,
    thread: Entity<Thread>,
    said: String,
    said_id: Option<String>,
    step: Option<Step>,
    pending: Vec<Event>,
    loaded: bool,
    read: Option<Task<()>>,
    _events: Subscription,
    _changes: Subscription,
    _attention: Subscription,
}

enum Step {
    Work(String, Option<String>),
    Thinking,
}

impl Step {
    fn text(&self) -> String {
        match self {
            Self::Work(label, Some(title)) => format!("{label}  {title}"),
            Self::Work(label, None) => label.clone(),
            Self::Thinking => "Thinking…".to_owned(),
        }
    }
}

impl Card {
    fn new(thread: Entity<Thread>, cx: &mut Context<Self>) -> Self {
        let id = thread.read(cx).record().id.clone();
        let read = Workspace::dispatch(
            Op::Thread(ThreadOp::Events {
                id: id.clone(),
                after_seq: None,
                limit: Some(EVENTS),
                order: Some(Order::Desc),
            }),
            cx,
        );
        let watched = id.clone();
        let events = cx.subscribe(&Workspace::global(cx), move |this, _, event, cx| {
            let WorkspaceEvent::Event { event, .. } = event else {
                return;
            };
            if event.thread_id != watched {
                return;
            }
            this.absorb(event);
            cx.notify();
        });
        let changes = cx.observe(&thread, |_, _, cx| cx.notify());
        let attention = crate::kagan::attention::global(cx);
        let questions = cx.observe(&attention, |_, _, cx| cx.notify());
        let mut this = Self {
            id,
            thread,
            said: String::new(),
            said_id: None,
            step: None,
            pending: Vec::new(),
            loaded: false,
            read: None,
            _events: events,
            _changes: changes,
            _attention: questions,
        };
        this.read = Some(cx.spawn(async move |this, cx| {
            let newest = read.await;
            _ = this.update(cx, |this, cx| {
                this.loaded = true;
                if let Ok(kg_domain::OpResult::Events(mut events)) = newest {
                    events.reverse();
                    for event in &events {
                        this.absorb(event);
                    }
                }
                let pending = std::mem::take(&mut this.pending);
                for event in &pending {
                    this.absorb(event);
                }
                cx.notify();
            });
        }));
        this
    }

    fn absorb(&mut self, event: &Event) {
        if !self.loaded {
            self.pending.push(event.clone());
            return;
        }
        match &event.kind {
            EventKind::Message {
                role: Role::Agent,
                text,
                message_id,
                ..
            } => {
                if message_id.is_some() && message_id == &self.said_id {
                    self.said.push_str(text);
                } else {
                    self.said.clone_from(text);
                    self.said_id.clone_from(message_id);
                }
            }
            EventKind::Tool(call) => {
                let (label, title) = step(call);
                self.step = Some(Step::Work(label, title));
            }
            EventKind::Thought { .. } => self.step = Some(Step::Thinking),
            _ => {}
        }
    }

    // The live step while the agent works, what it waits on, and the run's end once it stopped.
    fn detail(&self, tone: Tone, cx: &App) -> Option<String> {
        match tone {
            Tone::Busy => Some(
                self.step
                    .as_ref()
                    .map_or_else(|| "Thinking…".to_owned(), Step::text),
            ),
            Tone::Needs => self.step.as_ref().map(Step::text),
            _ => Workspace::global(cx).read(cx).end_line(&self.id),
        }
    }

    // The first line of the last answer, whole for the tooltip and cut on a word for the line.
    fn said_line(&self) -> Option<(String, String)> {
        let line = self.said.lines().next()?.trim();
        (!line.is_empty()).then(|| (line.to_owned(), cut(line, 160)))
    }
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tone = crate::kagan::attention::global(cx)
            .read(cx)
            .tone(self.thread.read(cx).record(), cx);
        let word = tone.word();
        let detail = self.detail(tone, cx);
        let said = self.said_line();
        let value = said
            .as_ref()
            .map(|(line, _)| line.clone())
            .or_else(|| detail.clone())
            .unwrap_or_default();
        let open = self.id.clone();
        let kagan = crate::kagan::front(cx).map(|(_, kagan)| kagan);
        v_flex()
            .id(ElementId::Name(format!("thread-chat-{}", self.id.0).into()))
            .ui_observe_as(Facts::label(format!("Thread chat: {word}"), value))
            .gap_1()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .bg(cx.theme().secondary)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(tone.mark(cx))
                    .child(
                        div()
                            .flex_shrink_0()
                            .type_small(cx)
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .text_color(cx.theme().foreground)
                            .child(word),
                    )
                    .children(detail.map(|detail| {
                        h_flex()
                            .min_w_0()
                            .gap_2()
                            .type_small(cx)
                            .text_color(cx.theme().muted_foreground)
                            .child("·")
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("thread-chat-{}-step", self.id.0).into(),
                                    ))
                                    .ui_observe_as(Facts::label("Step", detail.clone()))
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(detail),
                            )
                    }))
                    .child(div().flex_1())
                    .children(kagan.map(|kagan| {
                        div()
                            .id(ElementId::Name(
                                format!("thread-chat-{}-open", self.id.0).into(),
                            ))
                            .ui_observe_as(Facts::button("Open"))
                            .child(
                                Button::new(SharedString::from(format!(
                                    "thread-chat-{}-open-button",
                                    self.id.0
                                )))
                                .small()
                                .outline()
                                .label("Open")
                                .tooltip("Open the thread in the center")
                                .on_click(
                                    move |_, window, cx| {
                                        kagan.update(cx, |kagan, cx| {
                                            kagan.select(open.clone(), window, cx);
                                        });
                                    },
                                ),
                            )
                    })),
            )
            .children(said.map(|(line, cut)| {
                let tip = line.clone();
                div()
                    .id(ElementId::Name(
                        format!("thread-chat-{}-said", self.id.0).into(),
                    ))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .ui_observe_as(Facts::label("Last message", line))
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .text_ellipsis()
                    .child(cut)
            }))
    }
}

pub(super) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    registry.register(
        ComponentDescriptor::new("ThreadChat", Arc::new(ThreadChat))
            .with_constructors(vec![ConstructorDescriptor::new(
                "ThreadChat",
                vec![super::string("thread_id")],
                |arguments| {
                    let [ComponentArgument::String(id)] = arguments else {
                        return Err("ThreadChat expects a thread id".into());
                    };
                    super::nonempty(id)?;
                    Ok(ComponentPayload::new(ThreadId(id.clone())))
                },
            )])
            .with_methods(vec![plain_method::<bool>(
                "compact",
                |compact| *compact = true,
                "Draws two lines and Open: the thread's tone and live step, then the first line of its last answer. It reads at most the newest 50 events.",
            )])
            .with_documentation(
                "A thread's chat on a plugin's own screen. new ThreadChat(thread_id) draws the transcript, the cards and the prompt box, and needs \"chat\" in powers.areas. compact() draws two lines and Open instead, for a list of threads. A terminal, archived or removed thread answers with one line.",
            ),
    )
    .map(|_| ())
}
