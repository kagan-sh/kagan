use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_domain::{Event, EventKind, Link, Role, Thread, ThreadId, ThreadStatus, Timestamp};

use super::{click, label, open, shows, value_of, wait_until};

/// A thread the store already holds: an ACP one has no program in this process, so the center
/// shows its transcript and prompt box.
fn seed_acp_thread(data: &std::path::Path) -> ThreadId {
    let mut store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    let id = ThreadId("thr_chat".into());
    store
        .put_thread(&Thread {
            id: id.clone(),
            project_id: kg_domain::ProjectId("prj_kagan".into()),
            environment_id: None,
            title: "Chat".into(),
            title_locked: false,
            provider: "opencode".into(),
            link: Link::Acp,
            status: ThreadStatus::Idle,
            visibility: kg_domain::Visibility::Visible,
            parent_thread_id: None,
            quiet: false,
            notify: kg_domain::ThreadNotify::Default,
            lifecycle_owner_thread_id: None,
            section_id: None,
            pinned: false,
            last_read_at: None,
            latest_attention_at: Timestamp(0),
            mark: kg_domain::ThreadMark::None,
            archived_at: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        })
        .unwrap();
    store
        .append_events(
            &id,
            Timestamp(1),
            [
                kg_domain::EventKind::Message {
                    role: kg_domain::Role::User,
                    text: "say hi".into(),
                    message_id: None,
                    attachments: Vec::new(),
                },
                kg_domain::EventKind::Message {
                    role: kg_domain::Role::Agent,
                    text: "hi there".into(),
                    message_id: None,
                    attachments: Vec::new(),
                },
            ],
        )
        .unwrap();
    id
}

/// Opens a window on a seeded ACP thread and shows its chat.
fn show_chat(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::kagan::Kagan>,
    AnyWindowHandle,
    tempfile::TempDir,
) {
    let (kagan, window, data) = open(cx);
    let id = seed_acp_thread(data.path());
    // A thread written behind the window's back arrives with the next refresh, which marking it
    // read asks for.
    let read = cx.update(|cx| {
        kg_core::Workspace::dispatch(
            kg_domain::Op::Thread(kg_domain::ThreadOp::MarkRead { id: id.clone() }),
            cx,
        )
    });
    let _ = cx.foreground_executor().block_test(read);
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    (kagan, window, data)
}

#[gpui_kit::test]
fn an_acp_thread_shows_its_transcript_and_prompt_box(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    assert!(!shows(cx, window, "terminal"));
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-1").is_some_and(|text| text.contains("hi there"))
    });
    assert_eq!(
        value_of(cx, window, "chat-message-0").as_deref(),
        Some("say hi")
    );
    assert!(shows(cx, window, "chat-prompt"));
    assert!(shows(cx, window, "chat-send"));
    // The hint says what this state wants, and `kg ui` can read it.
    assert_eq!(
        value_of(cx, window, "chat-hint").as_deref(),
        Some("The agent is not running. Start it again below.")
    );
}

/// Escape with no reply open is left for whatever else handles it: a stopped thread says nothing.
#[gpui_kit::test]
fn escape_with_no_reply_open_says_nothing(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    cx.dispatch_action(window, crate::kagan::CancelTurn);
    cx.run_until_parked();
    assert!(!shows(cx, window, "note"));
}

/// A message is named by who spoke and when, and its words are the value, so a reader hears both.
#[gpui_kit::test]
fn a_message_is_named_by_who_spoke_and_when(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-1").is_some_and(|text| text.contains("hi there"))
    });
    let agent = label(cx, window, "chat-message-1");
    assert!(
        agent.starts_with("OpenCode, "),
        "the program's short name and the time: {agent}"
    );
    assert!(!agent.contains("hi there"), "{agent}");
    let user = label(cx, window, "chat-message-0");
    assert!(user.starts_with("You, "), "{user}");
}

/// What the workspace emits for an event, which is what the view's subscription hears.
fn emit(cx: &mut TestAppContext, event: Event) {
    cx.update(|cx| {
        kg_core::Workspace::global(cx).update(cx, |_, cx| {
            cx.emit(kg_core::WorkspaceEvent::Event {
                event,
                project_id: kg_domain::ProjectId("prj_kagan".into()),
            });
        });
    });
}

fn agent_said(id: &ThreadId, seq: u64, text: &str) -> Event {
    Event {
        thread_id: id.clone(),
        seq,
        at: Timestamp(2),
        kind: EventKind::Message {
            role: Role::Agent,
            text: text.into(),
            message_id: None,
            attachments: Vec::new(),
        },
    }
}

/// An event the load has already read is drawn once, not twice and out of order.
#[gpui_kit::test]
fn an_event_the_load_already_read_is_drawn_once(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    let id = ThreadId("thr_chat".into());
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-1").is_some_and(|text| text.contains("hi there"))
    });
    emit(cx, agent_said(&id, 1, "hi there"));
    assert_eq!(
        value_of(cx, window, "chat-message-1").as_deref(),
        Some("hi there"),
        "the chunk the load already read is not written again"
    );
    emit(
        cx,
        Event {
            thread_id: id.clone(),
            seq: 3,
            at: Timestamp(3),
            kind: EventKind::Notice {
                severity: kg_domain::NoticeSeverity::Info,
                text: "and again".into(),
            },
        },
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-notice-2").is_some_and(|text| text.contains("and again"))
    });
}

/// A load that fails still ends the wait, so the live events behind it are drawn.
#[gpui_kit::test]
fn a_load_that_fails_still_draws_the_events_after_it(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    // The store holds this thread and the workspace has never heard of it, so its load fails.
    let id = seed_acp_thread(data.path());
    let window = cx
        .add_window(|window, cx| crate::kagan::chat::ChatView::new(id.clone(), window, cx))
        .into();
    cx.run_until_parked();
    assert!(!shows(cx, window, "chat-message-0"), "nothing was read");
    emit(cx, agent_said(&id, 1, "the first word"));
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-0").as_deref() == Some("the first word")
    });
}
