use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{
    Link, Op, OpResult, Plugin, PluginId, PluginOp, PreferenceOp, Slot, Thread, ThreadId, ThreadOp,
    ThreadStatus, Timestamp, Visibility,
};
use serde_json::json;

use super::replace::answer_standalone;
use crate::kagan::{Kagan, replace_read};
use crate::tests::kagan::{
    chord, click, label, open_with_plugins_then, plugin_log, shows, value_of, wait_until,
};

const SWAP: &str = "dev.test.chat";
const CHAT: &str = "thr_chat";
const SHELL: &str = "thr_shell";

const BODY: &str = "return div()
      .track_focus(this.focus)
      .children(this.buttons());";

const THROWS: &str = r#"throw new Error("transcript exploded");"#;

const GRABS: &str = r#"return div()
      .track_focus(this.focus)
      .on_key_down((_event, cx) => cx.stop_propagation())
      .child(new KgButton("chat-body").label("Transcript"));"#;

const GRAB_EVERY_FRAME: &str = r"cx.spawn(async (cx) => {
      for (;;) {
        this.focus.focus();
        await cx.sleep(16);
      }
    });";

fn source(body: &str) -> String {
    let grab = if body.contains("track_focus") {
        GRAB_EVERY_FRAME
    } else {
        ""
    };
    format!(
        r#"// @ts-check
import {{ View, div }} from "gpui-kit";
import {{ KgButton }} from "gpui-component";
import {{ context, onContextChanged }} from "kagan/host";
import {{ draft, setDraft }} from "kagan/chat";
import {{ next, reply, fail }} from "kagan/jobs";

export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        Promise.resolve()
          .then(() => draft(job.threadId))
          .then((text) => setDraft(job.threadId, "improve: " + text))
          .then(() => reply(job.id, {{ text: "ok" }}), (error) => fail(job.id, String(error)));
      }}
    }});
  }}
  render() {{ return ""; }}
}}

export class Transcript extends View {{
  init(_props, cx) {{
    this.focus = cx.focus_handle();
    {grab}
    this.said = "";
    const show = (value) => {{ this.said = String(value); cx.notify(); }};
    const call = (send) => Promise.resolve().then(send).then(show, show);
    const thread = () => (context().props || {{}}).threadId;
    this.buttons = () => [
      new KgButton("chat-body").label("Transcript"),
      new KgButton("chat-draft").label("Read").on_click(() => call(() => draft(thread()))),
      new KgButton("chat-set").label("Write").on_click(() => call(() => setDraft(thread(), "written"))),
      new KgButton("chat-elsewhere").label("Other").on_click(() => call(() => setDraft("{SHELL}", "x"))),
      new KgButton("chat-huge").label("Huge").on_click(() => call(() => setDraft(thread(), "x".repeat(1048577)))),
      new KgButton("chat-said").label(this.said),
    ];
    cx.spawn(async () => {{
      for (;;) {{
        await onContextChanged();
        cx.notify();
      }}
    }});
  }}
  render() {{
    {body}
  }}
}}
"#
    )
}

fn seed(path: &Path) {
    let mut store = kg_store::Store::open(path.join("kagan.sqlite3")).unwrap();
    let record = |id: &str, link| Thread {
        id: ThreadId(id.into()),
        project_id: kg_domain::ProjectId("prj_kagan".into()),
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: "opencode".into(),
        link,
        status: ThreadStatus::Idle,
        visibility: Visibility::Visible,
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
    };
    store.put_thread(&record(CHAT, Link::Acp)).unwrap();
    store.put_thread(&record(SHELL, Link::Terminal)).unwrap();
    store
        .append_events(
            &ThreadId(CHAT.into()),
            Timestamp(1),
            [kg_domain::EventKind::Message {
                role: kg_domain::Role::Agent,
                text: "hi there".into(),
                message_id: None,
                attachments: Vec::new(),
            }],
        )
        .unwrap();
}

