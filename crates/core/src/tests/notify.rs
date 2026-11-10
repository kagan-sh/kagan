use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, Reap, ask_options, begin_wait, busy_of, events, finish_wait, op, outcome, script,
    sent_text, started, wait_for_status, wait_for_waiting, wait_until_busy,
};

const SLEEP: &str = r#"/bin/sh -c "sleep 30""#;

fn finishing(dir: &std::path::Path, name: &str) -> String {
    script(
        dir,
        name,
        "printf '\\033]0;\\342\\240\\213 working\\007'\nsleep 1\nprintf '\\033]0;ready\\007'\nsleep 30",
    )
}

async fn child(
    fixture: &Fixture,
    program: &str,
    parent: &ThreadId,
    quiet: bool,
    cx: &mut TestAppContext,
) -> (ThreadId, Reap) {
    titled_child(fixture, program, parent, None, quiet, cx).await
}

async fn titled_child(
    fixture: &Fixture,
    program: &str,
    parent: &ThreadId,
    title: Option<&str>,
    quiet: bool,
    cx: &mut TestAppContext,
) -> (ThreadId, Reap) {
    let request = Op::Thread(ThreadOp::Run(Run {
        project_id: fixture.project.clone(),
        program: program.into(),
        title: title.map(str::to_owned),
        environment: EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: Some(parent.clone()),
        quiet,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }));
    let OpResult::Thread(started) = op(request, cx).await.unwrap() else {
        panic!("thread")
    };
    let pid = super::pid_of(&started.id, cx).expect("the host started the program");
    (started.id, Reap(pid))
}

async fn sent_to(parent: &ThreadId, cx: &mut TestAppContext) -> Vec<String> {
    for _ in 0..400 {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let recorded = events(parent, cx).await;
        let sent: Vec<String> = sent_text(&recorded)
            .into_iter()
            .map(str::to_owned)
            .collect();
        if !sent.is_empty() {
            return sent;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Vec::new()
}

#[gpui_kit::test]
async fn a_finished_turn_is_typed_into_an_idle_parent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (parent, _parent_run) = started(&fixture, SLEEP, cx).await;
    let program = finishing(fixture.dir.path(), "finish");
    let (child_id, _child_run) = child(&fixture, &program, &parent, false, cx).await;
    wait_for_waiting(&child_id, 1, cx).await;
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&child_id.to_string()), "{}", sent[0]);
    assert!(sent[0].contains("finished its turn"), "{}", sent[0]);
    assert!(
        sent[0].contains(&format!("kg thread screen {child_id}")),
        "{}",
        sent[0]
    );
}

#[gpui_kit::test]
async fn a_busy_parent_gets_one_message_for_two_children(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let flag = fixture.dir.path().join("parent-done");
    let parent_program = script(
        fixture.dir.path(),
        "parent",
        &format!(
            "printf '\\033]0;\\342\\240\\213 working\\007'\nwhile [ ! -f '{}' ]; do sleep 0.1; done\nprintf '\\033]0;ready\\007'\nsleep 30",
            flag.display()
        ),
    );
    let (parent, _parent_run) = started(&fixture, &parent_program, cx).await;
    wait_until_busy(&fixture, &parent, cx);
    let first = finishing(fixture.dir.path(), "first");
    let second = finishing(fixture.dir.path(), "second");
    let (first_id, _first_run) = child(&fixture, &first, &parent, false, cx).await;
    let (second_id, _second_run) = child(&fixture, &second, &parent, false, cx).await;
    wait_for_waiting(&first_id, 1, cx).await;
    wait_for_waiting(&second_id, 1, cx).await;
    let before = events(&parent, cx).await;
    assert!(sent_text(&before).is_empty(), "{:?}", sent_text(&before));
    std::fs::write(&flag, "").unwrap();
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&first_id.to_string()), "{}", sent[0]);
    assert!(sent[0].contains(&second_id.to_string()), "{}", sent[0]);
}

#[gpui_kit::test]
async fn a_quiet_child_sends_nothing_and_its_sibling_still_does(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (parent, _parent_run) = started(&fixture, SLEEP, cx).await;
    let program = finishing(fixture.dir.path(), "finish");
    let (quiet_id, _quiet_run) = child(&fixture, &program, &parent, true, cx).await;
    let (loud_id, _loud_run) = child(&fixture, &program, &parent, false, cx).await;
    wait_for_waiting(&quiet_id, 1, cx).await;
    wait_for_waiting(&loud_id, 1, cx).await;
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&loud_id.to_string()), "{}", sent[0]);
    assert!(!sent[0].contains(&quiet_id.to_string()), "{}", sent[0]);
}

