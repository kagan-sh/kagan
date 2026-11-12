use std::time::Duration;

use gpui_kit::{Task, TestAppContext};
use kg_domain::*;

use super::{Fixture, events, op, program};
use crate::{Requester, Workspace};

fn add(project: &ProjectId, minutes: Option<u16>, in_minutes: Option<u16>) -> Op {
    Op::Automation(AutomationOp::Add {
        name: "Morning".into(),
        body: "ping".into(),
        project_id: project.clone(),
        program: program().into(),
        minutes,
        in_minutes,
    })
}

async fn automations(cx: &mut TestAppContext) -> Vec<Automation> {
    let OpResult::Automations(rows) =
        op(Op::Automation(AutomationOp::List { project_id: None }), cx)
            .await
            .unwrap()
    else {
        panic!("list")
    };
    rows
}

fn delete(id: &AutomationId, cx: &mut TestAppContext) -> Task<Result<OpResult, OpError>> {
    cx.update(|cx| Workspace::dispatch(Op::Automation(AutomationOp::Delete { id: id.clone() }), cx))
}

async fn approve(cx: &mut TestAppContext) {
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
}

async fn thread_count(cx: &mut TestAppContext) -> usize {
    let OpResult::Threads(threads) = op(
        Op::Thread(ThreadOp::List {
            project_id: None,
            archived: None,
            limit: None,
            before: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("threads")
    };
    threads.len()
}

async fn sent(id: &ThreadId, cx: &mut TestAppContext) -> bool {
    events(id, cx).await.into_iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Message {
                role: Role::User,
                ref text,
                ..
            } if text == "ping"
        )
    })
}

async fn drive_until_sent(cx: &mut TestAppContext) -> ThreadId {
    for _ in 0..80 {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let rows = automations(cx).await;
        if let Some(id) = rows.first().and_then(|row| row.threads.first().cloned())
            && sent(&id, cx).await
        {
            return id;
        }
    }
    panic!("the automation never sent its text")
}

#[gpui_kit::test]
async fn add_refuses_a_missing_time_and_a_missing_project(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let neither = op(add(&fixture.project, None, None), cx).await.unwrap_err();
    assert!(
        matches!(neither, OpError::Invalid(ref message) if message.contains("needs minutes")),
        "{neither}"
    );
    let both = op(add(&fixture.project, Some(1), Some(1)), cx)
        .await
        .unwrap_err();
    assert!(matches!(both, OpError::Invalid(_)), "{both}");
    let zero = op(add(&fixture.project, Some(0), None), cx)
        .await
        .unwrap_err();
    assert!(matches!(zero, OpError::Invalid(_)), "{zero}");
    let missing = op(add(&ProjectId("prj_missing".into()), Some(1), None), cx)
        .await
        .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
}

#[gpui_kit::test]
async fn a_paused_automation_does_not_start_a_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Automation(saved) = op(add(&fixture.project, Some(1), None), cx).await.unwrap()
    else {
        panic!("add")
    };
    op(
        Op::Automation(AutomationOp::Pause {
            id: saved.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    cx.executor().advance_clock(Duration::from_mins(2));
    cx.run_until_parked();
    assert_eq!(thread_count(cx).await, 0);
    assert!(automations(cx).await[0].threads.is_empty());
    assert!(automations(cx).await[0].paused);

    op(Op::Automation(AutomationOp::Resume { id: saved.id }), cx)
        .await
        .unwrap();
    assert!(!automations(cx).await[0].paused);
    let started = drive_until_sent(cx).await;
    assert_eq!(automations(cx).await[0].threads, vec![started]);
}

#[gpui_kit::test]
async fn a_due_automation_starts_a_thread_and_sends_its_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(add(&fixture.project, Some(1), None), cx).await.unwrap();
    cx.executor().advance_clock(Duration::from_secs(30));
    cx.run_until_parked();
    assert_eq!(
        thread_count(cx).await,
        0,
        "the first run waits one interval"
    );

    cx.executor().advance_clock(Duration::from_secs(30));
    let id = drive_until_sent(cx).await;
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
    assert_eq!(automations(cx).await[0].threads, vec![id]);
}

#[gpui_kit::test]
async fn a_running_thread_is_left_alone_when_the_automation_is_due_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(add(&fixture.project, Some(1), None), cx).await.unwrap();
    cx.executor().advance_clock(Duration::from_secs(60));
    let id = drive_until_sent(cx).await;

    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(thread_count(cx).await, 1);
    assert_eq!(automations(cx).await[0].threads, vec![id.clone()]);
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
}

