//! One agent that answers over its own pipes, and the thread that talks to it. Slice 1 speaks
//! text only: a prompt in, message chunks and a stop reason out.

use std::io::{BufRead as _, Write as _};
use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use kg_domain::*;
use serde_json::{Value, json};

use super::{Fixture, alive, events, gone, kinds_of, op, thread, wait_for_status};
use crate::Workspace;

/// One recorded turn: the chunks the agent says, and why it stopped. `hold` keeps the prompt
/// open until a cancel; `sleep` spawns a child that must die with the agent.
fn turn(chunks: &[&str], stop: &str) -> Value {
    json!({ "chunks": chunks, "stopReason": stop })
}

/// A turn recorded as raw ACP updates, then the stop reason.
fn transcript(updates: &Value, stop: &str) -> Value {
    json!({ "updates": updates, "stopReason": stop })
}

/// A turn that asks for permission before it finishes. `options` are the agent's own.
fn asking(tool: &Value, options: &Value, stop: &str) -> Value {
    json!({
        "updates": [tool],
        "permission": {
            "toolCall": {"toolCallId": "call_1", "title": "Run the tests", "kind": "execute"},
            "options": options,
        },
        "stopReason": stop,
    })
}

fn holding() -> Value {
    json!({ "chunks": [], "hold": true })
}

fn spawning() -> Value {
    json!({ "chunks": [], "stopReason": "end_turn", "sleep": true })
}

struct Agent {
    dir: tempfile::TempDir,
    previous: Option<std::ffi::OsString>,
}

impl Agent {
    /// Records `sessions` and `turns`, and puts a program named `opencode` on the thread's line
    /// that re-runs this test binary as the agent.
    fn new(sessions: &[&str], turns: &[Value]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sessions"), sessions.join("\n") + "\n").unwrap();
        let turns: Vec<String> = turns
            .iter()
            .map(Value::to_string)
            .map(|line| line + "\n")
            .collect();
        std::fs::write(dir.path().join("turns.ndjson"), turns.concat()).unwrap();
        let program = dir.path().join("opencode");
        let exe = std::env::current_exe().unwrap();
        // Quiet libtest still prints a progress dot: a line that is not JSON is skipped below.
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nexec '{}' -q --exact tests::acp::fake_agent --nocapture\n",
                exe.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let previous = std::env::var_os("KG_FAKE_ACP_DIR");
        // SAFETY: nextest runs each test in its own process, so this test owns the variable while
        // the guard is alive, and the agent inherits it.
        unsafe {
            std::env::set_var("KG_FAKE_ACP_DIR", dir.path());
        }
        Self { dir, previous }
    }

    fn program(&self) -> String {
        self.dir.path().join("opencode").display().to_string()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn pid(&self, name: &str, cx: &mut TestAppContext) -> u32 {
        let file = self.path(name);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            cx.run_until_parked();
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            if let Ok(pid) = text.trim().parse() {
                return pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the agent never wrote {}",
                file.display()
            );
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        // SAFETY: see `new`.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("KG_FAKE_ACP_DIR", value),
                None => std::env::remove_var("KG_FAKE_ACP_DIR"),
            }
        }
    }
}

async fn started(fixture: &Fixture, agent: &Agent, cx: &mut TestAppContext) -> ThreadId {
    let OpResult::Thread(started) = op(fixture.run_acp(&agent.program()), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Active, cx).await;
    started.id
}

async fn wait_for(
    id: &ThreadId,
    want: impl Fn(&[EventKind]) -> bool,
    cx: &mut TestAppContext,
) -> Vec<EventKind> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let kinds = kinds_of(id, cx).await;
        if want(&kinds) {
            return kinds;
        }
        assert!(std::time::Instant::now() < deadline, "events: {kinds:?}");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn count(kinds: &[EventKind], want: impl Fn(&EventKind) -> bool) -> usize {
    kinds.iter().filter(|kind| want(kind)).count()
}

fn ended(kinds: &[EventKind], outcome: TurnOutcome) -> bool {
    kinds
        .iter()
        .any(|kind| matches!(kind, EventKind::TurnEnded { outcome: got, .. } if *got == outcome))
}

/// What kagan said went wrong, if it recorded a failure.
fn failed(kinds: &[EventKind]) -> Option<String> {
    kinds.iter().find_map(|kind| match kind {
        EventKind::Failed { error } => error.clone(),
        _ => None,
    })
}

