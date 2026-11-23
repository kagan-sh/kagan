//! One agent that answers over its own pipes, and the thread that talks to it. Slice 1 speaks
//! text only: a prompt in, message chunks and a stop reason out.

use std::io::{BufRead as _, Write as _};
use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use kg_domain::*;
use serde_json::{Value, json};

use super::{Fixture, alive, events, gone, kinds_of, op, thread, wait_for_status};

/// One recorded turn: the chunks the agent says, and why it stopped. `hold` keeps the prompt
/// open until a cancel; `sleep` spawns a child that must die with the agent.
fn turn(chunks: &[&str], stop: &str) -> Value {
    json!({ "chunks": chunks, "stopReason": stop })
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

#[gpui_kit::test]
async fn acp_a_permission_request_is_answered_cancelled(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = Agent::new(&["ses_ask"], &[turn(&["done"], "end_turn")]);
    std::fs::write(agent.path("modes"), "permission\n").unwrap();
    let id = started(&fixture, &agent, cx).await;
    send(&id, "go", cx).await;
    wait_for(&id, |kinds| ended(kinds, TurnOutcome::Completed), cx).await;
    let answered = agent.path("permission");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let answer = loop {
        cx.run_until_parked();
        if let Ok(answer) = std::fs::read_to_string(&answered) {
            break answer;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the agent never heard kagan's answer"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(answer.contains("cancelled"), "{answer}");
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

fn permission_request(session: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "session/request_permission",
        "params": {
            "sessionId": session,
            "toolCall": {"toolCallId": "call_1", "title": "read a file", "kind": "read"},
            "options": [{"optionId": "allow", "name": "Allow once", "kind": "allow_once"}],
        },
    })
}

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
    for line in stdin.lines().map_while(Result::ok) {
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
                turn_chunks(&mut out, &session, &record, &modes);
                if record["hold"].as_bool() == Some(true) {
                    open = Some((id, record.clone()));
                } else {
                    let stop = record["stopReason"].as_str().unwrap_or("end_turn");
                    write(&mut out, &reply(&id, &json!({ "stopReason": stop })));
                }
                if record["quit"].as_bool() == Some(true) {
                    return;
                }
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

/// Writes one recorded turn's chunks, with the prompt echo and the message id the modes ask for.
fn turn_chunks(out: &mut std::io::Stdout, session: &str, record: &Value, modes: &str) {
    let mode = |name: &str| modes.lines().any(|line| line == name);
    if mode("echo") {
        write(out, &update(session, "user_message_chunk", "echo me", None));
    }
    if mode("permission") {
        write(out, &permission_request(session));
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