#[gpui_kit::test]
async fn a_stop_sends_nothing_and_a_sibling_finish_still_does(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (parent, _parent_run) = started(&fixture, SLEEP, cx).await;
    let spinning = script(
        fixture.dir.path(),
        "spin",
        "printf '\\033]0;\\342\\240\\213 working\\007'\nsleep 30",
    );
    let (stopped_id, _stopped_run) = child(&fixture, &spinning, &parent, false, cx).await;
    wait_until_busy(&fixture, &stopped_id, cx);
    op(
        Op::Thread(ThreadOp::Stop {
            id: stopped_id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&stopped_id, ThreadStatus::Idle, cx).await;
    assert!(!busy_of(&fixture, &stopped_id, cx));
    let program = finishing(fixture.dir.path(), "finish");
    let (done_id, _done_run) = child(&fixture, &program, &parent, false, cx).await;
    wait_for_waiting(&done_id, 1, cx).await;
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&done_id.to_string()), "{}", sent[0]);
    assert!(!sent[0].contains(&stopped_id.to_string()), "{}", sent[0]);
}

#[gpui_kit::test]
async fn a_parent_that_has_ended_gets_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(ended) = op(fixture.run(r#"/bin/sh -c "exit 0""#), cx).await.unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&ended.id, ThreadStatus::Idle, cx).await;
    let (running, _running_run) = started(&fixture, SLEEP, cx).await;
    let program = finishing(fixture.dir.path(), "finish");
    let (orphan_id, _orphan_run) = child(&fixture, &program, &ended.id, false, cx).await;
    let (kept_id, _kept_run) = child(&fixture, &program, &running, false, cx).await;
    wait_for_waiting(&orphan_id, 1, cx).await;
    wait_for_waiting(&kept_id, 1, cx).await;
    let sent = sent_to(&running, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&kept_id.to_string()), "{}", sent[0]);
    let ended_events = events(&ended.id, cx).await;
    assert!(sent_text(&ended_events).is_empty());
}

#[gpui_kit::test]
async fn a_question_from_a_child_is_typed_as_needs_you(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (parent, _parent_run) = started(&fixture, SLEEP, cx).await;
    let (child_id, _child_run) = child(&fixture, SLEEP, &parent, false, cx).await;
    let _asking = cx.update(|cx| {
        Workspace::ask_for_person(
            Some(&child_id),
            None,
            "Keep going?".into(),
            ask_options(),
            cx,
        )
    });
    cx.run_until_parked();
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("needs you"), "{}", sent[0]);
    assert!(sent[0].contains(&child_id.to_string()), "{}", sent[0]);
}

fn parent_until(dir: &std::path::Path, flag: &std::path::Path) -> String {
    script(
        dir,
        "parent",
        &format!(
            "printf '\\033]0;\\342\\240\\213 working\\007'\nwhile [ ! -f '{}' ]; do sleep 0.1; done\nprintf '\\033]0;ready\\007'\nsleep 30",
            flag.display()
        ),
    )
}

async fn typed(id: &ThreadId, text: &str, cx: &mut TestAppContext) {
    op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            text: text.into(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
}

fn wait_until_idle(fixture: &Fixture, id: &ThreadId, cx: &mut TestAppContext) {
    for _ in 0..400 {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        if !busy_of(fixture, id, cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the parent did not end its turn");
}

#[gpui_kit::test]
async fn a_line_waits_while_the_person_has_an_unsent_draft(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let flag = fixture.dir.path().join("parent-done");
    let (parent, _parent_run) =
        started(&fixture, &parent_until(fixture.dir.path(), &flag), cx).await;
    wait_until_busy(&fixture, &parent, cx);
    let program = finishing(fixture.dir.path(), "finish");
    let (child_id, _child_run) = child(&fixture, &program, &parent, false, cx).await;
    wait_for_waiting(&child_id, 1, cx).await;
    typed(&parent, "half", cx).await;
    std::fs::write(&flag, "").unwrap();
    wait_until_idle(&fixture, &parent, cx);
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert!(sent_text(&events(&parent, cx).await).is_empty());
    cx.executor().advance_clock(Duration::from_secs(30));
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&child_id.to_string()), "{}", sent[0]);
}

#[gpui_kit::test]
async fn enter_ends_the_wait_for_a_draft(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let flag = fixture.dir.path().join("parent-done");
    let (parent, _parent_run) =
        started(&fixture, &parent_until(fixture.dir.path(), &flag), cx).await;
    wait_until_busy(&fixture, &parent, cx);
    let program = finishing(fixture.dir.path(), "finish");
    let (child_id, _child_run) = child(&fixture, &program, &parent, false, cx).await;
    wait_for_waiting(&child_id, 1, cx).await;
    typed(&parent, "half", cx).await;
    cx.executor().advance_clock(Duration::from_secs(3));
    typed(&parent, "\r", cx).await;
    std::fs::write(&flag, "").unwrap();
    wait_until_idle(&fixture, &parent, cx);
    cx.executor().advance_clock(Duration::from_secs(2));
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains(&child_id.to_string()), "{}", sent[0]);
}

