use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, op, started, thread, wait_for_status};
use crate::Workspace;

async fn shell_fixture(cx: &mut TestAppContext) -> Fixture {
    let fixture = Fixture::new(cx).await;
    let path = std::env::var_os("PATH").unwrap_or_default();
    fixture.workspace.update(cx, |workspace, _| {
        workspace
            .use_program_environment(std::path::Path::new(&path), std::path::Path::new("/bin/sh"));
    });
    fixture
}

async fn terminal(op_: TerminalOp, cx: &mut TestAppContext) -> Result<OpResult, OpError> {
    op(Op::Thread(ThreadOp::Terminal(op_)), cx).await
}

async fn open(id: &ThreadId, cx: &mut TestAppContext) -> u32 {
    match terminal(TerminalOp::New { id: id.clone() }, cx)
        .await
        .unwrap()
    {
        OpResult::Terminals(opened) => opened[0].number,
        other => panic!("{other:?}"),
    }
}

async fn numbers(id: &ThreadId, cx: &mut TestAppContext) -> Vec<u32> {
    match terminal(TerminalOp::List { id: id.clone() }, cx)
        .await
        .unwrap()
    {
        OpResult::Terminals(list) => list.into_iter().map(|extra| extra.number).collect(),
        other => panic!("{other:?}"),
    }
}

async fn type_into(id: &ThreadId, number: Option<u32>, text: &str, cx: &mut TestAppContext) {
    op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            text: text.into(),
            terminal: number,
        }),
        cx,
    )
    .await
    .unwrap();
}

async fn screen(id: &ThreadId, number: Option<u32>, cx: &mut TestAppContext) -> String {
    match op(
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback: false,
            lines: None,
            terminal: number,
        }),
        cx,
    )
    .await
    .unwrap()
    {
        OpResult::Screen(text) => text,
        other => panic!("{other:?}"),
    }
}

async fn until(cx: &mut TestAppContext, mut done: impl AsyncFnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !done(cx).await {
        assert!(Instant::now() < deadline, "timed out");
        cx.executor().advance_clock(Duration::from_millis(40));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[gpui_kit::test]
async fn an_extra_terminal_runs_a_shell_in_the_threads_folder(cx: &mut TestAppContext) {
    let fixture = shell_fixture(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    let number = open(&id, cx).await;
    assert_eq!(number, 1);
    type_into(
        &id,
        Some(number),
        "printf 'extra-%s\\n' \"$(basename \"$PWD\")\"\n",
        cx,
    )
    .await;
    let folder = fixture
        .dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let wanted = format!("extra-{folder}");
    until(cx, async |cx| {
        screen(&id, Some(number), cx).await.contains(&wanted)
    })
    .await;
    assert!(!screen(&id, None, cx).await.contains("extra-"));
    let grid = cx
        .update(|cx| Workspace::global(cx).read(cx).grid(&id, Some(number)))
        .unwrap();
    assert!(grid.lines.iter().any(|line| {
        line.iter()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains(&wanted)
    }));
}

#[gpui_kit::test]
async fn numbers_are_not_reused_and_close_ends_one(cx: &mut TestAppContext) {
    let fixture = shell_fixture(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    assert_eq!(open(&id, cx).await, 1);
    assert_eq!(open(&id, cx).await, 2);
    assert_eq!(numbers(&id, cx).await, vec![1, 2]);
    terminal(
        TerminalOp::Close {
            id: id.clone(),
            number: 1,
        },
        cx,
    )
    .await
    .unwrap();
    assert_eq!(numbers(&id, cx).await, vec![2]);
    assert!(matches!(
        terminal(
            TerminalOp::Close {
                id: id.clone(),
                number: 1
            },
            cx
        )
        .await,
        Err(OpError::NotFound(_))
    ));
    terminal(
        TerminalOp::Close {
            id: id.clone(),
            number: 2,
        },
        cx,
    )
    .await
    .unwrap();
    assert_eq!(open(&id, cx).await, 3);
    assert!(matches!(
        op(
            Op::Thread(ThreadOp::Input {
                id: id.clone(),
                text: "x".into(),
                terminal: Some(1),
            }),
            cx,
        )
        .await,
        Err(OpError::NotFound(_))
    ));
}

#[gpui_kit::test]
async fn a_shell_that_exits_closes_its_tab_and_leaves_the_thread_alone(cx: &mut TestAppContext) {
    let fixture = shell_fixture(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    let before = thread(&id, cx).await;
    let number = open(&id, cx).await;
    type_into(&id, Some(number), "exit 3\n", cx).await;
    until(cx, async |cx| numbers(&id, cx).await.is_empty()).await;
    let after = thread(&id, cx).await;
    assert_eq!(after.status, ThreadStatus::Active);
    assert_eq!(after.mark, before.mark);
    assert_eq!(after.latest_attention_at, before.latest_attention_at);
}

#[gpui_kit::test]
async fn archiving_ends_extra_terminals_and_refuses_new_ones(cx: &mut TestAppContext) {
    let fixture = shell_fixture(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    open(&id, cx).await;
    op(Op::Thread(ThreadOp::Archive { id: id.clone() }), cx)
        .await
        .unwrap();
    assert_eq!(
        numbers(&id, cx).await,
        vec![1],
        "Undo keeps them for the grace"
    );
    cx.executor().advance_clock(Duration::from_secs(12));
    cx.run_until_parked();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    assert!(numbers(&id, cx).await.is_empty());
    assert!(matches!(
        terminal(TerminalOp::New { id: id.clone() }, cx).await,
        Err(OpError::Invalid(_))
    ));
}

#[gpui_kit::test]
async fn quitting_ends_extra_terminals(cx: &mut TestAppContext) {
    let fixture = shell_fixture(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    open(&id, cx).await;
    fixture
        .workspace
        .update(cx, |workspace, _| workspace.end_programs());
    assert!(numbers(&id, cx).await.is_empty());
}
