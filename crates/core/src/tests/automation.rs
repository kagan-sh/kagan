use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, events, op, program};

fn add(project: &ProjectId, minutes: Option<u16>, at: Option<Timestamp>) -> Op {
    Op::Automation(AutomationOp::Add {
        name: "Morning".into(),
        body: "ping".into(),
        project_id: project.clone(),
        program: program().into(),
        minutes,
        at,
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
    let both = op(add(&fixture.project, Some(1), Some(Timestamp(1))), cx)
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
    op(add(&fixture.project, None, Some(Timestamp(1))), cx)
        .await
        .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
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
    let OpResult::Automation(saved) = op(add(&fixture.project, None, Some(Timestamp(1))), cx)
        .await
        .unwrap()
    else {
        panic!("add")
    };
    cx.executor().advance_clock(Duration::from_secs(1));
    let id = drive_until_sent(cx).await;
    op(Op::Automation(AutomationOp::Delete { id: saved.id }), cx)
        .await
        .unwrap();
    cx.executor().advance_clock(Duration::from_mins(5));
    cx.run_until_parked();
    assert!(automations(cx).await.is_empty());
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
}