fn agent_said(kinds: &[EventKind]) -> String {
    kinds
        .iter()
        .filter_map(|kind| match kind {
            EventKind::Message {
                role: Role::Agent,
                text,
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[gpui_kit::test]
async fn acp_input_streams_messages_and_ends_the_turn(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_alpha"], &[turn(&["H", "ello"], "end_turn")]);
    let id = started(&fixture, &agent, cx).await;
    let link = thread(&id, cx).await.link;
    assert_eq!(link, Link::Acp);
    op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            terminal: None,
            text: "hi".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert!(agent_said(&kinds).eq("Hello"), "{kinds:?}");
    assert_eq!(
        count(&kinds, |kind| matches!(kind, EventKind::TurnStarted)),
        1
    );
    assert_eq!(
        count(&kinds, |kind| matches!(
            kind,
            EventKind::Message {
                role: Role::User,
                ..
            }
        )),
        1
    );
    assert!(kinds.contains(&EventKind::Waiting), "{kinds:?}");
    assert!(
        kinds
            .iter()
            .position(|kind| matches!(kind, EventKind::Waiting))
            > kinds
                .iter()
                .position(|kind| matches!(kind, EventKind::TurnEnded { .. })),
        "the turn ends before the thread waits: {kinds:?}"
    );
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store.conversation(&id).unwrap().unwrap().to_string(),
        "ses_alpha"
    );
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
}

#[gpui_kit::test]
async fn acp_cancel_ends_the_turn_and_keeps_the_agent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_beta"], &[holding(), turn(&["again"], "end_turn")]);
    let id = started(&fixture, &agent, cx).await;
    let pid = agent.pid("pid", cx);
    send(&id, "one", cx).await;
    wait_for(
        &id,
        |kinds| count(kinds, |kind| matches!(kind, EventKind::TurnStarted)) == 1,
        cx,
    )
    .await;
    op(Op::Thread(ThreadOp::Cancel { id: id.clone() }), cx)
        .await
        .unwrap();
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Interrupted), cx).await;
    assert!(!kinds.contains(&EventKind::Stopped), "{kinds:?}");
    assert!(alive(pid), "the agent keeps running");
    send(&id, "two", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert!(agent_said(&kinds).contains("again"), "{kinds:?}");
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    assert!(gone(pid), "the agent ends with the thread");
}

#[gpui_kit::test]
async fn acp_stop_ends_the_agents_process_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_gamma"], &[spawning()]);
    let id = started(&fixture, &agent, cx).await;
    let pid = agent.pid("pid", cx);
    send(&id, "go", cx).await;
    let sleep = agent.pid("sleep.pid", cx);
    assert!(alive(pid) && alive(sleep), "both run before the stop");
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    let kinds = wait_for(&id, |kinds| kinds.contains(&EventKind::Idle), cx).await;
    assert!(kinds.contains(&EventKind::Stopped), "{kinds:?}");
    assert!(gone(pid), "the agent is gone");
    assert!(gone(sleep), "what the agent started is gone too");
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
}

#[gpui_kit::test]
async fn acp_screen_and_resize_are_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_delta"], &[]);
    let id = started(&fixture, &agent, cx).await;
    for refused in [
        ThreadOp::Screen {
            id: id.clone(),
            terminal: None,
            scrollback: false,
            lines: None,
        },
        ThreadOp::Resize {
            id: id.clone(),
            terminal: None,
            cols: 80,
            rows: 24,
        },
    ] {
        let Err(OpError::Invalid(message)) = op(Op::Thread(refused), cx).await else {
            panic!("a screen op on an ACP thread is refused")
        };
        assert!(message.contains("kg thread events"), "{message}");
    }
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_input_while_a_turn_is_open_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_epsilon"], &[holding(), turn(&["done"], "end_turn")]);
    let id = started(&fixture, &agent, cx).await;
    send(&id, "one", cx).await;
    wait_for(
        &id,
        |kinds| count(kinds, |kind| matches!(kind, EventKind::TurnStarted)) == 1,
        cx,
    )
    .await;
    let refused = op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            terminal: None,
            text: "two".into(),
        }),
        cx,
    )
    .await;
    assert!(
        matches!(refused, Err(OpError::Invalid(ref message)) if message.contains("cancel")),
        "{refused:?}"
    );
    op(Op::Thread(ThreadOp::Cancel { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Interrupted), cx).await;
    send(&id, "three", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert!(agent_said(&kinds).contains("done"), "{kinds:?}");
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_start_again_starts_a_new_session(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_first", "ses_second"], &[]);
    let id = started(&fixture, &agent, cx).await;
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store.conversation(&id).unwrap().unwrap().to_string(),
        "ses_first"
    );
    drop(store);
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Active, cx).await;
    let kinds = kinds_of(&id, cx).await;
    assert_eq!(
        count(&kinds, |kind| matches!(kind, EventKind::Started)),
        2,
        "{kinds:?}"
    );
    assert!(
        count(&kinds, |kind| matches!(kind, EventKind::Notice { .. })) >= 1,
        "the person is told the earlier conversation is not restored: {kinds:?}"
    );
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store.conversation(&id).unwrap().unwrap().to_string(),
        "ses_second"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_agent_that_quits_is_gone_and_a_prompt_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_eta"],
        &[json!({ "chunks": ["bye"], "stopReason": "end_turn", "quit": true })],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    let before = kinds_of(&id, cx).await.len();
    let refused = op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            terminal: None,
            text: "again".into(),
        }),
        cx,
    )
    .await;
    assert!(
        matches!(refused, Err(OpError::Invalid(ref message)) if message.contains("start the thread again")),
        "{refused:?}"
    );
    assert_eq!(
        kinds_of(&id, cx).await.len(),
        before,
        "a refused prompt writes nothing"
    );
}