fn install(path: &Path, id: &str, prompts: usize, body: &str) {
    seed(path);
    let root = path.join(id);
    std::fs::create_dir(&root).unwrap();
    let commands: Vec<serde_json::Value> = (0..prompts)
        .map(|ix| {
            json!({
                "id": format!("improve{ix}"),
                "title": format!("Improve{ix}"),
                "when": "thread",
                "prompt": true,
            })
        })
        .collect();
    let manifest = json!({
        "id": id,
        "name": "Chat",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Draws a chat's transcript.",
        "powers": {"areas": ["chat"]},
        "commands": commands,
        "ui": {"panels": [{
            "id": "transcript",
            "title": "Chat",
            "view": "Transcript",
            "placement": "center",
            "replaces": "chat",
            "follows": "thread"
        }]}
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), source(body)).unwrap();
    let store = kg_store::Store::open(path.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {"areas": ["chat"]}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

fn opened(
    cx: &mut TestAppContext,
    body: &'static str,
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let (kagan, window, data) = open_with_plugins_then(cx, |data| install(data, SWAP, 1, body));
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    (kagan, window, data)
}

fn run(cx: &mut TestAppContext, op: Op) -> Result<OpResult, kg_domain::OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task)
}

fn choose(cx: &mut TestAppContext, slot: Slot, id: &str) {
    run(
        cx,
        Op::Plugin(PluginOp::Use {
            slot,
            id: id.parse().unwrap(),
            thread_id: None,
        }),
    )
    .unwrap();
    cx.run_until_parked();
}

fn saved(cx: &mut TestAppContext, slot: Slot) -> serde_json::Value {
    match run(
        cx,
        Op::Preference(PreferenceOp::Get {
            key: slot.preference().to_owned(),
        }),
    )
    .unwrap()
    {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn filled(cx: &mut TestAppContext, kagan: &Entity<Kagan>, slot: Slot) -> bool {
    cx.update(|cx| replace_read::filled(kagan.read(cx), slot))
}

fn bounds(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> gpui_kit::Bounds<f32> {
    cx.update_window(window, |_, window, _| {
        let bounds =
            crate::ui_registry::find(window, &gpui_kit::ElementId::Name(id.to_owned().into()))
                .unwrap_or_else(|| panic!("{id} is not drawn"))
                .bounds;
        gpui_kit::Bounds::new(
            gpui_kit::point(f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
            gpui_kit::size(f32::from(bounds.size.width), f32::from(bounds.size.height)),
        )
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_persons_choice_draws_a_chats_body_and_a_terminal_takes_it_away(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, BODY);
    choose(cx, Slot::Chat, SWAP);
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));
    assert!(filled(cx, &kagan, Slot::Chat));

    click(cx, window, &format!("thread-{SHELL}"));
    wait_until(cx, |cx| !shows(cx, window, "chat-body"));
    assert!(!filled(cx, &kagan, Slot::Chat));

    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));
    assert!(filled(cx, &kagan, Slot::Chat));
    assert!(!shows(cx, window, "chat-transcript"));
}

#[gpui_kit::test]
fn draft_reads_and_writes_the_prompt_box_and_refuses_a_thread_it_does_not_show(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, _data) = opened(cx, BODY);
    choose(cx, Slot::Chat, SWAP);
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));

    click(cx, window, "chat-set");
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-prompt").as_deref() == Some("written")
    });

    click(cx, window, "chat-draft");
    wait_until(cx, |cx| label(cx, window, "chat-said") == "written");

    click(cx, window, "chat-elsewhere");
    wait_until(cx, |cx| {
        label(cx, window, "chat-said").contains("is not on screen")
    });

    click(cx, window, "chat-huge");
    wait_until(cx, |cx| {
        label(cx, window, "chat-said").contains("over 1 MiB")
    });
    wait_until(cx, |cx| {
        label(cx, window, "chat-said").contains("over 1 MiB")
    });
    // A refused write leaves the box as it was.
    assert_eq!(
        value_of(cx, window, "chat-prompt").as_deref(),
        Some("written")
    );
}