#[gpui_kit::test]
async fn one_time_runs_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(add(&fixture.project, None, Some(1)), cx).await.unwrap();
    cx.executor().advance_clock(Duration::from_mins(1));
    let id = drive_until_sent(cx).await;
    cx.executor().advance_clock(Duration::from_mins(5));
    cx.run_until_parked();
    assert_eq!(automations(cx).await[0].threads, vec![id]);
    assert!(automations(cx).await[0].next_at.is_none());
    assert_eq!(thread_count(cx).await, 1);
}

#[gpui_kit::test]
async fn deleting_an_automation_leaves_a_running_thread_and_starts_no_other(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Automation(saved) = op(add(&fixture.project, None, Some(1)), cx).await.unwrap()
    else {
        panic!("add")
    };
    cx.executor().advance_clock(Duration::from_mins(1));
    let id = drive_until_sent(cx).await;
    let deleting = delete(&saved.id, cx);
    cx.run_until_parked();
    approve(cx).await;
    deleting.await.unwrap();
    cx.executor().advance_clock(Duration::from_mins(5));
    cx.run_until_parked();
    assert!(automations(cx).await.is_empty());
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
}

#[gpui_kit::test]
async fn a_one_time_automation_that_cannot_start_waits_before_it_retries(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(
        Op::Automation(AutomationOp::Add {
            name: "Missing".into(),
            body: "ping".into(),
            project_id: fixture.project.clone(),
            program: "kg-no-such-program".into(),
            minutes: None,
            in_minutes: Some(1),
        }),
        cx,
    )
    .await
    .unwrap();
    let due = automations(cx).await[0].next_at;
    cx.executor()
        .advance_clock(Duration::from_mins(1) + Duration::from_secs(1));
    cx.run_until_parked();
    let row = automations(cx).await.remove(0);
    assert!(
        row.last_error.is_some(),
        "the failure must be kept on the row"
    );
    assert!(row.threads.is_empty());
    assert_eq!(thread_count(cx).await, 0);
    assert!(
        row.next_at
            .is_some_and(|next| due.is_some_and(|due| next > due)),
        "a failed start must wait, not retry every second: {:?}",
        row.next_at
    );
}

#[gpui_kit::test]
async fn a_start_that_fails_before_its_thread_is_saved_is_not_a_run(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let raw = rusqlite::Connection::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    raw.execute_batch(
        "CREATE TRIGGER no_threads BEFORE INSERT ON threads BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
    )
    .unwrap();
    op(
        Op::Automation(AutomationOp::Add {
            name: "Once".into(),
            body: "ping".into(),
            project_id: fixture.project.clone(),
            program: program().into(),
            minutes: None,
            in_minutes: Some(1),
        }),
        cx,
    )
    .await
    .unwrap();
    let due = automations(cx).await[0].next_at;
    cx.executor()
        .advance_clock(Duration::from_mins(1) + Duration::from_secs(1));
    cx.run_until_parked();
    let row = automations(cx).await.remove(0);
    assert!(row.threads.is_empty(), "{:?}", row.threads);
    assert_eq!(thread_count(cx).await, 0);
    assert!(
        row.next_at
            .is_some_and(|next| due.is_some_and(|due| next > due)),
        "a start that saved no thread must be retried, not kept as a run: {:?}",
        row.next_at
    );

    raw.execute_batch("DROP TRIGGER no_threads").unwrap();
    cx.executor().advance_clock(Duration::from_mins(2));
    let id = drive_until_sent(cx).await;
    assert_eq!(automations(cx).await[0].threads, vec![id]);
}

#[gpui_kit::test]
async fn a_store_read_error_does_not_stop_the_automation_loop(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(add(&fixture.project, Some(1), None), cx).await.unwrap();
    let raw = rusqlite::Connection::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    raw.execute("UPDATE automations SET name = x'FF'", [])
        .unwrap();
    cx.executor().advance_clock(Duration::from_mins(2));
    cx.run_until_parked();
    raw.execute("UPDATE automations SET name = 'Morning'", [])
        .unwrap();
    let id = drive_until_sent(cx).await;
    assert_eq!(automations(cx).await[0].threads, vec![id]);
}

