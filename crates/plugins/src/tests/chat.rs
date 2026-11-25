use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{
    EventKind, Link, NoticeSeverity, Op, Plugin, Thread, ThreadId, ThreadOp, ThreadStatus,
    Timestamp, TurnOutcome, Visibility,
};
use serde_json::{Value, json};

use super::{
    dispatched, forward, installed, open_kept, probe_plugin, run, sample_manifest, settled, thread,
    write_plugin,
};

const CHAT: &str = "thr_chat";
const HIDDEN: &str = "thr_hidden";

const CHAT_FORWARD: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as chat from "kagan/chat";
import * as threads from "kagan/threads";

const modules = { chat, threads };
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          const { module, call, args } = job.input;
          const value = await modules[module][call](...args);
          reply(job.id, { text: JSON.stringify(value ?? null) });
        } catch (error) { reply(job.id, { text: String(error), isError: true }); }
      }
    });
  }
  render() { return "forward"; }
}
"#;

fn message(text: &str) -> EventKind {
    EventKind::Message {
        role: kg_domain::Role::Agent,
        text: text.into(),
        message_id: None,
        attachments: Vec::new(),
    }
}

fn chat_plugin() -> (tempfile::TempDir, Plugin) {
    let plugin = probe_plugin(&["chat", "threads"], CHAT_FORWARD);
    let mut value = sample_manifest();
    value["powers"] = json!({ "areas": ["chat", "threads"] });
    value["tools"].as_array_mut().unwrap().push(json!({
        "name": "probe",
        "description": "Forward a host call",
        "input": "schemas/any.json",
        "scope": "all"
    }));
    write_plugin(plugin.path(), &value);
    std::fs::write(plugin.path().join("main.js"), CHAT_FORWARD).unwrap();
    let installed = installed(plugin.path());
    (plugin, installed)
}

fn open_chat(cx: &mut TestAppContext, plugin: Plugin) {
    open_seeded(cx, plugin, |_| {});
}

fn open_seeded(cx: &mut TestAppContext, plugin: Plugin, extra: impl FnOnce(&mut kg_store::Store)) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    let mut store = kg_store::Store::open(path.join("kagan.sqlite3")).unwrap();
    store
        .put_project(&kg_domain::Project {
            id: kg_domain::ProjectId("prj_test".into()),
            kind: kg_domain::ProjectKind::Standard,
            name: "Project".into(),
            path: dunce::canonicalize(&path).unwrap(),
            git_remote_url: None,
            sort: 0,
            program: None,
        })
        .unwrap();
    let record = |id: &str, visibility| Thread {
        id: ThreadId(id.into()),
        project_id: kg_domain::ProjectId("prj_test".into()),
        environment_id: None,
        title: "Chat".into(),
        title_locked: false,
        provider: "opencode".into(),
        link: Link::Acp,
        status: ThreadStatus::Idle,
        visibility,
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
    store
        .put_thread(&record(CHAT, Visibility::Visible))
        .unwrap();
    store
        .put_thread(&record(HIDDEN, Visibility::Hidden))
        .unwrap();
    store
        .append_events(
            &ThreadId(CHAT.into()),
            Timestamp(1),
            [
                EventKind::TurnStarted,
                message("say hi"),
                EventKind::Notice {
                    severity: NoticeSeverity::Info,
                    text: "a note".into(),
                },
                EventKind::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    error: None,
                },
            ],
        )
        .unwrap();
    extra(&mut store);
    drop(store);
    open_kept(cx, &path, data, &[plugin]);
}

fn ask(cx: &mut TestAppContext, id: &ThreadId) {
    run(
        cx,
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: vec![
                kg_domain::InteractionOption {
                    id: "approve".into(),
                    label: "Allow".into(),
                    role: Some(kg_domain::OptionRole::Primary),
                },
                kg_domain::InteractionOption {
                    id: "decline".into(),
                    label: "Deny".into(),
                    role: None,
                },
            ],
            fields: Some(Vec::new()),
        }),
    )
    .unwrap();
}

#[gpui_kit::test]
fn items_reads_the_events_threads_events_returns_and_approvals_stay_their_own_read(
    cx: &mut TestAppContext,
) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);
    ask(cx, &ThreadId(CHAT.into()));

    let items = forward(cx, "chat", "items", &json!([CHAT, { "order": "asc" }])).unwrap();
    let events = forward(cx, "threads", "events", &json!([CHAT, { "order": "asc" }])).unwrap();
    assert_eq!(items, events, "a chat reads the log's own events");
    let items = items.as_array().unwrap();
    let types: Vec<&str> = items
        .iter()
        .map(|item| item["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        ["turn.started", "message", "notice", "turn.ended"],
        "{items:?}"
    );
    assert_eq!(items[1]["threadId"], CHAT);
    assert_eq!(items[1]["data"]["text"], "say hi");

    let asked = forward(cx, "chat", "approvals", &json!([CHAT])).unwrap();
    let asked = asked.as_array().unwrap();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0]["title"], "Ship it?");
    assert_eq!(asked[0]["options"][0]["label"], "Allow");
    assert_eq!(asked[0]["options"][0]["role"], "primary");
    assert_eq!(asked[0]["options"][1]["role"], Value::Null);
    assert!(asked[0]["interactionId"].is_string(), "{asked:?}");
    assert!(asked[0].get("seq").is_none(), "{asked:?}");
}