#[gpui_kit::test]
fn a_draft_for_an_archived_chat_is_refused(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, BODY);
    choose(cx, Slot::Chat, SWAP);
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));

    run(
        cx,
        Op::Thread(ThreadOp::Archive {
            id: ThreadId(CHAT.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();

    click(cx, window, "chat-draft");
    wait_until(cx, |cx| {
        label(cx, window, "chat-said").contains("is not in kagan's thread list")
    });

    click(cx, window, "chat-set");
    wait_until(cx, |cx| {
        label(cx, window, "chat-said").contains("is not in kagan's thread list")
    });
    assert_eq!(
        value_of(cx, window, "chat-prompt").as_deref(),
        Some(""),
        "the archived chat left the screen, so the refusal proves nothing"
    );
}

#[gpui_kit::test]
fn a_chat_plugin_that_throws_goes_back_to_the_built_in_and_says_why(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, THROWS);
    choose(cx, Slot::Chat, SWAP);
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| saved(cx, Slot::Chat) == json!("kagan"));
    assert!(shows(cx, window, "chat-transcript"));
    assert!(!filled(cx, &kagan, Slot::Chat));
    let log = plugin_log(cx, SWAP);
    assert!(
        log.contains("The replaceable chat is back to built-in: its view threw"),
        "{log}"
    );
    assert!(log.contains("transcript exploded"), "{log}");
}

#[gpui_kit::test]
fn the_built_in_chat_key_works_while_the_plugin_holds_focus(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, GRABS);
    choose(cx, Slot::Chat, SWAP);
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));
    click(cx, window, "chat-body");
    wait_until(cx, |cx| {
        cx.update_window(window, |_, window, cx| {
            replace_read::holds_focus(kagan.read(cx), Slot::Chat, window, cx)
        })
        .unwrap()
    });
    cx.simulate_keystrokes(window, &chord("UseBuiltInChat"));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    assert!(!filled(cx, &kagan, Slot::Chat));
    assert_eq!(saved(cx, Slot::Chat), json!("kagan"));
}

#[gpui_kit::test]
fn a_prompt_button_sits_before_send_and_runs_for_the_shown_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, BODY);
    choose(cx, Slot::Chat, SWAP);
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));

    let prompt = format!("prompt/{SWAP}/improve0");
    assert!(shows(cx, window, &prompt));
    assert!(bounds(cx, window, &prompt).origin.x < bounds(cx, window, "chat-send").origin.x);

    click(cx, window, &prompt);
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-prompt").as_deref() == Some("improve: ")
    });
}

#[gpui_kit::test]
fn the_window_draws_four_prompt_buttons_at_most(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install(data, "dev.test.a", 4, BODY);
        install(data, "dev.test.b", 1, BODY);
    });
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    for ix in 0..4 {
        let drawn = format!("prompt/dev.test.a/improve{ix}");
        assert!(shows(cx, window, &drawn), "{drawn} is not drawn");
    }
    assert!(!shows(cx, window, "prompt/dev.test.b/improve0"));
}

#[gpui_kit::test]
fn a_chat_choice_from_outside_the_window_asks_the_person(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, BODY);
    let (_keep, gone) = async_channel::bounded::<()>(1);
    let from_terminal = |cx: &mut TestAppContext, op: Op| {
        let gone = gone.clone();
        cx.update(|cx| Workspace::dispatch_for(op, Some(kg_core::Requester::new(gone)), cx))
    };
    let choosing = Op::Plugin(PluginOp::Use {
        slot: Slot::Chat,
        id: SWAP.parse().unwrap(),
        thread_id: None,
    });
    let declined = from_terminal(cx, choosing.clone());
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());
    cx.run_until_parked();
    assert_eq!(saved(cx, Slot::Chat), serde_json::Value::Null);
    assert!(!filled(cx, &kagan, Slot::Chat));

    let approved = from_terminal(cx, choosing);
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(approved).unwrap();
    click(cx, window, &format!("thread-{CHAT}"));
    wait_until(cx, |cx| shows(cx, window, "chat-body"));
    assert!(filled(cx, &kagan, Slot::Chat));

    // `chat kagan` is the built-in, so it asks nothing and draws the transcript again.
    run(
        cx,
        Op::Plugin(PluginOp::Use {
            slot: Slot::Chat,
            id: "kagan".parse().unwrap(),
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    assert_eq!(saved(cx, Slot::Chat), json!("kagan"));
}
