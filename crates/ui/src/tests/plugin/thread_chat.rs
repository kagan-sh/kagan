use std::path::Path;

use gpui_kit::{AnyWindowHandle, TestAppContext};
use kg_core::Workspace;
use kg_domain::{
    Event, EventKind, InteractionOption, Link, Op, Plugin, PluginId, PluginOp, Role, Slot, Thread,
    ThreadId, ThreadOp, ThreadStatus, Timestamp, ToolCall, ToolKind, ToolStatus, Visibility,
};
use serde_json::json;

use super::surfaces::{body, open_surface, wide};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, plugin_log, press_key, shows, shows_within,
    value_of, wait_until,
};

const WRITER: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
import { KgButton, ThreadChat } from "gpui-component";
import { setDraft } from "kagan/chat";

export default class Plugin extends View {
  render() { return ""; }
}

export class Writer extends View {
  init(_props, cx) { this.focus = cx.focus_handle(); }
  render() {
    return div()
      .track_focus(this.focus)
      .child(new KgButton("write-draft").label("Write").on_click(() => Promise.resolve()
        .then(() => setDraft("thr_chat", "from the panel"))
        .catch(() => {})))
      .child(new ThreadChat("thr_chat"));
  }
}
"#;

const PLUGIN: &str = "dev.test.threads";
const CHAT: &str = "thr_chat";
const SHELL: &str = "thr_shell";
const STOPPED: &str = "thr_stopped";
const ASKING: &str = "thr_asking";
const OLD: &str = "thr_old";

const SOURCE: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
import { ThreadChat } from "gpui-component";

export default class Plugin extends View {
  render() { return ""; }
}

export class Side extends View {
  render() { return new ThreadChat("thr_chat"); }
}

export class Page extends View {
  render() { return div().size_full().child(new ThreadChat("thr_chat")); }
}

export class Shell extends View {
  render() { return div().child(new ThreadChat("thr_shell")); }
}

export class AskingCard extends View {
  render() { return div().child(new ThreadChat("thr_asking").compact()); }
}

export class StoppedCard extends View {
  render() { return div().child(new ThreadChat("thr_stopped").compact()); }
}

export class OldCard extends View {
  render() { return div().child(new ThreadChat("thr_old").compact()); }
}
"#;

fn panel(id: &str, view: &str, placement: &str, replaces: Option<&str>) -> serde_json::Value {
    let mut panel = json!({
        "id": id,
        "title": view,
        "view": view,
        "placement": placement,
        "follows": "thread",
    });
    if let Some(slot) = replaces {
        panel["replaces"] = json!(slot);
    }
    panel
}

fn install(data: &Path, areas: &[&str], panels: &[serde_json::Value]) {
    install_source(data, areas, panels, SOURCE);
}

// The writer listens for a draft write, so it needs the `chat` module in its own source.
fn install_writer(data: &Path, areas: &[&str]) {
    install_source(
        data,
        areas,
        &[panel("writer", "Writer", "right", None)],
        WRITER,
    );
}

fn install_source(data: &Path, areas: &[&str], panels: &[serde_json::Value], source: &str) {
    let root = data.join(PLUGIN);
    std::fs::create_dir(&root).unwrap();
    let manifest = json!({
        "id": PLUGIN,
        "name": "Threads",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Draws a thread's chat.",
        "powers": {"areas": areas},
        "ui": {"panels": panels},
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), source).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(PLUGIN.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {"areas": areas}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

fn side(data: &Path) {
    install(data, &["chat"], &[panel("side", "Side", "right", None)]);
}

fn writer(data: &Path) {
    install_writer(data, &["chat"]);
}

fn page(data: &Path) {
    install(data, &["chat"], &[panel("page", "Page", "center", None)]);
}

fn chat_page(data: &Path) {
    install(
        data,
        &["chat"],
        &[panel("page", "Page", "center", Some("chat"))],
    );
}

fn thread(id: &str, link: Link, status: ThreadStatus) -> Thread {
    Thread {
        id: ThreadId(id.into()),
        project_id: kg_domain::ProjectId("prj_kagan".into()),
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: "opencode".into(),
        link,
        status,
        visibility: Visibility::Visible,
        parent_thread_id: None,
        quiet: false,
        notify: kg_domain::ThreadNotify::Default,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned: false,
        last_read_at: Some(Timestamp(0)),
        latest_attention_at: Timestamp(0),
        mark: kg_domain::ThreadMark::None,
        archived_at: None,
        created_at: Timestamp(0),
        updated_at: Timestamp(0),
    }
}

fn said(text: &str) -> EventKind {
    EventKind::Message {
        role: Role::Agent,
        text: text.into(),
        message_id: None,
        attachments: Vec::new(),
    }
}

fn tool(id: &str, status: ToolStatus, command: &str) -> EventKind {
    EventKind::Tool(ToolCall {
        id: id.into(),
        name: None,
        kind: Some(ToolKind::Execute),
        title: None,
        status: Some(status),
        input: Some(json!({ "command": command })),
        output: None,
        diff: Vec::new(),
        locations: Vec::new(),
        approval: None,
    })
}

fn seed(data: &Path) {
    let mut store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    for id in [CHAT, ASKING, OLD, STOPPED] {
        store
            .put_thread(&thread(id, Link::Acp, ThreadStatus::Idle))
            .unwrap();
    }
    store
        .put_thread(&thread(SHELL, Link::Terminal, ThreadStatus::Idle))
        .unwrap();
    store
        .append_events(&ThreadId(CHAT.into()), Timestamp(1), [said("hi there")])
        .unwrap();
    store
        .append_events(
            &ThreadId(STOPPED.into()),
            Timestamp(1),
            [EventKind::Exited {
                code: Some(1),
                signal: None,
            }],
        )
        .unwrap();
    store
        .append_events(
            &ThreadId(ASKING.into()),
            Timestamp(1),
            [tool("call_1", ToolStatus::Pending, "rm -rf node_modules")],
        )
        .unwrap();
    // The only answer is older than the newest 50 events, so a bounded read cannot see it.
    store
        .append_events(
            &ThreadId(OLD.into()),
            Timestamp(1),
            [said("the first answer")],
        )
        .unwrap();
    let rest = (0..60)
        .map(|ix| tool(&format!("old_{ix}"), ToolStatus::Completed, "ls"))
        .collect::<Vec<_>>();
    store
        .append_events(&ThreadId(OLD.into()), Timestamp(2), rest)
        .unwrap();
}

fn opened(
    cx: &mut TestAppContext,
    curate: impl FnOnce(&Path),
) -> (AnyWindowHandle, tempfile::TempDir) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |data| {
        seed(data);
        curate(data);
    });
    wide(cx, window);
    (window, data)
}