#[gpui_kit::test]
async fn acp_an_answer_that_ends_with_the_agent_is_kept(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_last_word"],
        &[json!({
            "chunks": ["the last word"],
            "stopReason": "end_turn",
            "quit": true,
        })],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert_eq!(
        agent_said(&kinds),
        "the last word",
        "the agent's last frames are read before its output ends: {kinds:?}"
    );
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
}

#[gpui_kit::test]
async fn acp_events_are_the_transcript(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_zeta"], &[turn(&["one ", "two"], "end_turn")]);
    let id = started(&fixture, &agent, cx).await;
    send(&id, "read it", cx).await;
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    let transcript: Vec<Event> = events(&id, cx)
        .await
        .into_iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::Message {
                    role: Role::Agent,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        transcript
            .iter()
            .map(|event| match &event.kind {
                EventKind::Message { text, .. } => text.as_str(),
                _ => unreachable!(),
            })
            .collect::<String>(),
        "one two"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_a_session_id_kagan_cannot_store_is_said(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&[""], &[]);
    let OpResult::Thread(started) = op(fixture.run_acp(&agent.program()), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Active, cx).await;
    let kinds = kinds_of(&started.id, cx).await;
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            EventKind::Notice {
                severity: NoticeSeverity::Warning,
                text
            } if text.contains("cannot store")
        )),
        "the person is told why the session is not kept: {kinds:?}"
    );
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert!(store.conversation(&started.id).unwrap().is_none());
    op(Op::Thread(ThreadOp::Stop { id: started.id }), cx)
        .await
        .unwrap();
}