#[gpui_kit::test]
fn items_pages_the_events_after_seq_as_threads_events_does(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);

    let page = forward(
        cx,
        "chat",
        "items",
        &json!([CHAT, { "afterSeq": 2, "limit": 1 }]),
    )
    .unwrap();
    let page = page.as_array().unwrap();
    assert_eq!(page.len(), 1, "{page:?}");
    assert_eq!(page[0]["type"], "notice");
    assert_eq!(page[0]["seq"], 3);
}

#[gpui_kit::test]
fn items_reads_a_hundred_a_call_by_default(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_seeded(cx, plugin, |store| {
        let more: Vec<EventKind> = (0..150).map(|_| message("more")).collect();
        store
            .append_events(&ThreadId(CHAT.into()), Timestamp(2), more)
            .unwrap();
    });

    let page = forward(cx, "chat", "items", &json!([CHAT])).unwrap();
    let page = page.as_array().unwrap();
    assert_eq!(page.len(), 100, "{page:?}");
    assert_eq!(page[0]["seq"], 1);
    assert_eq!(page[99]["seq"], 100);
}

#[gpui_kit::test]
fn items_refuses_more_than_two_hundred_a_call(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);

    let refused = forward(cx, "chat", "items", &json!([CHAT, { "limit": 201 }])).unwrap_err();
    assert!(refused.contains("at most 200 items a call"), "{refused}");
    let most = forward(cx, "chat", "items", &json!([CHAT, { "limit": 200 }])).unwrap();
    assert_eq!(most.as_array().unwrap().len(), 4, "{most}");
}

#[gpui_kit::test]
fn a_chat_call_refuses_a_thread_that_is_not_a_chat(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);
    let terminal = thread().0;

    let items = forward(cx, "chat", "items", &json!([terminal])).unwrap_err();
    assert!(
        items.contains("is not a chat; its transcript is its terminal"),
        "{items}"
    );
    let waited = forward(
        cx,
        "chat",
        "waitItem",
        &json!([terminal, { "afterSeq": 0, "timeoutMs": 10 }]),
    )
    .unwrap_err();
    assert!(waited.contains("is not a chat"), "{waited}");
    let asked = forward(cx, "chat", "approvals", &json!([terminal])).unwrap_err();
    assert!(asked.contains("is not a chat"), "{asked}");
    let sent = forward(cx, "chat", "send", &json!([terminal, "hello"])).unwrap_err();
    assert!(sent.contains("is not a chat"), "{sent}");
    let cancelled = forward(cx, "chat", "cancel", &json!([terminal])).unwrap_err();
    assert!(cancelled.contains("is not a chat"), "{cancelled}");
}

#[gpui_kit::test]
fn a_chat_call_leaves_out_a_hidden_or_archived_thread(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);
    let chat = ThreadId(CHAT.into());

    let hidden = forward(cx, "chat", "items", &json!([HIDDEN])).unwrap_err();
    assert!(hidden.contains("is not in kagan's thread list"), "{hidden}");

    run(cx, Op::Thread(ThreadOp::Archive { id: chat })).unwrap();
    let archived = forward(cx, "chat", "items", &json!([CHAT])).unwrap_err();
    assert!(
        archived.contains("is not in kagan's thread list"),
        "{archived}"
    );
}

#[test]
fn a_wait_runs_for_a_minute_at_most() {
    assert_eq!(
        crate::chat::wait_duration(u64::MAX),
        Duration::from_millis(60_000)
    );
    assert_eq!(crate::chat::wait_duration(5), Duration::from_millis(5));
}

#[gpui_kit::test]
fn wait_item_returns_each_event_in_turn_then_null_at_the_deadline(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);

    let mut after = 0;
    for expected in ["turn.started", "message", "notice", "turn.ended"] {
        let event = forward(
            cx,
            "chat",
            "waitItem",
            &json!([CHAT, { "afterSeq": after, "timeoutMs": 1000 }]),
        )
        .unwrap();
        assert_eq!(event["type"], expected, "{event:?}");
        after = event["seq"].as_u64().unwrap();
    }

    let waiting = dispatched(
        cx,
        super::forwarded(
            "chat",
            "waitItem",
            &json!([CHAT, { "afterSeq": after, "timeoutMs": 50 }]),
        ),
    );
    let value = super::forwarded_reply(settled(cx, &waiting, Duration::from_millis(10))).unwrap();
    assert_eq!(value, Value::Null);
}

#[gpui_kit::test]
fn wait_item_answers_null_when_a_question_is_raised(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);
    let chat = ThreadId(CHAT.into());

    let waiting = dispatched(
        cx,
        super::forwarded(
            "chat",
            "waitItem",
            &json!([CHAT, { "afterSeq": 99, "timeoutMs": 5000 }]),
        ),
    );
    cx.run_until_parked();
    ask(cx, &chat);
    let value = super::forwarded_reply(settled(cx, &waiting, Duration::from_millis(10))).unwrap();
    assert_eq!(value, Value::Null);
}
