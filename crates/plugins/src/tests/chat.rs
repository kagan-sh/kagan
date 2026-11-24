use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{
    Event, EventKind, Link, NoticeSeverity, Op, PlanEntry, PlanStatus, Plugin, Thread, ThreadId,
    ThreadOp, ThreadStatus, Timestamp, ToolKind, ToolOutcome, ToolStatus, TurnOutcome, Visibility,
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

const modules = { chat };
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

fn at(event: EventKind, seq: u64) -> Event {
    Event {
        thread_id: ThreadId(CHAT.into()),
        seq,
        at: Timestamp(seq.cast_signed()),
        kind: event,
    }
}

fn message(text: &str) -> EventKind {
    EventKind::Message {
        role: kg_domain::Role::Agent,
        text: text.into(),
        message_id: None,
        attachments: Vec::new(),
    }
}

#[test]
fn every_kind_a_transcript_draws_maps_and_the_rest_are_left_out() {
    let mapped = |kind| crate::chat::item(&at(kind, 1)).map(|_| ());

    assert!(mapped(message("hi")).is_some());
    assert!(
        mapped(EventKind::Thought {
            text: "thinking".into(),
            message_id: Some("t1".into()),
        })
        .is_some()
    );
    assert!(
        mapped(EventKind::Plan {
            entries: Vec::new()
        })
        .is_some()
    );
    assert!(
        mapped(EventKind::Notice {
            severity: NoticeSeverity::Info,
            text: "note".into()
        })
        .is_some()
    );
    assert!(
        mapped(EventKind::TurnEnded {
            outcome: TurnOutcome::Completed,
            error: None
        })
        .is_some()
    );
    assert!(mapped(delta(None, None, None, None)).is_some());
    // Usage is the header line's, and the rest is turn state the row already shows.
    assert!(mapped(EventKind::Usage { used: 10, size: 20 }).is_none());
    for kind in [
        EventKind::TurnStarted,
        EventKind::Started,
        EventKind::Waiting,
        EventKind::Idle,
        EventKind::Opened,
        EventKind::Rewound { before_seq: 0 },
        // The agent reports the same call in its own `tool` delta, so this one is not an item.
        EventKind::ToolStarted {
            id: "t1".into(),
            name: "skill_load".into(),
        },
        EventKind::ToolEnded {
            id: "t1".into(),
            status: ToolOutcome::Ok,
        },
    ] {
        let shown = format!("{kind:?}");
        assert!(mapped(kind).is_none(), "{shown} is not an item");
    }
}

fn delta(
    kind: Option<ToolKind>,
    status: Option<ToolStatus>,
    input: Option<Value>,
    output: Option<String>,
) -> EventKind {
    EventKind::Tool {
        id: "t1".into(),
        name: Some("read_file".into()),
        kind,
        title: Some("src/main.rs".into()),
        status,
        input,
        output,
        diff: Vec::new(),
        locations: Vec::new(),
        approval: None,
    }
}

fn work_of(kind: EventKind) -> Value {
    serde_json::to_value(crate::chat::item(&at(kind, 1)).unwrap()).unwrap()["work"].clone()
}

#[test]
fn a_tool_delta_carries_its_kind_title_status_and_fields() {
    let input = json!({ "path": "src/main.rs" });
    let running = work_of(delta(
        Some(ToolKind::Read),
        Some(ToolStatus::InProgress),
        Some(input.clone()),
        None,
    ));
    assert_eq!(running["tool"], "read");
    assert_eq!(running["title"], "src/main.rs");
    assert_eq!(running["status"], "running");
    assert_eq!(running["args"], input);
    assert!(running.get("argsTruncated").is_none());

    let ended = work_of(delta(
        None,
        Some(ToolStatus::Completed),
        None,
        Some("one file".into()),
    ));
    assert_eq!(ended["status"], "ok");
    assert_eq!(ended["output"], "one file");
    assert!(ended.get("outputTruncated").is_none());

    // A kind the union has no name for reads as "other", as the design says.
    let failed = work_of(delta(
        Some(ToolKind::SwitchMode),
        Some(ToolStatus::Failed),
        None,
        None,
    ));
    assert_eq!(failed["tool"], "other");
    assert_eq!(failed["status"], "error");
}

/// The classification the host's rows use, on the events a delta sends: the name that arrives in
/// one event names the call, and a kind that arrives in another is read the same way.
#[test]
fn a_work_item_is_classified_as_the_host_draws_it() {
    let named = |name: &str, input: Option<Value>| {
        work_of(EventKind::Tool {
            id: "t1".into(),
            name: Some(name.into()),
            kind: None,
            title: None,
            status: None,
            input,
            output: None,
            diff: Vec::new(),
            locations: Vec::new(),
            approval: None,
        })
    };
    let called = named("mcp__github__create_issue", None);
    assert_eq!(called["tool"], "mcp");
    assert_eq!(called["title"], "github.create_issue");
    let own = named("kagan_open_pull_request", None);
    assert_eq!(own["tool"], "mcp");
    assert_eq!(own["title"], "kagan.open_pull_request");
    let loaded = named("skill_load", Some(json!({ "name": "rust-review" })));
    assert_eq!(loaded["tool"], "skill");
    assert_eq!(loaded["title"], "rust-review");

    // A delta that names neither the call nor its kind reads as "other" and keeps its own title:
    // a page holds the tool and title of the event that named the call.
    let ended = work_of(delta(None, None, None, None));
    assert_eq!(ended["tool"], "other");
    assert_eq!(ended["title"], "src/main.rs");
}

#[test]
fn a_long_args_or_output_is_cut_at_a_letter_and_flagged() {
    let text = "é".repeat(6 * 1024);
    let args = work_of(delta(None, None, Some(json!({ "text": text })), None));
    let cut = args["args"].as_str().unwrap();
    assert_eq!(args["argsTruncated"], true);
    assert!(cut.len() <= 4 * 1024, "args is {} bytes", cut.len());
    assert!(cut.ends_with('é'), "args cut mid-letter");

    let output = work_of(delta(None, None, None, Some(text)));
    assert_eq!(output["outputTruncated"], true);
    assert_eq!(output["output"].as_str().unwrap(), "é".repeat(4096));
}

#[test]
fn a_plan_maps_its_steps_and_their_state() {
    let item = serde_json::to_value(
        crate::chat::item(&at(
            EventKind::Plan {
                entries: vec![
                    PlanEntry {
                        text: "read".into(),
                        status: PlanStatus::Completed,
                    },
                    PlanEntry {
                        text: "write".into(),
                        status: PlanStatus::InProgress,
                    },
                    PlanEntry {
                        text: "test".into(),
                        status: PlanStatus::Pending,
                    },
                ],
            },
            1,
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(item["kind"], "plan");
    assert_eq!(
        item["plan"]["steps"],
        json!([
            { "step": "read", "status": "done" },
            { "step": "write", "status": "running" },
            { "step": "test", "status": "pending" }
        ])
    );
}

/// Kagan's own MCP call is drawn once, from the agent's `tool` delta; the host folds the same way.
#[test]
fn a_call_kagans_own_mcp_made_is_not_a_second_work_item() {
    let started = EventKind::ToolStarted {
        id: "t1".into(),
        name: "skill_load".into(),
    };
    assert!(crate::chat::item(&at(started, 1)).is_none());
    let ended = EventKind::ToolEnded {
        id: "t1".into(),
        status: ToolOutcome::Ok,
    };
    assert!(crate::chat::item(&at(ended, 2)).is_none());
}

#[test]
fn a_message_keeps_its_text_and_id_as_the_log_wrote_them() {
    let item = serde_json::to_value(
        crate::chat::item(&at(
            EventKind::Message {
                role: kg_domain::Role::User,
                text: "say hi".into(),
                message_id: Some("m1".into()),
                attachments: Vec::new(),
            },
            3,
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(item["kind"], "message");
    assert_eq!(item["role"], "user");
    assert_eq!(item["text"], "say hi");
    assert_eq!(item["messageId"], "m1");
    assert_eq!(item["seq"], 3);
}

#[test]
fn a_page_stops_before_the_item_that_would_pass_the_ceiling() {
    let items = |count: usize| {
        (0..count)
            .map(|seq| crate::chat::item(&at(message(&"x".repeat(1024)), seq as u64 + 1)).unwrap())
            .collect::<Vec<_>>()
    };
    let page = |items| crate::chat::page(items).unwrap().as_array().unwrap().len();
    assert_eq!(page(items(3)), 3);
    // 256 KiB of 1 KiB messages is far more than 200; the ceiling stops the page instead.
    assert!(page(items(400)) < 400);
    // One item that is over the ceiling on its own still comes back, or it never would.
    let huge = vec![
        crate::chat::item(&at(message(&"x".repeat(300 * 1024)), 1)).unwrap(),
        crate::chat::item(&at(message("after"), 2)).unwrap(),
    ];
    assert_eq!(page(huge), 1);
}

fn chat_plugin() -> (tempfile::TempDir, Plugin) {
    let plugin = probe_plugin(&["chat"], CHAT_FORWARD);
    let mut value = sample_manifest();
    value["powers"] = json!({ "areas": ["chat"] });
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

/// A window whose data folder already holds a chat thread with a message, a notice and a turn,
/// and a hidden chat thread.
fn open_chat(cx: &mut TestAppContext, plugin: Plugin) {
    open_chat_with(cx, plugin, 0);
}

/// `notices` more items after the seeded three, to page past the 100 a call returns by default.
fn open_chat_with(cx: &mut TestAppContext, plugin: Plugin, notices: usize) {
    open_seeded(cx, plugin, |store| {
        store
            .append_events(
                &ThreadId(CHAT.into()),
                Timestamp(2),
                (0..notices).map(|ix| EventKind::Notice {
                    severity: NoticeSeverity::Info,
                    text: format!("more {ix}"),
                }),
            )
            .unwrap();
    });
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
fn items_reads_a_chats_log_and_approvals_are_their_own_read(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);
    ask(cx, &ThreadId(CHAT.into()));

    let items = forward(cx, "chat", "items", &json!([CHAT, { "order": "asc" }])).unwrap();
    let items = items.as_array().unwrap().clone();
    let kinds: Vec<&str> = items
        .iter()
        .map(|item| item["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["message", "notice", "turn"], "{items:?}");
    assert_eq!(items[0]["threadId"], CHAT);

    // An approval is not a log item, whichever page the caller reads.
    let newest = forward(cx, "chat", "items", &json!([CHAT])).unwrap();
    let newest = newest.as_array().unwrap();
    assert_eq!(newest[0]["kind"], "turn", "{newest:?}");
    let past = forward(cx, "chat", "items", &json!([CHAT, { "afterSeq": 99 }])).unwrap();
    assert_eq!(past, json!([]));

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
fn items_refuses_more_than_its_limit_and_a_thread_that_is_not_a_chat(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);

    let refused = forward(cx, "chat", "items", &json!([CHAT, { "limit": 201 }])).unwrap_err();
    assert!(refused.contains("at most 200 items a call"), "{refused}");

    let terminal = forward(cx, "chat", "items", &json!([thread().0])).unwrap_err();
    assert!(
        terminal.contains("is not a chat; its transcript is its terminal"),
        "{terminal}"
    );

    let asked = forward(cx, "chat", "approvals", &json!([thread().0])).unwrap_err();
    assert!(asked.contains("is not a chat"), "{asked}");

    let sent = forward(cx, "chat", "send", &json!([thread().0, "hello"])).unwrap_err();
    assert!(sent.contains("is not a chat"), "{sent}");
    let cancelled = forward(cx, "chat", "cancel", &json!([thread().0])).unwrap_err();
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
fn wait_item_reads_past_a_full_batch_of_events_that_are_not_items(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_seeded(cx, plugin, |store| {
        store
            .append_events(
                &ThreadId(CHAT.into()),
                Timestamp(2),
                (0..250).map(|_| EventKind::Idle),
            )
            .unwrap();
        store
            .append_events(
                &ThreadId(CHAT.into()),
                Timestamp(3),
                [message("after the batch")],
            )
            .unwrap();
    });

    let item = forward(
        cx,
        "chat",
        "waitItem",
        &json!([CHAT, { "afterSeq": 4, "timeoutMs": 50 }]),
    )
    .unwrap();
    assert_eq!(item["kind"], "message", "{item:?}");
    assert_eq!(item["text"], "after the batch");
}

#[gpui_kit::test]
fn wait_item_returns_the_next_one_then_null_at_the_deadline(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat(cx, plugin);

    let first = forward(
        cx,
        "chat",
        "waitItem",
        &json!([CHAT, { "afterSeq": 0, "timeoutMs": 1000 }]),
    )
    .unwrap();
    assert_eq!(first["kind"], "message");

    let last = forward(
        cx,
        "chat",
        "waitItem",
        &json!([CHAT, { "afterSeq": first["seq"], "timeoutMs": 1000 }]),
    )
    .unwrap();
    assert_eq!(last["kind"], "notice");

    let ended = forward(
        cx,
        "chat",
        "waitItem",
        &json!([CHAT, { "afterSeq": last["seq"], "timeoutMs": 1000 }]),
    )
    .unwrap();
    assert_eq!(ended["kind"], "turn");

    let waiting = dispatched(
        cx,
        super::forwarded(
            "chat",
            "waitItem",
            &json!([CHAT, { "afterSeq": ended["seq"], "timeoutMs": 50 }]),
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

#[gpui_kit::test]
fn items_reads_a_hundred_a_call_by_default(cx: &mut TestAppContext) {
    let (_dir, plugin) = chat_plugin();
    open_chat_with(cx, plugin, 150);
    let page = forward(cx, "chat", "items", &json!([CHAT])).unwrap();
    assert_eq!(page.as_array().unwrap().len(), 100, "{page:?}");
}
