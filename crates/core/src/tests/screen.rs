use std::path::Path;

use gpui_kit::TestAppContext;
use kg_domain::*;
use kg_store::{ScreenRow, Store};

use crate::Workspace;

use super::{
    Fixture, idle_thread, op, pid_of, program, screen_of, screen_with, started, thread, type_in,
    wait_for_status, wait_saved,
};

#[test]
fn the_saved_screen_uses_the_same_limits_as_a_resize() {
    assert_eq!(kg_terminal::MIN_COLS, *TERMINAL_COLS.start());
    assert_eq!(kg_terminal::MAX_COLS, *TERMINAL_COLS.end());
    assert_eq!(kg_terminal::MIN_ROWS, *TERMINAL_ROWS.start());
    assert_eq!(kg_terminal::MAX_ROWS, *TERMINAL_ROWS.end());
    assert_eq!(kg_terminal::SNAPSHOT_MAX, SCREEN_MAX_BYTES);
}

#[gpui_kit::test]
async fn confirmed_quit_stops_and_saves_the_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(unix)]
    type_in(&id, "printf QUIT-MARKER\n", cx).await;
    #[cfg(windows)]
    type_in(&id, "echo QUIT-MARKER\r\n", cx).await;
    assert!(
        screen_with(&id, "QUIT-MARKER", cx)
            .await
            .contains("QUIT-MARKER")
    );
    let stopping = cx.update(Workspace::stop_for_quit);
    wait_saved(fixture.dir.path(), &id, cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    stopping.await.unwrap();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
    assert!(screen_of(&id, true, cx).await.contains("QUIT-MARKER"));
    let fixture = fixture.restart(cx, |_| {}).await;
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(pid_of(&id, cx).is_some());
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id))
            .is_none()
    );
}

#[gpui_kit::test]
async fn a_crash_while_a_program_runs_keeps_no_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(unix)]
    type_in(&id, "printf CRASH-MARKER\n", cx).await;
    #[cfg(windows)]
    type_in(&id, "echo CRASH-MARKER\r\n", cx).await;
    assert!(
        screen_with(&id, "CRASH-MARKER", cx)
            .await
            .contains("CRASH-MARKER")
    );
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(pid_of(&id, cx).is_some());
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id))
            .is_none()
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&id))
            .is_none()
    );
    assert!(
        !fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.program_ended(&id))
    );
    assert!(
        !fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.recovery(&id))
    );
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    let replay = store.screen_replay(&id).unwrap().unwrap_or_default();
    assert!(
        !String::from_utf8_lossy(&replay).contains("CRASH-MARKER"),
        "the screen from the crashed run was kept"
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn a_quit_returns_once_the_screen_is_saved(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(unix)]
    type_in(&id, "printf SAVED-MARKER\n", cx).await;
    #[cfg(windows)]
    type_in(&id, "echo SAVED-MARKER\r\n", cx).await;
    assert!(
        screen_with(&id, "SAVED-MARKER", cx)
            .await
            .contains("SAVED-MARKER")
    );
    let stopping = cx.update(Workspace::stop_for_quit);
    let started = std::time::Instant::now();
    while !stopping.is_ready() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(8),
            "the quit did not finish"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(10));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(saved_screen(fixture.dir.path(), &id).is_some_and(|row| row.screen_kept));
    stopping.await.unwrap();
    drop(fixture);
}

#[gpui_kit::test]
async fn a_finished_program_keeps_its_screen_and_its_cause(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c echo KEEP-MARKER"
    } else {
        "/bin/sh -c \"printf 'KEEP-MARKER\\n'\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &started.id, cx);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Idle);
    assert!(pid_of(&started.id, cx).is_none());
    let text = screen_of(&started.id, true, cx).await;
    assert!(text.contains("KEEP-MARKER"), "{text}");
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&started.id)),
        Some("Exited with code 0.".into())
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.program_ended(&started.id))
    );
    assert!(
        !fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.recovery(&started.id))
    );
    let bound = cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.set_snapshot_bound(&started.id, 1)
        })
    });
    assert!(!bound);
    drop(fixture);
}