fn run(cx: &mut TestAppContext, op: Op) -> Result<kg_domain::OpResult, kg_domain::OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task)
}

fn saved_chat(cx: &mut TestAppContext) -> serde_json::Value {
    match run(
        cx,
        Op::Preference(kg_domain::PreferenceOp::Get {
            key: Slot::Chat.preference().to_owned(),
        }),
    )
    .unwrap()
    {
        kg_domain::OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn emit(cx: &mut TestAppContext, id: &str, seq: u64, kind: EventKind) {
    let thread = ThreadId(id.into());
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |_, cx| {
            cx.emit(kg_core::WorkspaceEvent::Event {
                event: Box::new(Event {
                    thread_id: thread,
                    seq,
                    at: Timestamp(2),
                    kind,
                }),
                project_id: kg_domain::ProjectId("prj_kagan".into()),
            });
        });
    });
}

#[gpui_kit::test]
fn a_right_panel_draws_a_chats_transcript_and_its_box(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, side);
    open_surface(cx, PLUGIN, "side").unwrap();
    // The center shows a terminal thread, so the embed owns the only prompt box.
    click(cx, window, &format!("thread-{SHELL}"));
    wait_until(cx, |cx| {
        shows_within(cx, window, "chat-prompt", &body(PLUGIN, "side"))
    });
    assert!(shows_within(cx, window, "chat-box", &body(PLUGIN, "side")));
    assert!(shows_within(
        cx,
        window,
        "chat-start-again",
        &body(PLUGIN, "side")
    ));
    assert!(shows_within(
        cx,
        window,
        "chat-transcript",
        &body(PLUGIN, "side")
    ));
    assert!(!shows_within(cx, window, "chat-transcript", "center"));
}

#[gpui_kit::test]
fn an_embed_whose_thread_is_centered_says_reply_in_the_center(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, writer);
    open_surface(cx, PLUGIN, "writer").unwrap();
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| {
        shows_within(cx, window, "chat-transcript", "center")
    });
    wait_until(cx, |cx| {
        shows_within(cx, window, "chat-reply-in-center", &body(PLUGIN, "writer"))
    });
    assert!(
        !shows_within(cx, window, "chat-prompt", &body(PLUGIN, "writer")),
        "the embed drew a second box for a thread the center shows"
    );
    click(cx, window, "chat-reply-in-center");
    wait_until(cx, |cx| focused_of(cx, window, "chat-prompt") == Some(true));
}

#[gpui_kit::test]
fn a_draft_from_a_panel_reaches_the_box_that_is_on_screen(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, writer);
    open_surface(cx, PLUGIN, "writer").unwrap();
    click(cx, window, &format!("thread-{SHELL}"));
    wait_until(cx, |cx| {
        shows_within(cx, window, "chat-prompt", &body(PLUGIN, "writer"))
    });
    click(cx, window, "write-draft");
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-prompt").as_deref() == Some("from the panel")
    });

    // With the chat in the center, the same write reaches the center's box instead.
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| {
        shows_within(cx, window, "chat-transcript", "center")
    });
    click(cx, window, "write-draft");
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-prompt").as_deref() == Some("from the panel")
    });
}

