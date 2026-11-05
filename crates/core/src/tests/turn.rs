use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, ask_op, ask_options, assert_quit_leaves_the_turn, begin_wait, busy_and_unread,
    busy_of, finish_quit, finish_wait, kinds_of, op, outcome, pending, row_end_after_its_event,
    screen_with, script, set_row_status, settle, sleeping, started, thread, turn_counts,
    wait_for_status, wait_for_waiting, wait_until_busy, waiting_count,
};

#[cfg(unix)]
#[gpui_kit::test]
async fn a_natural_exit_cancels_the_questions_and_clears_busy(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "busy",
        "printf '\\033]0;\\342\\240\\213 working\\007'\nsleep 1\nexit 0",
    );
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    let options = vec![
        InteractionOption {
            id: "approve".into(),
            label: "Approve".into(),
        },
        InteractionOption {
            id: "decline".into(),
            label: "Decline".into(),
        },
    ];
    let asking = cx.update(|cx| {
        Workspace::ask_for_person(Some(&started.id), None, "Keep going?".into(), options, cx)
    });
    cx.run_until_parked();
    let mut busy_seen = false;
    for _ in 0..200 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        cx.run_until_parked();
        if busy_of(&fixture, &started.id, cx) {
            busy_seen = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(busy_seen, "the spinner title never made the thread busy");
    let OpResult::Interactions(pending) = op(
        Op::Thread(ThreadOp::Interactions {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("interactions")
    };
    assert_eq!(pending.len(), 1);
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    assert!(!busy_of(&fixture, &started.id, cx));
    let OpResult::Interactions(pending) = op(
        Op::Thread(ThreadOp::Interactions {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("interactions")
    };
    assert!(pending.is_empty(), "{pending:?}");
    let kinds = kinds_of(&started.id, cx).await;
    assert!(kinds.contains(&EventKind::Exited {
        code: Some(0),
        signal: None,
    }));
    drop(asking);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_plain_title_after_a_spinner_records_waiting_once_per_reply(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "quiet",
        "dir=$(CDPATH= cd -- \"$(dirname \"$0\")\" && pwd)\n\
         spin=$(printf '\\342\\240\\213')\n\
         title() { printf '\\033]0;%s\\007' \"$1\"; }\n\
         waitgo() { while [ ! -f \"$dir/go\" ]; do sleep 0.05; done; rm -f \"$dir/go\"; }\n\
         title \"$spin working\"\n\
         waitgo\n\
         title ready\n\
         waitgo\n\
         title \"$spin working\"\n\
         waitgo\n\
         title ready\n\
         sleep 60\n",
    );
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    op(
        Op::Thread(ThreadOp::MarkRead {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let go = fixture.dir.path().join("go");
    let release = || std::fs::write(&go, b"go").unwrap();
    wait_until_busy(&fixture, &started.id, cx);
    release();
    wait_for_waiting(&started.id, 1, cx).await;
    let noticed = thread(&started.id, cx).await;
    assert_eq!(noticed.mark, kg_domain::ThreadMark::Done);
    let OpResult::Threads(list) = op(
        Op::Thread(ThreadOp::List {
            project_id: None,
            archived: Some(false),
            limit: None,
            before: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("threads")
    };
    assert_eq!(
        list.iter()
            .find(|thread| thread.id == started.id)
            .unwrap()
            .mark,
        kg_domain::ThreadMark::Done
    );
    op(
        Op::Thread(ThreadOp::MarkRead {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(
        thread(&started.id, cx).await.mark,
        kg_domain::ThreadMark::None
    );
    assert_eq!(noticed.status, ThreadStatus::Active);
    assert!(!busy_of(&fixture, &started.id, cx));
    for _ in 0..40 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
    }
    assert_eq!(waiting_count(&started.id, cx).await, 1);
    release();
    wait_until_busy(&fixture, &started.id, cx);
    release();
    wait_for_waiting(&started.id, 2, cx).await;
    let kinds = kinds_of(&started.id, cx).await;
    assert_eq!(turn_counts(&kinds), (2, 2), "{kinds:?}");
    assert!(
        !kinds.iter().any(|kind| matches!(kind, EventKind::Idle)),
        "{kinds:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_stop_during_a_spinner_ends_the_turn_as_interrupted(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "spin-stop",
        "printf '\\033]0;\\342\\240\\213 working\\007'\nsleep 60\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    wait_until_busy(&fixture, &id, cx);
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    let kinds = kinds_of(&id, cx).await;
    assert!(kinds.contains(&EventKind::TurnStarted), "{kinds:?}");
    assert!(
        kinds.contains(&EventKind::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            error: None,
        }),
        "{kinds:?}"
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| matches!(kind, EventKind::TurnStarted))
            .count(),
        1,
        "{kinds:?}"
    );
    assert!(kinds.contains(&EventKind::Stopped), "{kinds:?}");
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, EventKind::Failed { .. })),
        "{kinds:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_quit_during_a_spinner_does_not_end_the_turn(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "spin-quit",
        "printf '\\033]0;\\342\\240\\213 working\\007'\nsleep 60\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    wait_until_busy(&fixture, &id, cx);
    let quitting = cx.update(Workspace::stop_for_quit);
    finish_quit(cx, &id, quitting).await;
    assert_quit_leaves_the_turn(&kinds_of(&id, cx).await);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_quit_while_a_plain_title_settles_does_not_end_the_turn(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "spin-quiet-quit",
        "dir=$(CDPATH= cd -- \"$(dirname \"$0\")\" && pwd)\n\
         spin=$(printf '\\342\\240\\213')\n\
         printf '\\033]0;%s working\\007' \"$spin\"\n\
         while [ ! -f \"$dir/plain\" ]; do sleep 0.05; done\n\
         printf '\\033]0;ready\\007'\n\
         sleep 60\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    wait_until_busy(&fixture, &id, cx);
    std::fs::write(fixture.dir.path().join("plain"), b"go").unwrap();
    for _ in 0..40 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        let pending = cx.read(|cx| {
            fixture
                .workspace
                .read(cx)
                .thread(&id)
                .unwrap()
                .read(cx)
                .plain_since
                .is_some()
        });
        if pending {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        cx.read(|cx| {
            let thread = fixture.workspace.read(cx).thread(&id).unwrap();
            thread.read(cx).busy() && thread.read(cx).plain_since.is_some()
        }),
        "the plain title never started its quiet wait"
    );
    let quitting = cx.update(Workspace::stop_for_quit);
    finish_quit(cx, &id, quitting).await;
    assert_quit_leaves_the_turn(&kinds_of(&id, cx).await);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_exit_during_a_spinner_ends_the_turn_as_failed(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "spin-fail",
        "printf '\\033]0;\\342\\240\\213 working\\007'\nexit 7\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    wait_for_status(&id, ThreadStatus::Error, cx).await;
    let kinds = kinds_of(&id, cx).await;
    let error = "the program exited with code 7";
    assert!(kinds.contains(&EventKind::TurnStarted), "{kinds:?}");
    assert!(
        kinds.contains(&EventKind::TurnEnded {
            outcome: TurnOutcome::Failed,
            error: Some(error.into()),
        }),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&EventKind::Failed {
            error: Some(error.into()),
        }),
        "{kinds:?}"
    );
}

#[gpui_kit::test]
async fn wait_for_a_missing_thread_is_not_found(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let error = op(
        Op::Thread(ThreadOp::Wait {
            id: ThreadId("missing".into()),
            timeout_ms: 1_000,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("not found"), "{error}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn wait_on_an_ended_program_returns_at_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf done\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    assert_eq!(settle(&started.id, 5_000, cx).await, WaitOutcome::Exited);
    let failed = op(fixture.run("/bin/sh -c \"exit 7\""), cx).await.unwrap();
    let OpResult::Thread(failed) = failed else {
        panic!("thread")
    };
    wait_for_status(&failed.id, ThreadStatus::Error, cx).await;
    assert_eq!(settle(&failed.id, 5_000, cx).await, WaitOutcome::Failed);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_quiet_program_is_not_busy_only_after_the_arm(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh -c \"sleep 30\"", cx).await;
    let task = begin_wait(&id, 10_000, cx);
    cx.run_until_parked();
    assert!(!task.is_ready());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(!task.is_ready());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::NotBusy);
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_timeout_leaves_the_thread_alone(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh -c \"sleep 30\"", cx).await;
    let before = kinds_of(&id, cx).await;
    let task = begin_wait(&id, 200, cx);
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::Timeout);
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
    assert_eq!(kinds_of(&id, cx).await, before);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_exit_during_the_wait_names_exited_or_failed(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh -c \"sleep 0.2; exit 0\"", cx).await;
    let task = begin_wait(&id, 10_000, cx);
    finish_wait(&task, cx);
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::Exited);
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);

    let (id, _reap) = started(&fixture, "/bin/sh -c \"sleep 0.2; exit 7\"", cx).await;
    let task = begin_wait(&id, 10_000, cx);
    finish_wait(&task, cx);
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::Failed);
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn stop_during_the_wait_returns_exited(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh -c \"sleep 30\"", cx).await;
    let task = begin_wait(&id, 10_000, cx);
    cx.run_until_parked();
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    finish_wait(&task, cx);
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::Exited);
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn stop_while_starting_returns_exited(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh -c \"sleep 30\"", cx).await;
    set_row_status(&fixture, &id, ThreadStatus::Starting, cx);
    let before = kinds_of(&id, cx).await;
    let task = begin_wait(&id, 10_000, cx);
    cx.run_until_parked();
    assert!(!task.is_ready());
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(task.is_ready());
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::Exited);
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
    assert_eq!(kinds_of(&id, cx).await, before);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_end_logged_before_the_wait_returns_when_the_row_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    row_end_after_its_event(
        &fixture,
        EventKind::Idle,
        ThreadStatus::Idle,
        WaitOutcome::Exited,
        cx,
    )
    .await;
    row_end_after_its_event(
        &fixture,
        EventKind::Failed { error: None },
        ThreadStatus::Error,
        WaitOutcome::Failed,
        cx,
    )
    .await;
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_spinner_returns_waiting_only_after_the_quiet(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "settle",
        "dir=$(CDPATH= cd -- \"$(dirname \"$0\")\" && pwd)\n\
         spin=$(printf '\\342\\240\\213')\n\
         title() { printf '\\033]0;%s\\007' \"$1\"; }\n\
         while [ ! -f \"$dir/go\" ]; do sleep 0.05; done\n\
         rm -f \"$dir/go\"\n\
         title \"$spin working\"\n\
         while [ ! -f \"$dir/go\" ]; do sleep 0.05; done\n\
         rm -f \"$dir/go\"\n\
         title ready\n\
         sleep 60\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    let go = fixture.dir.path().join("go");
    std::fs::write(&go, b"go").unwrap();
    wait_until_busy(&fixture, &id, cx);
    let task = begin_wait(&id, 20_000, cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(!task.is_ready());
    std::fs::write(&go, b"go").unwrap();
    let mut after_plain = 0;
    let mut seen_plain = false;
    for _ in 0..400 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        let plain = cx.read(|cx| {
            fixture
                .workspace
                .read(cx)
                .thread(&id)
                .unwrap()
                .read(cx)
                .plain_since
                .is_some()
        });
        if plain {
            if seen_plain {
                after_plain += 50;
            }
            if task.is_ready() {
                assert!(
                    after_plain >= 1_000,
                    "waiting returned after {after_plain}ms of quiet"
                );
                break;
            }
            seen_plain = true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(task.is_ready());
    assert_eq!(outcome(&task.await.unwrap()), WaitOutcome::Waiting);
    assert_eq!(waiting_count(&id, cx).await, 1);
}

#[gpui_kit::test]
async fn ask_for_a_missing_thread_is_not_found(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let error = op(
        ask_op(ThreadId("thr_missing".into()), "Ship it?", ask_options()),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_with_a_blank_title_or_one_option_stores_nothing(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let empty = op(ask_op(id.clone(), "  ", ask_options()), cx)
        .await
        .unwrap_err();
    assert!(
        matches!(empty, OpError::Invalid(ref message) if message.contains("title")),
        "{empty}"
    );
    let one = op(
        ask_op(
            id.clone(),
            "Ship it?",
            vec![InteractionOption {
                id: "yes".into(),
                label: "Yes".into(),
            }],
        ),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(one, OpError::Invalid(_)), "{one}");
    assert!(pending(&id, cx).await.is_empty());
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_returns_the_open_question_until_the_person_answers(cx: &mut TestAppContext) {
    let (fixture, id) = sleeping(cx).await;
    let OpResult::Text(first) = op(ask_op(id.clone(), "Ship it?", ask_options()), cx)
        .await
        .unwrap()
    else {
        panic!("ask");
    };
    assert!(first.starts_with("interaction_"), "{first}");
    let OpResult::Text(second) = op(ask_op(id.clone(), "Again?", ask_options()), cx)
        .await
        .unwrap()
    else {
        panic!("second ask");
    };
    assert_eq!(second, first);
    let open = pending(&id, cx).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id.0, first);
    assert_eq!(open[0].title, "Ship it?");
    assert_eq!(
        open[0]
            .options
            .iter()
            .map(|option| (option.id.as_str(), option.label.as_str()))
            .collect::<Vec<_>>(),
        vec![("yes", "Yes"), ("no", "No")]
    );
    assert!(cx.read(|cx| fixture.workspace.read(cx).needs_a_person(&open[0].id)));

    cx.update(|cx| Workspace::answer(&id, PendingInteractionId(first.clone()), "yes".into(), cx))
        .await
        .unwrap();
    assert!(pending(&id, cx).await.is_empty());
    let OpResult::Event(Some(event)) = op(
        Op::Thread(ThreadOp::WaitEvent {
            id: id.clone(),
            kind: "answered".into(),
            after_seq: None,
            timeout_ms: 1_000,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("answered");
    };
    assert_eq!(
        event.kind,
        EventKind::Answered {
            title: "Ship it?".into(),
            option: "yes".into(),
        }
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_busy_thread_that_goes_plain_is_done(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let dir = fixture.dir.path().join("titles");
    std::fs::create_dir_all(&dir).unwrap();
    // A busy title, then the plain title claude shows once it has answered.
    let program = script(
        &dir,
        "claude",
        "printf '\\033]2;\\342\\227\\220 work\\007'; sleep 1; printf '\\033]2;\\342\\234\\263 work\\007'; sleep 30",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut was_busy = false;
    loop {
        let (busy, _) = busy_and_unread(&id, cx);
        was_busy |= busy;
        if was_busy && !busy {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the thread never went waiting"
        );
        std::thread::yield_now();
    }
    wait_for_waiting(&id, 1, cx).await;
    assert_eq!(thread(&id, cx).await.mark, kg_domain::ThreadMark::Done);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_that_sets_no_spinner_title_is_never_busy(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let dir = fixture.dir.path().join("plain");
    std::fs::create_dir_all(&dir).unwrap();
    let program = script(
        &dir,
        "shell",
        "printf '\\033]2;plain\\007'; echo plain-ready; sleep 30",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    assert!(
        screen_with(&id, "plain-ready", cx)
            .await
            .contains("plain-ready")
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    assert_eq!(busy_and_unread(&id, cx), (false, false));
}

#[gpui_kit::test]
async fn an_unknown_answer_option_leaves_the_question_open(cx: &mut TestAppContext) {
    let (fixture, id) = sleeping(cx).await;
    let OpResult::Text(first) = op(ask_op(id.clone(), "Ship it?", ask_options()), cx)
        .await
        .unwrap()
    else {
        panic!("ask");
    };
    let error = cx
        .update(|cx| {
            Workspace::answer(&id, PendingInteractionId(first.clone()), "maybe".into(), cx)
        })
        .await
        .unwrap_err();
    assert!(
        matches!(error, OpError::Invalid(ref message) if message.contains("unknown interaction option")),
        "{error}"
    );
    assert_eq!(pending(&id, cx).await.len(), 1);
    assert!(fixture.workspace.read_with(cx, |workspace, _| {
        workspace.needs_a_person(&PendingInteractionId(first))
    }));
}

#[gpui_kit::test]
async fn answering_the_same_question_twice_is_not_found(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let OpResult::Text(first) = op(ask_op(id.clone(), "Ship it?", ask_options()), cx)
        .await
        .unwrap()
    else {
        panic!("ask");
    };
    cx.update(|cx| Workspace::answer(&id, PendingInteractionId(first.clone()), "yes".into(), cx))
        .await
        .unwrap();
    let error = cx
        .update(|cx| Workspace::answer(&id, PendingInteractionId(first), "no".into(), cx))
        .await
        .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
}