#[gpui_kit::test]
async fn a_failed_exit_keeps_the_screen_and_names_the_code(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c \"echo FAIL-MARKER& exit 1\""
    } else {
        "/bin/sh -c \"printf 'FAIL-MARKER\\n'; exit 1\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Error, cx).await;
    wait_saved(fixture.dir.path(), &started.id, cx);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Error);
    assert!(
        screen_of(&started.id, true, cx)
            .await
            .contains("FAIL-MARKER")
    );
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&started.id)),
        Some("Exited with code 1.".into())
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn a_stopped_program_keeps_its_screen_across_a_restart(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(unix)]
    type_in(&id, "printf STOP-MARKER\n", cx).await;
    #[cfg(windows)]
    type_in(&id, "echo STOP-MARKER\r\n", cx).await;
    assert!(
        screen_with(&id, "STOP-MARKER", cx)
            .await
            .contains("STOP-MARKER")
    );
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &id, cx);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert!(screen_of(&id, true, cx).await.contains("STOP-MARKER"));
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id)),
        Some("Stopped.".into())
    );
    assert!(pid_of(&id, cx).is_none());
    drop(fixture);
}

#[gpui_kit::test]
async fn a_screen_limit_is_still_told_after_reopening(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let fixture = fixture
        .restart(cx, move |store| {
            let kept = ThreadId("thr_kept".into());
            store
                .put_thread(&idle_thread("thr_kept", project.clone()))
                .unwrap();
            let started = store
                .append_events(&kept, Timestamp(1), [EventKind::Started])
                .unwrap();
            store
                .append_events(
                    &kept,
                    Timestamp(2),
                    [EventKind::Exited {
                        code: Some(0),
                        signal: None,
                    }],
                )
                .unwrap();
            store
                .put_screen(&kg_store::ScreenRow {
                    thread_id: kept.clone(),
                    started_seq: started[0].seq,
                    cols: 80,
                    rows: 24,
                    replay: b"KEPT-NOTE\n".to_vec(),
                    history_dropped: 4,
                    screen_kept: true,
                })
                .unwrap();
            let big = ThreadId("thr_big".into());
            store.put_thread(&idle_thread("thr_big", project)).unwrap();
            let started = store
                .append_events(&big, Timestamp(1), [EventKind::Started])
                .unwrap();
            store
                .append_events(&big, Timestamp(2), [EventKind::Stopped])
                .unwrap();
            store
                .put_screen(&kg_store::ScreenRow {
                    thread_id: big,
                    started_seq: started[0].seq,
                    cols: 80,
                    rows: 24,
                    replay: Vec::new(),
                    history_dropped: 0,
                    screen_kept: false,
                })
                .unwrap();
        })
        .await;
    let kept = ThreadId("thr_kept".into());
    let big = ThreadId("thr_big".into());
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&kept)),
        Some("Older scrollback was not kept.")
    );
    assert!(screen_of(&kept, true, cx).await.contains("KEPT-NOTE"));
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&big)),
        Some("The screen was outside the geometry or the 4 MiB bound.")
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.grid(&big, None))
            .is_none()
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn an_omission_is_announced_when_the_screen_settles(cx: &mut TestAppContext) {
    let note = "The screen was outside the geometry or the 4 MiB bound.";
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c ping -n 2 127.0.0.1 >nul"
    } else {
        "/bin/sh -c \"sleep 1\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    let id = started.id.clone();
    let announced = std::rc::Rc::new(std::cell::Cell::new(false));
    let workspace = fixture.workspace.clone();
    let _subscription = cx.update({
        let announced = announced.clone();
        let id = id.clone();
        move |cx| {
            cx.subscribe(
                &workspace,
                move |workspace, event: &crate::WorkspaceEvent, cx| {
                    if matches!(event, crate::WorkspaceEvent::Changed)
                        && workspace.read(cx).screen_note(&id) == Some(note)
                    {
                        announced.set(true);
                    }
                },
            )
        }
    });
    cx.run_until_parked();
    let bound = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let set = cx.update(|cx| {
            Workspace::global(cx).update(cx, |workspace, _| workspace.set_snapshot_bound(&id, 1))
        });
        if set {
            break;
        }
        cx.run_until_parked();
        assert!(
            std::time::Instant::now() < bound,
            "the program ended before the bound was set"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while !announced.get() {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        assert!(
            std::time::Instant::now() < deadline,
            "the omission was not announced after the screen settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&id)),
        Some(note)
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.recovery(&id))
    );
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, cx| {
            let seq = workspace.runs[&id].started_seq.expect("current run");
            crate::thread::remember(
                workspace,
                &id,
                seq.wrapping_add(1),
                Some(crate::end::Loss::History),
                cx,
            );
            assert_eq!(workspace.screen_note(&id), Some(note));
        });
    });
    drop(fixture);
}