#[gpui_kit::test]
async fn a_wait_on_the_child_drops_the_line_it_already_told_the_parent(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let flag = fixture.dir.path().join("parent-done");
    let (parent, _parent_run) =
        started(&fixture, &parent_until(fixture.dir.path(), &flag), cx).await;
    wait_until_busy(&fixture, &parent, cx);
    // The child must outlive the test, or its exit sends a second, unwaited line.
    let program = script(
        fixture.dir.path(),
        "linger",
        "printf '\\033]0;\\342\\240\\213 working\\007'\nsleep 1\nprintf '\\033]0;ready\\007'\nsleep 600",
    );
    let (child_id, _child_run) = child(&fixture, &program, &parent, false, cx).await;
    wait_until_busy(&fixture, &child_id, cx);
    let waited = begin_wait(&child_id, 20_000, cx);
    finish_wait(&waited, cx);
    assert_eq!(outcome(&waited.await.unwrap()), WaitOutcome::Waiting);
    std::fs::write(&flag, "").unwrap();
    wait_until_idle(&fixture, &parent, cx);
    let sent = sent_to(&parent, cx).await;
    assert!(sent.is_empty(), "{sent:?}");
}

#[gpui_kit::test]
async fn a_wait_for_the_child_exit_drops_the_line_it_already_told_the_parent(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let flag = fixture.dir.path().join("parent-done");
    let (parent, _parent_run) =
        started(&fixture, &parent_until(fixture.dir.path(), &flag), cx).await;
    wait_until_busy(&fixture, &parent, cx);
    let (child_id, _child_run) =
        child(&fixture, r#"/bin/sh -c "exit 0""#, &parent, false, cx).await;
    wait_for_status(&child_id, ThreadStatus::Idle, cx).await;
    op(
        Op::Thread(ThreadOp::WaitEvent {
            id: child_id.clone(),
            kind: "exited".into(),
            after_seq: None,
            timeout_ms: 1_000,
        }),
        cx,
    )
    .await
    .unwrap();
    std::fs::write(&flag, "").unwrap();
    wait_until_idle(&fixture, &parent, cx);
    let sent = sent_to(&parent, cx).await;
    assert!(sent.is_empty(), "{sent:?}");
}

#[gpui_kit::test]
async fn a_child_title_stays_out_of_the_parent_line(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (parent, _parent_run) = started(&fixture, SLEEP, cx).await;
    let program = finishing(fixture.dir.path(), "finish");
    let (child_id, _child_run) = titled_child(
        &fixture,
        &program,
        &parent,
        Some("x\" ; $(echo hi) ; \""),
        false,
        cx,
    )
    .await;
    wait_for_waiting(&child_id, 1, cx).await;
    let sent = sent_to(&parent, cx).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(!sent[0].contains("echo hi"), "{}", sent[0]);
    assert!(sent[0].contains(&child_id.to_string()), "{}", sent[0]);
}

#[gpui_kit::test]
async fn a_child_end_under_a_parent_with_no_run_adds_no_run(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (parent, _parent_run) = started(&fixture, SLEEP, cx).await;
    let program = finishing(fixture.dir.path(), "finish");
    let (child_id, _child_run) = child(&fixture, &program, &parent, false, cx).await;
    fixture.workspace.update(cx, |workspace, _| {
        workspace.runs.remove(&parent);
    });
    wait_for_waiting(&child_id, 1, cx).await;
    assert!(
        !fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.runs.contains_key(&parent))
    );
}

#[test]
fn an_exit_or_a_failure_is_a_report_to_the_parent() {
    use crate::thread::notify::reports_end;
    let event = |kind: EventKind| {
        OpResult::Event(Some(Event {
            thread_id: ThreadId("thr_1".into()),
            seq: 1,
            at: Timestamp(0),
            kind,
        }))
    };
    assert!(reports_end(&event(EventKind::Exited {
        code: Some(1),
        signal: None,
    })));
    assert!(reports_end(&event(EventKind::Failed {
        error: Some("boom".into()),
    })));
    assert!(!reports_end(&event(EventKind::Idle)));
}