#[gpui_kit::test]
fn escape_in_a_page_embed_closes_the_page(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, page);
    open_surface(cx, PLUGIN, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    wait_until(cx, |cx| shows(cx, window, "chat-prompt"));
    click(cx, window, "chat-prompt");
    wait_until(cx, |cx| focused_of(cx, window, "chat-prompt") == Some(true));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "surface-page"));
}

#[gpui_kit::test]
fn a_terminal_thread_answers_with_one_line(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, |data| {
        install(data, &["chat"], &[panel("shell", "Shell", "right", None)]);
    });
    open_surface(cx, PLUGIN, "shell").unwrap();
    click(cx, window, &format!("thread-{SHELL}"));
    wait_until(cx, |cx| shows(cx, window, "thread-chat-refused"));
    assert_eq!(
        label(cx, window, "thread-chat-refused"),
        "This thread is not a chat."
    );
    assert!(!shows(cx, window, "chat-box"));
}

#[gpui_kit::test]
fn without_the_chat_area_the_component_says_what_it_needs(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, |data| {
        install(data, &[], &[panel("side", "Side", "right", None)]);
    });
    open_surface(cx, PLUGIN, "side").unwrap();
    click(cx, window, &format!("thread-{SHELL}"));
    wait_until(cx, |cx| shows(cx, window, "thread-chat-refused"));
    assert_eq!(
        label(cx, window, "thread-chat-refused"),
        "ThreadChat needs \"chat\" in powers.areas."
    );
    assert!(!shows(cx, window, "chat-box"));
}

#[gpui_kit::test]
fn a_chat_slot_cannot_hold_a_chat_and_the_built_in_returns(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, chat_page);
    run(
        cx,
        Op::Plugin(PluginOp::Use {
            slot: Slot::Chat,
            id: PLUGIN.parse().unwrap(),
            thread_id: None,
        }),
    )
    .unwrap();
    cx.run_until_parked();
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| saved_chat(cx) == json!("kagan"));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    let log = plugin_log(cx, PLUGIN);
    assert!(
        log.contains("uses ThreadChat, which draws the chat it stands in for"),
        "{log}"
    );
}

#[gpui_kit::test]
fn a_card_needing_you_names_the_call_that_waits(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, |data| {
        install(
            data,
            &["chat"],
            &[panel("asking", "AskingCard", "right", None)],
        );
    });
    open_surface(cx, PLUGIN, "asking").unwrap();
    let card = format!("thread-chat-{ASKING}");
    wait_until(cx, |cx| shows(cx, window, &card));
    run(
        cx,
        Op::Thread(ThreadOp::Ask {
            id: ThreadId(ASKING.into()),
            title: "Keep going?".into(),
            options: ["yes", "no"]
                .into_iter()
                .map(|id| InteractionOption {
                    id: id.into(),
                    label: id.into(),
                    role: None,
                })
                .collect(),
            fields: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| {
        label(cx, window, &card) == "Thread chat: needs you"
    });
    wait_until(cx, |cx| {
        value_of(cx, window, &format!("{card}-step")).as_deref()
            == Some("Waiting to run command  rm -rf node_modules")
    });

    emit(
        cx,
        ASKING,
        2,
        tool("call_2", ToolStatus::InProgress, "npm test -- login"),
    );
    wait_until(cx, |cx| {
        value_of(cx, window, &format!("{card}-step")).as_deref()
            == Some("Running command  npm test -- login")
    });
    emit(
        cx,
        ASKING,
        3,
        said("Found it: the test clicks Sign in too early."),
    );
    wait_until(cx, |cx| {
        value_of(cx, window, &format!("{card}-said")).as_deref()
            == Some("Found it: the test clicks Sign in too early.")
    });
}

#[gpui_kit::test]
fn a_stopped_card_says_the_end_and_opens_the_thread(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, |data| {
        install(
            data,
            &["chat"],
            &[panel("stopped", "StoppedCard", "right", None)],
        );
    });
    open_surface(cx, PLUGIN, "stopped").unwrap();
    let card = format!("thread-chat-{STOPPED}");
    wait_until(cx, |cx| shows(cx, window, &card));
    assert_eq!(label(cx, window, &card), "Thread chat: ended");
    assert_eq!(
        value_of(cx, window, &format!("{card}-step")).as_deref(),
        Some("Exited with code 1.")
    );
    click(cx, window, &format!("{card}-open"));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    assert!(shows(cx, window, &card), "the card left with the panel");
}

#[gpui_kit::test]
fn a_card_reads_only_the_newest_events(cx: &mut TestAppContext) {
    let (window, _data) = opened(cx, |data| {
        install(data, &["chat"], &[panel("old", "OldCard", "right", None)]);
    });
    open_surface(cx, PLUGIN, "old").unwrap();
    let card = format!("thread-chat-{OLD}");
    wait_until(cx, |cx| shows(cx, window, &card));
    wait_until(cx, |cx| label(cx, window, &card) == "Thread chat: idle");
    assert!(
        !shows(cx, window, &format!("{card}-said")),
        "an answer older than the newest 50 events was read"
    );
}