#[gpui_kit::test]
async fn a_thread_left_starting_says_so_and_has_no_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let fixture = fixture
        .restart(cx, move |store| {
            let mut record = idle_thread("thr_start", project);
            record.status = ThreadStatus::Starting;
            store.put_thread(&record).unwrap();
        })
        .await;
    let id = ThreadId("thr_start".into());
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id)),
        Some("Stopped while starting.".into())
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.grid(&id, None))
            .is_none()
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn a_late_save_after_start_again_cannot_replace_the_new_run(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c echo FIRST-RUN"
    } else {
        "/bin/sh -c \"printf 'FIRST-RUN\\n'\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    let id = started.id.clone();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &id, cx);
    let path = fixture.dir.path().join("kagan.sqlite3");
    let old_seq = {
        let store = kg_store::Store::open(&path).unwrap();
        store
            .screens()
            .unwrap()
            .into_iter()
            .find(|row| row.thread_id == id)
            .unwrap()
            .started_seq
    };
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        let store = kg_store::Store::open(&path).unwrap();
        let current = store
            .screens()
            .unwrap()
            .into_iter()
            .find(|row| row.thread_id == id);
        let replaced = match &current {
            None => true,
            Some(row) => row.started_seq != old_seq,
        };
        if replaced {
            assert!(
                !store
                    .put_screen(&kg_store::ScreenRow {
                        thread_id: id.clone(),
                        started_seq: old_seq,
                        cols: 80,
                        rows: 24,
                        replay: b"STALE-OLD".to_vec(),
                        history_dropped: 0,
                        screen_kept: true,
                    })
                    .unwrap()
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "start again did not replace the run"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &id, cx);
    let saved = kg_store::Store::open(&path)
        .unwrap()
        .screens()
        .unwrap()
        .into_iter()
        .find(|row| row.thread_id == id)
        .unwrap();
    assert_ne!(saved.started_seq, old_seq);
    assert!(!saved.replay.windows(9).any(|chunk| chunk == b"STALE-OLD"));
    let text = screen_of(&id, true, cx).await;
    assert!(text.contains("FIRST-RUN"), "{text}");
    drop(fixture);
}

#[gpui_kit::test]
async fn an_unsafe_screen_is_a_limit_after_reopening(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let db = fixture.dir.path().join("kagan.sqlite3");
    let fixture = fixture
        .restart(cx, move |store| {
            let wide = ThreadId("thr_wide".into());
            store
                .put_thread(&idle_thread("thr_wide", project.clone()))
                .unwrap();
            let started = store
                .append_events(&wide, Timestamp(1), [EventKind::Started])
                .unwrap();
            store
                .append_events(
                    &wide,
                    Timestamp(2),
                    [EventKind::Exited {
                        code: Some(0),
                        signal: None,
                    }],
                )
                .unwrap();
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "INSERT INTO terminal_screens \
                    (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
                 VALUES ('thr_wide', ?1, 9999, 24, X'ABCD', 9, 1)",
                [started[0].seq],
            )
            .unwrap();
            let short = ThreadId("thr_short".into());
            store
                .put_thread(&idle_thread("thr_short", project))
                .unwrap();
            let started = store
                .append_events(&short, Timestamp(1), [EventKind::Started])
                .unwrap();
            store
                .append_events(&short, Timestamp(2), [EventKind::Stopped])
                .unwrap();
            conn.execute(
                "INSERT INTO terminal_screens \
                    (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
                 VALUES ('thr_short', ?1, 80, 0, X'00', 2, 1)",
                [started[0].seq],
            )
            .unwrap();
        })
        .await;
    let wide = ThreadId("thr_wide".into());
    let short = ThreadId("thr_short".into());
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&wide)),
        Some("The screen was outside the geometry or the 4 MiB bound.")
    );
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&short)),
        Some("The screen was outside the geometry or the 4 MiB bound.")
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.grid(&wide, None))
            .is_none()
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.grid(&short, None))
            .is_none()
    );
    drop(fixture);
}