#[gpui_kit::test]
async fn deleting_an_automation_waits_for_the_person(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Automation(saved) = op(add(&fixture.project, Some(1), None), cx).await.unwrap()
    else {
        panic!("add")
    };
    let deleting = delete(&saved.id, cx);
    cx.run_until_parked();
    assert_eq!(
        automations(cx).await.len(),
        1,
        "nothing is deleted before the person approves"
    );
    approve(cx).await;
    deleting.await.unwrap();
    assert!(automations(cx).await.is_empty());
}

#[cfg(unix)]
fn pause_heard(
    paused: &async_channel::Receiver<(String, String)>,
    cx: &mut TestAppContext,
) -> Option<(String, String)> {
    for _ in 0..200 {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        if let Ok(event) = paused.try_recv() {
            return Some(event);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[cfg(unix)]
async fn runs_with_status(
    automation: &AutomationId,
    status: ThreadStatus,
    count: usize,
    cx: &mut TestAppContext,
) -> bool {
    for _ in 0..200 {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let row = automations(cx)
            .await
            .into_iter()
            .find(|row| &row.id == automation);
        if let Some(row) = row
            && row.threads.len() == count
            && super::thread(&row.threads[count - 1], cx).await.status == status
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[cfg(unix)]
#[gpui_kit::test]
async fn three_failed_runs_in_a_row_pause_the_automation(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Automation(automation) = op(
        Op::Automation(AutomationOp::Add {
            name: "Failing".into(),
            body: "ping".into(),
            project_id: fixture.project.clone(),
            program: "/bin/sh -c 'exit 3'".into(),
            minutes: Some(1),
            in_minutes: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("automation")
    };
    let (told, paused) = async_channel::unbounded();
    let workspace = fixture.workspace.clone();
    let _subscription = cx.update(move |cx| {
        cx.subscribe(&workspace, move |_, event: &crate::WorkspaceEvent, _| {
            if let crate::WorkspaceEvent::AutomationPaused(name, reason) = event {
                let _ = told.try_send((name.clone(), reason.clone()));
            }
        })
    });
    for run in 1..=3 {
        cx.executor().advance_clock(Duration::from_secs(60));
        assert!(
            runs_with_status(&automation.id, ThreadStatus::Error, run, cx).await,
            "run {run} did not fail"
        );
    }
    let row = automations(cx).await.remove(0);
    assert!(row.failing, "three failed runs are a failing automation");
    assert!(!row.paused, "it pauses only when the next run is due");

    cx.executor().advance_clock(Duration::from_secs(60));
    let heard = pause_heard(&paused, cx);
    assert_eq!(
        heard,
        Some(("Failing".to_owned(), "3 runs failed in a row".to_owned())),
        "the due run pauses instead of starting"
    );
    let row = automations(cx).await.remove(0);
    assert!(row.paused, "the due run pauses instead of starting");
    assert!(
        paused.try_recv().is_err(),
        "the activity log hears about the pause once"
    );
    assert!(row.failing);
    assert_eq!(row.threads.len(), 3, "no fourth thread was started");
    assert_eq!(thread_count(cx).await, 3);

    op(
        Op::Automation(AutomationOp::Resume {
            id: automation.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let row = automations(cx).await.remove(0);
    assert!(!row.failing, "a resume starts a new streak");
    cx.executor().advance_clock(Duration::from_secs(60));
    assert!(
        runs_with_status(&automation.id, ThreadStatus::Error, 4, cx).await,
        "the first run after a resume starts"
    );
    assert!(!automations(cx).await.remove(0).failing);
}

#[gpui_kit::test]
async fn a_person_question_is_reported_to_the_requester_by_title(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Automation(saved) = op(add(&fixture.project, Some(1), None), cx).await.unwrap()
    else {
        panic!("add")
    };
    let (gone_sender, gone) = async_channel::bounded::<()>(1);
    let (questions, asked) = async_channel::unbounded();
    let requester = Requester::new(gone).waiting_on(questions);
    let deleting = cx.update(|cx| {
        Workspace::dispatch_for(
            Op::Automation(AutomationOp::Delete {
                id: saved.id.clone(),
            }),
            Some(requester),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        asked.try_recv().unwrap(),
        "Delete the automation “Morning”?"
    );
    approve(cx).await;
    deleting.await.unwrap();
    drop(gone_sender);
}

#[test]
fn a_failure_reason_is_one_short_line() {
    assert_eq!(crate::automation::one_line("first\nsecond"), "first");
    assert_eq!(crate::automation::one_line(""), "");
    assert_eq!(
        crate::automation::one_line(&"x".repeat(500))
            .chars()
            .count(),
        200
    );
}