#[gpui_kit::test]
async fn acp_message_ids_are_kept_with_their_chunks(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_ids"], &[turn(&["a", "b"], "end_turn")]);
    std::fs::write(agent.path("modes"), "ids\n").unwrap();
    let id = started(&fixture, &agent, cx).await;
    send(&id, "one", cx).await;
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    let ids: Vec<_> = events(&id, cx)
        .await
        .into_iter()
        .filter_map(|event| match event.kind {
            EventKind::Message {
                role: Role::Agent,
                message_id,
                ..
            } => Some(message_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        vec![Some("msg_1".into())],
        "the chunk's own id, joined once"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_an_echo_of_the_prompt_is_not_drawn(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_echo"], &[turn(&["done"], "end_turn")]);
    std::fs::write(agent.path("modes"), "echo\n").unwrap();
    let id = started(&fixture, &agent, cx).await;
    send(&id, "one", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert_eq!(agent_said(&kinds), "done", "{kinds:?}");
    assert!(
        !kinds.iter().any(
            |kind| matches!(kind, EventKind::Message { text, .. } if text.contains("echo me"))
        ),
        "the agent's echo of the prompt is not a second message: {kinds:?}"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

fn allow_options() -> Value {
    json!([
        {"optionId": "reject-once", "name": "Deny", "kind": "reject_once"},
        {"optionId": "allow-once", "name": "Allow once", "kind": "allow_once"},
        {"optionId": "allow-always", "name": "Always allow", "kind": "allow_always"}
    ])
}

fn calling() -> Value {
    json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "call_1",
        "title": "Run the tests",
        "kind": "execute",
        "status": "in_progress",
        "rawInput": {"command": "cargo t"}
    })
}

/// Waits for the thread's first pending question.
async fn asked(id: &ThreadId, cx: &mut TestAppContext) -> PendingInteraction {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        if let Some(interaction) = super::pending(id, cx).await.into_iter().next() {
            return interaction;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the agent's question was never raised"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Waits for the file the fake agent writes with what it heard.
fn heard_in(path: &Path, cx: &mut TestAppContext) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if let Ok(text) = std::fs::read_to_string(path) {
            return text;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the agent never wrote {}",
            path.display()
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The two thought chunks of the recording arrive as one stream, in order.
fn one_thought(kinds: &[EventKind]) {
    let thoughts: Vec<_> = kinds
        .iter()
        .filter_map(|kind| match kind {
            EventKind::Thought { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thoughts.concat(), "look first before", "{kinds:?}");
}

/// A read call opens the row; its update completes it and repeats neither title nor input.
fn a_read_call_and_its_update(kinds: &[EventKind]) {
    let tools: Vec<_> = kinds
        .iter()
        .filter(|kind| matches!(kind, EventKind::Tool { .. }))
        .collect();
    assert_eq!(tools.len(), 2, "{kinds:?}");
    let EventKind::Tool {
        id: call,
        name,
        kind,
        title,
        status,
        input,
        locations,
        ..
    } = tools[0]
    else {
        unreachable!()
    };
    assert_eq!(call, "t1");
    assert_eq!(name.as_deref(), Some("read_file"));
    assert_eq!(kind, &Some(ToolKind::Read));
    assert_eq!(title.as_deref(), Some("Read file auth.spec.ts"));
    assert_eq!(status, &Some(ToolStatus::InProgress));
    assert_eq!(
        input,
        &Some(json!({"file_path": "auth.spec.ts"})),
        "the ACP raw input is kept as it arrived"
    );
    assert_eq!(
        locations,
        &vec![ToolLocation {
            path: "/work/auth.spec.ts".into(),
            line: Some(41),
        }]
    );
    let EventKind::Tool {
        id: call,
        title,
        status,
        output,
        diff,
        input,
        ..
    } = tools[1]
    else {
        unreachable!()
    };
    assert_eq!(call, "t1");
    assert_eq!(status, &Some(ToolStatus::Completed));
    assert_eq!(output.as_deref(), Some("line one\nline two"));
    assert_eq!(
        diff,
        &vec![ToolDiff {
            path: "/work/auth.spec.ts".into(),
            old: Some("old".into()),
            new: "new".into(),
        }]
    );
    assert!(title.is_none(), "an update need not repeat the title");
    assert!(input.is_none(), "an update need not repeat the input");
}

#[gpui_kit::test]
async fn acp_thought_and_tool_updates_fill_the_log(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_tool"],
        &[transcript(
            &json!([
                {"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "look first"}},
                {"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": " before"}},
                {
                    "sessionUpdate": "tool_call",
                    "toolCallId": "t1",
                    "name": "read_file",
                    "title": "Read file auth.spec.ts",
                    "kind": "read",
                    "status": "in_progress",
                    "rawInput": {"file_path": "auth.spec.ts"},
                    "locations": [{"path": "/work/auth.spec.ts", "line": 41}]
                },
                {
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": "t1",
                    "status": "completed",
                    "content": [
                        {"type": "content", "content": {"type": "text", "text": "line one\nline two"}},
                        {"type": "diff", "path": "/work/auth.spec.ts", "oldText": "old", "newText": "new"}
                    ]
                }
            ]),
            "end_turn",
        )],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    one_thought(&kinds);
    a_read_call_and_its_update(&kinds);
    let ats: Vec<_> = events(&id, cx).await.iter().map(|event| event.at).collect();
    assert!(
        ats.windows(2).all(|pair| pair[0] <= pair[1]),
        "the log keeps the order things happened: {ats:?}"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_mcp_and_skill_calls_are_plain_tool_events(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_mcp"],
        &[transcript(
            &json!([
                {
                    "sessionUpdate": "tool_call",
                    "toolCallId": "m1",
                    "name": "mcp__github__create_issue",
                    "title": "Create issue",
                    "kind": "other",
                    "status": "completed",
                    "rawInput": {"repo": "acme/webapp"}
                },
                {
                    "sessionUpdate": "tool_call",
                    "toolCallId": "s1",
                    "name": "skill_load",
                    "title": "load skill rust-review",
                    "kind": "other",
                    "status": "completed",
                    "rawInput": {"name": "rust-review"}
                }
            ]),
            "end_turn",
        )],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    let calls: Vec<_> = kinds
        .iter()
        .filter_map(|kind| match kind {
            EventKind::Tool { name, input, .. } => Some((name.clone(), input.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        vec![
            (
                Some("mcp__github__create_issue".into()),
                Some(json!({"repo": "acme/webapp"}))
            ),
            (
                Some("skill_load".into()),
                Some(json!({"name": "rust-review"}))
            ),
        ],
        "{kinds:?}"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_plan_and_usage_are_events(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_plan"],
        &[transcript(
            &json!([
                {
                    "sessionUpdate": "plan",
                    "entries": [
                        {"content": "Read the test", "priority": "high", "status": "completed"},
                        {"content": "Find the race", "priority": "medium", "status": "in_progress"},
                        {"content": "Fix it", "priority": "low", "status": "pending"}
                    ]
                },
                {"sessionUpdate": "usage_update", "used": 12000, "size": 200_000}
            ]),
            "end_turn",
        )],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let kinds = wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    let plan = kinds.iter().find_map(|kind| match kind {
        EventKind::Plan { entries } => Some(entries.clone()),
        _ => None,
    });
    assert_eq!(
        plan,
        Some(vec![
            PlanEntry {
                text: "Read the test".into(),
                status: PlanStatus::Completed,
            },
            PlanEntry {
                text: "Find the race".into(),
                status: PlanStatus::InProgress,
            },
            PlanEntry {
                text: "Fix it".into(),
                status: PlanStatus::Pending,
            },
        ]),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&EventKind::Usage {
            used: 12000,
            size: 200_000,
        }),
        "{kinds:?}"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_approval_allow_once_answers_the_agent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_allow"],
        &[asking(&calling(), &allow_options(), "end_turn")],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let interaction = asked(&id, cx).await;
    assert!(
        interaction.title.contains("wants to run a command"),
        "{}",
        interaction.title
    );
    assert_eq!(
        interaction.options,
        vec![
            InteractionOption {
                id: "reject-once".into(),
                label: "Deny".into(),
                role: Some(OptionRole::Destructive),
            },
            InteractionOption {
                id: "allow-once".into(),
                label: "Allow once".into(),
                role: Some(OptionRole::Primary),
            },
            InteractionOption {
                id: "allow-always".into(),
                label: "Always allow".into(),
                role: None,
            },
        ],
        "the agent's labels and order stay as they are"
    );
    let kinds = kinds_of(&id, cx).await;
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            EventKind::Tool {
                approval: Some(approval),
                status: Some(ToolStatus::Pending),
                ..
            } if approval == &interaction.id
        )),
        "the tool row links to its question: {kinds:?}"
    );
    cx.update(|cx| Workspace::answer(&id, interaction.id, "allow-once".into(), cx))
        .await
        .unwrap();
    let answer = heard_in(&agent.path("answer"), cx);
    assert_eq!(answer.trim(), "allow-once");
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert!(
        super::pending(&id, cx).await.is_empty(),
        "the question is gone once answered"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_approval_deny_answers_the_agent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_deny"],
        &[asking(&calling(), &allow_options(), "end_turn")],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let interaction = asked(&id, cx).await;
    cx.update(|cx| Workspace::answer(&id, interaction.id, "reject-once".into(), cx))
        .await
        .unwrap();
    let answer = heard_in(&agent.path("answer"), cx);
    assert_eq!(answer.trim(), "reject-once");
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_cancel_withdraws_the_approval(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_withdraw"],
        &[asking(&calling(), &allow_options(), "end_turn")],
    );
    let id = started(&fixture, &agent, cx).await;
    let pid = agent.pid("pid", cx);
    send(&id, "go", cx).await;
    let interaction = asked(&id, cx).await;
    assert!(
        !interaction.options.is_empty(),
        "the question shows the agent's options before it is withdrawn"
    );
    op(Op::Thread(ThreadOp::Cancel { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Interrupted), cx).await;
    assert!(
        super::pending(&id, cx).await.is_empty(),
        "the card goes the moment the person cancels"
    );
    assert!(
        !agent.path("answer").exists(),
        "a withdrawn question never reaches the agent"
    );
    assert!(alive(pid), "the agent keeps running");
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_cancel_withdraws_the_agent_approval_and_leaves_the_threads_question(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_cancel_ask"],
        &[asking(&calling(), &allow_options(), "end_turn")],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let approval = asked(&id, cx).await;
    let option = |id: &str| InteractionOption {
        id: id.into(),
        label: id.into(),
        role: None,
    };
    let OpResult::Text(asked_by_agent) = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Which way?".into(),
            options: vec![option("left"), option("right")],
            fields: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("ask")
    };
    assert_eq!(
        super::pending(&id, cx).await.len(),
        2,
        "both questions wait"
    );
    op(Op::Thread(ThreadOp::Cancel { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Interrupted), cx).await;
    let pending = super::pending(&id, cx).await;
    assert_eq!(
        pending.len(),
        1,
        "cancel took the agent's card only: {pending:?}"
    );
    assert_eq!(pending[0].id.to_string(), asked_by_agent);
    assert_ne!(pending[0].id, approval.id);
    // The question the agent asked through kagan still has its waiter.
    let question = PendingInteractionId(asked_by_agent);
    cx.update(|cx| Workspace::answer(&id, question, "left".into(), cx))
        .await
        .unwrap();
    assert!(super::pending(&id, cx).await.is_empty());
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

/// An agent that exits while its request is open leaves no card and no waiter behind.
#[gpui_kit::test]
async fn acp_an_agent_that_hangs_up_with_a_question_open_leaves_nothing_waiting(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_hangup"],
        &[json!({
            "updates": [calling()],
            "permission": {
                "toolCall": {"toolCallId": "call_1", "title": "Run the tests", "kind": "execute"},
                "options": allow_options(),
                "hangup": true,
            },
            "stopReason": "end_turn",
        })],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let interaction = asked(&id, cx).await;
    assert!(
        interaction.title.contains("wants to run a command"),
        "{}",
        interaction.title
    );
    std::fs::write(agent.path("hangup"), "").unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    assert!(
        super::pending(&id, cx).await.is_empty(),
        "the agent's exit takes its question with it"
    );
    assert!(
        !super::busy_of(&fixture, &id, cx),
        "and the thread is not left waiting on an answer"
    );
}

/// Stop while the agent waits for an answer ends the wait with the agent.
#[gpui_kit::test]
async fn acp_stop_with_a_question_open_leaves_nothing_waiting(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_stop_ask"],
        &[asking(&calling(), &allow_options(), "end_turn")],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let interaction = asked(&id, cx).await;
    assert!(!interaction.options.is_empty());
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    assert!(
        super::pending(&id, cx).await.is_empty(),
        "the stopped thread has no question"
    );
    assert!(!agent.path("answer").exists(), "nobody answered it");
}

/// A question the agent left open when its turn ended is about a turn that is over.
#[gpui_kit::test]
async fn acp_a_question_left_open_when_the_turn_ends_goes_with_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_abandon"],
        &[json!({
            "updates": [calling()],
            "permission": {
                "toolCall": {"toolCallId": "call_1", "title": "Run the tests", "kind": "execute"},
                "options": allow_options(),
                "abandon": true,
            },
            "stopReason": "end_turn",
        })],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert!(
        super::pending(&id, cx).await.is_empty(),
        "a question the agent left behind does not outlive its turn"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_a_permission_request_with_no_options_is_cancelled(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_noopt"],
        &[asking(&calling(), &json!([]), "end_turn")],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    assert!(
        super::pending(&id, cx).await.is_empty(),
        "a request with nothing to choose is never raised"
    );
    let refused = heard_in(&agent.path("refused"), cx);
    assert!(refused.contains("cancelled"), "{refused}");
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

#[gpui_kit::test]
async fn acp_agent_that_fails_before_its_session_leaves_an_error(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_none"], &[]);
    std::fs::write(agent.path("modes"), "fail_init\n").unwrap();
    let OpResult::Thread(started) = op(fixture.run_acp(&agent.program()), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Error, cx).await;
    let kinds = kinds_of(&started.id, cx).await;
    let error = failed(&kinds).expect("the person is told why");
    assert!(error.contains("cannot start"), "{error}");
    assert!(
        kinds.contains(&EventKind::Exited {
            code: Some(3),
            signal: None
        }),
        "{kinds:?}"
    );
    assert!(!kinds.contains(&EventKind::Started), "{kinds:?}");
}

#[gpui_kit::test]
async fn acp_agent_that_crashes_says_why_it_did(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_crash"],
        &[json!({
            "chunks": [],
            "stopReason": "end_turn",
            "stderr": ["no model is configured"],
            "exit": 3,
        })],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let kinds = wait_for(&id, |kinds| failed(kinds).is_some(), cx).await;
    let error = failed(&kinds).unwrap();
    assert!(error.contains("code 3"), "{error}");
    assert!(error.contains("no model is configured"), "{error}");
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
}

#[gpui_kit::test]
async fn acp_agent_killed_mid_turn_is_a_failure_not_a_finish(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(
        &["ses_killed"],
        &[json!({ "chunks": [], "hold": true, "signal": true })],
    );
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    let kinds = wait_for(&id, |kinds| failed(kinds).is_some(), cx).await;
    let error = failed(&kinds).unwrap();
    assert!(error.contains("signal 9"), "{error}");
    assert!(!ended(&kinds, TurnOutcome::Completed), "{kinds:?}");
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
}

#[gpui_kit::test]
async fn acp_stop_while_starting_ends_the_agent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_hang"], &[]);
    std::fs::write(agent.path("modes"), "hang_init\n").unwrap();
    let OpResult::Thread(started) = op(fixture.run_acp(&agent.program()), cx).await.unwrap() else {
        panic!("thread")
    };
    let pid = agent.pid("pid", cx);
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Starting);
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    assert!(gone(pid), "the agent ends with the stop");
    let kinds = kinds_of(&started.id, cx).await;
    assert!(!kinds.contains(&EventKind::Started), "{kinds:?}");
}

#[gpui_kit::test]
async fn acp_stop_during_an_open_turn_asks_for_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_stop"], &[holding()]);
    let id = started(&fixture, &agent, cx).await;
    let pid = agent.pid("pid", cx);
    send(&id, "one", cx).await;
    wait_for(
        &id,
        |kinds| count(kinds, |kind| matches!(kind, EventKind::TurnStarted)) == 1,
        cx,
    )
    .await;
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    let kinds = wait_for(&id, |kinds| kinds.contains(&EventKind::Idle), cx).await;
    assert!(kinds.contains(&EventKind::Stopped), "{kinds:?}");
    assert!(ended(&kinds, TurnOutcome::Interrupted), "{kinds:?}");
    assert!(
        !agent.path("cancelled").exists(),
        "Stop ends the agent; it does not ask it to cancel first"
    );
    assert!(gone(pid), "the agent ends with the thread");
    let idle = kinds
        .iter()
        .position(|kind| matches!(kind, EventKind::Idle))
        .unwrap();
    assert_eq!(
        idle,
        kinds.len() - 1,
        "nothing is written after idle: {kinds:?}"
    );
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
}

#[gpui_kit::test]
async fn acp_a_hung_handshake_ends_with_a_reason(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_hung"], &[]);
    std::fs::write(agent.path("modes"), "hang_init\n").unwrap();
    let OpResult::Thread(started) = op(fixture.run_acp(&agent.program()), cx).await.unwrap() else {
        panic!("thread")
    };
    let pid = agent.pid("pid", cx);
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Starting);
    // The agent never answers initialize, so the one handshake deadline passes.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(40));
    wait_for_status(&started.id, ThreadStatus::Error, cx).await;
    let error = failed(&kinds_of(&started.id, cx).await).expect("the person is told why");
    assert!(error.contains("did not answer"), "{error}");
    assert!(gone(pid), "the agent that never answered is gone");
}

#[gpui_kit::test]
async fn acp_a_prompt_that_cannot_be_sent_writes_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_closed"], &[holding()]);
    let id = started(&fixture, &agent, cx).await;
    // The run still holds a connection whose commands are closed, as a send that lost its race
    // with the agent's end would find it.
    cx.update(|cx| {
        fixture.workspace.update(cx, |workspace, _| {
            workspace.run_mut(&id).acp.as_ref().unwrap().close();
        });
    });
    let before = kinds_of(&id, cx).await;
    let refused = op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            terminal: None,
            text: "hi".into(),
        }),
        cx,
    )
    .await;
    assert!(
        matches!(refused, Err(OpError::Invalid(ref message)) if message.contains("not running")),
        "{refused:?}"
    );
    assert_eq!(
        kinds_of(&id, cx).await.len(),
        before.len(),
        "a prompt that was not sent leaves no message and no turn"
    );
    op(Op::Thread(ThreadOp::Stop { id }), cx).await.unwrap();
}

async fn send(id: &ThreadId, text: &str, cx: &mut TestAppContext) {
    op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            terminal: None,
            text: text.into(),
        }),
        cx,
    )
    .await
    .unwrap();
}

fn reply(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn update(session: &str, kind: &str, text: &str, message_id: Option<&str>) -> Value {
    let mut chunk = json!({
        "sessionUpdate": kind,
        "content": {"type": "text", "text": text},
    });
    if let Some(message_id) = message_id {
        chunk["messageId"] = json!(message_id);
    }
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {"sessionId": session, "update": chunk},
    })
}

fn permission_request(session: &str, permission: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": PERMISSION_ID,
        "method": "session/request_permission",
        "params": {
            "sessionId": session,
            "toolCall": permission["toolCall"].clone(),
            "options": permission["options"].clone(),
        },
    })
}

/// The id kagan sees on every permission request this agent makes.
const PERMISSION_ID: i64 = 7;

fn sleep_child(dir: &Path) {
    let child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    std::fs::write(dir.join("sleep.pid"), child.id().to_string()).unwrap();
    // The agent's own process tree ends the child; the test never waits for it.
    std::mem::forget(child);
}

/// The agent, when this test binary is run again with `KG_FAKE_ACP_DIR` set: it answers the
/// agent protocol on stdin and stdout from the recording, and returns at once otherwise.
#[test]
fn fake_agent() {
    let Ok(dir) = std::env::var("KG_FAKE_ACP_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let mut turns = std::fs::read_to_string(dir.join("turns.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>()
        .into_iter();
    std::fs::write(dir.join("pid"), std::process::id().to_string()).unwrap();
    let modes = std::fs::read_to_string(dir.join("modes")).unwrap_or_default();
    let mode = |name: &str| modes.lines().any(|line| line == name);
    let fail_init = mode("fail_init");
    let hang_init = mode("hang_init");
    let mut open: Option<(Value, Value)> = None;
    let mut session = String::new();
    let mut out = std::io::stdout();
    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    let mut lines = stdin.lines().map_while(Result::ok);
    while let Some(line) = lines.next() {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = message["id"].clone();
        let method = message["method"].as_str().unwrap_or_default();
        if method.is_empty() {
            // A response to a request this agent made: what kagan answered the permission with.
            std::fs::write(dir.join("permission"), &line).unwrap();
            continue;
        }
        match method {
            "initialize" => {
                if fail_init {
                    eprintln!("the agent cannot start");
                    std::process::exit(3);
                }
                if hang_init {
                    continue;
                }
                write(
                    &mut out,
                    &reply(&id, &json!({"protocolVersion": 1, "agentCapabilities": {}})),
                );
            }
            "session/new" => {
                session = take_session(&dir);
                write(&mut out, &reply(&id, &json!({ "sessionId": session })));
            }
            "session/prompt" => {
                let record = turns.next().unwrap_or_else(|| turn(&[], "end_turn"));
                if record["sleep"].as_bool() == Some(true) {
                    sleep_child(&dir);
                }
                match record["updates"].as_array() {
                    Some(updates) => {
                        for update in updates {
                            write(&mut out, &notice(&session, update));
                        }
                    }
                    None => turn_chunks(&mut out, &session, &record, &modes),
                }
                let mut answered = false;
                if let Some(permission) = record.get("permission") {
                    answered =
                        permission_step(&dir, &mut lines, &mut out, &session, permission, &id);
                }
                if record["hold"].as_bool() == Some(true) && !answered {
                    open = Some((id, record.clone()));
                } else if !answered {
                    let stop = record["stopReason"].as_str().unwrap_or("end_turn");
                    write(&mut out, &reply(&id, &json!({ "stopReason": stop })));
                }
                if record["quit"].as_bool() == Some(true) || ends(&record) {
                    return;
                }
            }
            "session/cancel" => {
                std::fs::write(dir.join("cancelled"), "").unwrap();
                if let Some((id, _record)) = open.take() {
                    write(&mut out, &reply(&id, &json!({ "stopReason": "cancelled" })));
                }
            }
            method if !id.is_null() => {
                write(
                    &mut out,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32601, "message": format!("no {method}")},
                    }),
                );
            }
            _ => {}
        }
    }
}

/// The ways a recording ends the agent after a prompt: its stderr, its exit code, or a signal.
/// Returns whether the agent stops listening.
fn ends(record: &Value) -> bool {
    for line in record["stderr"].as_array().cloned().unwrap_or_default() {
        eprintln!("{}", line.as_str().unwrap_or_default());
    }
    if let Some(code) = record["exit"].as_i64() {
        std::process::exit(i32::try_from(code).unwrap_or(1));
    }
    if record["signal"].as_bool() == Some(true) {
        let _ = std::process::Command::new("kill")
            .args(["-9", &std::process::id().to_string()])
            .status();
    }
    false
}

/// Writes one recorded turn's chunks, with the prompt echo and the message id the modes ask for.
fn turn_chunks(out: &mut std::io::Stdout, session: &str, record: &Value, modes: &str) {
    let mode = |name: &str| modes.lines().any(|line| line == name);
    if mode("echo") {
        write(out, &update(session, "user_message_chunk", "echo me", None));
    }
    let message_id = mode("ids").then_some("msg_1");
    for chunk in record["chunks"].as_array().cloned().unwrap_or_default() {
        let text = chunk.as_str().unwrap_or_default();
        write(
            out,
            &update(session, "agent_message_chunk", text, message_id),
        );
    }
}

/// One raw ACP update on the session, as the recording holds it.
fn notice(session: &str, update: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {"sessionId": session, "update": update},
    })
}

/// Asks the person and waits for kagan's answer. Returns whether the prompt is already answered,
/// which a cancel does. A selected option is written to `answer`; a cancelled one to `refused`.
fn permission_step(
    dir: &Path,
    lines: &mut impl Iterator<Item = String>,
    out: &mut std::io::Stdout,
    session: &str,
    permission: &Value,
    prompt: &Value,
) -> bool {
    write(out, &permission_request(session, permission));
    // An agent that hangs up with its request open never answers it: it waits for the test to say
    // the card is up, then goes, and the card must not outlive it.
    if permission["hangup"].as_bool() == Some(true) {
        let signal = dir.join("hangup");
        for _ in 0..600 {
            if signal.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::process::exit(0);
    }
    // An agent that ends its turn without waiting for the answer leaves the request open.
    if permission["abandon"].as_bool() == Some(true) {
        write(out, &reply(prompt, &json!({ "stopReason": "end_turn" })));
        return true;
    }
    let mut answered = false;
    for line in lines.by_ref() {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message["method"] == "session/cancel" {
            std::fs::write(dir.join("cancelled"), "").unwrap();
            write(out, &reply(prompt, &json!({ "stopReason": "cancelled" })));
            answered = true;
            continue;
        }
        if message["id"] != json!(PERMISSION_ID) {
            continue;
        }
        if let Some(option) = message["result"]["outcome"]["optionId"].as_str() {
            std::fs::write(dir.join("answer"), option).unwrap();
        } else {
            std::fs::write(dir.join("refused"), message.to_string()).unwrap();
        }
        break;
    }
    answered
}

/// The next session id the recording holds, taken off the file so the next agent kagan starts
/// answers with the one after it.
fn take_session(dir: &Path) -> String {
    let text = std::fs::read_to_string(dir.join("sessions")).unwrap_or_default();
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default().to_owned();
    let rest: Vec<&str> = lines.collect();
    std::fs::write(dir.join("sessions"), rest.join("\n") + "\n").unwrap();
    first
}

fn write(out: &mut std::io::Stdout, message: &Value) {
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}