fn saved_screen(dir: &Path, id: &ThreadId) -> Option<ScreenRow> {
    Store::open(dir.join("kagan.sqlite3"))
        .ok()?
        .screens()
        .ok()?
        .into_iter()
        .find(|row| &row.thread_id == id)
}

#[gpui_kit::test]
async fn a_sweep_deletes_saved_screens_only_when_it_releases_one(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c echo KEEP-MARKER"
    } else {
        "/bin/sh -c \"printf 'KEEP-MARKER\\n'\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &started.id, cx);
    let row = saved_screen(fixture.dir.path(), &started.id).expect("the saved screen");
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    for _ in 0..400 {
        if saved_screen(fixture.dir.path(), &started.id).is_none() {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(saved_screen(fixture.dir.path(), &started.id).is_none());
    Store::open(fixture.dir.path().join("kagan.sqlite3"))
        .unwrap()
        .put_screen(&row)
        .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(3));
    cx.run_until_parked();
    assert!(saved_screen(fixture.dir.path(), &started.id).is_some());
}

#[gpui_kit::test]
async fn starting_a_thread_does_not_delete_an_old_saved_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c echo KEEP-MARKER"
    } else {
        "/bin/sh -c \"printf 'KEEP-MARKER\\n'\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &started.id, cx);
    let row = saved_screen(fixture.dir.path(), &started.id).expect("the saved screen");
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    for _ in 0..400 {
        if saved_screen(fixture.dir.path(), &started.id).is_none() {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Store::open(fixture.dir.path().join("kagan.sqlite3"))
        .unwrap()
        .put_screen(&row)
        .unwrap();
    op(fixture.run(line), cx).await.unwrap();
    cx.run_until_parked();
    assert!(saved_screen(fixture.dir.path(), &started.id).is_some());
}

#[gpui_kit::test]
async fn a_screen_of_a_long_archived_thread_is_dropped_at_start(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let dir = fixture.dir.path().to_path_buf();
    let fixture = fixture
        .restart(cx, move |store| {
            let now = crate::workspace::now();
            for (id, archived) in [("thr_long_ago", Timestamp(1)), ("thr_just_now", now)] {
                let mut record = idle_thread(id, project.clone());
                record.archived_at = Some(archived);
                store.put_thread(&record).unwrap();
                let thread = ThreadId(id.into());
                let started = store
                    .append_events(&thread, Timestamp(1), [EventKind::Started])
                    .unwrap();
                store
                    .put_screen(&ScreenRow {
                        thread_id: thread,
                        started_seq: started[0].seq,
                        cols: 80,
                        rows: 24,
                        replay: b"kept?\n".to_vec(),
                        history_dropped: 0,
                        screen_kept: true,
                    })
                    .unwrap();
            }
        })
        .await;
    let left: Vec<String> = Store::open(dir.join("kagan.sqlite3"))
        .unwrap()
        .screens()
        .unwrap()
        .into_iter()
        .map(|row| row.thread_id.to_string())
        .collect();
    assert_eq!(left, ["thr_just_now"]);
    drop(fixture);
}
