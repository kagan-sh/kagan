use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, Reap, alive, dispatch_delete, environment_of, events, gone, kinds_of, op,
    pending_delete, personal_run, pid_of, program, screen, screen_of, screen_with, started, thread,
    type_in, wait_for_status,
};
#[cfg(unix)]
use super::{dispatch_send, job_pid, screen_lines, script, sent_text, wait_until_busy};

#[gpui_kit::test]
async fn run_puts_the_programs_output_on_the_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(windows)]
    type_in(&id, "echo kagan-out\r\n", cx).await;
    #[cfg(unix)]
    type_in(&id, "printf kagan-out\n", cx).await;
    let frame = screen_with(&id, "kagan-out", cx).await;
    assert!(frame.contains("kagan-out"), "{frame:?}");
    let grid = cx
        .update(|cx| Workspace::global(cx).read(cx).grid(&id, None))
        .unwrap();
    assert!(grid.lines.iter().any(|line| {
        line.iter()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains("kagan-out")
    }));
    let missing = ThreadId("thr_missing".into());
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).grid(&missing, None))
            .is_none()
    );
}

#[gpui_kit::test]
async fn input_and_resize_reach_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(unix)]
    {
        op(
            Op::Thread(ThreadOp::Resize {
                id: id.clone(),
                cols: 100,
                rows: 40,
                terminal: None,
            }),
            cx,
        )
        .await
        .unwrap();
        type_in(&id, "stty size\n", cx).await;
        let frame = screen_with(&id, "40 100", cx).await;
        assert!(frame.contains("40 100"), "{frame:?}");
    }
    let refused = op(
        Op::Thread(ThreadOp::Resize {
            id: id.clone(),
            cols: 0,
            rows: 40,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(refused, OpError::Invalid(_)), "{refused}");
}

#[gpui_kit::test]
async fn a_stopped_thread_refuses_input_on_every_platform(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    let error = op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            text: "x".into(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("ended"), "{error}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_stopped_thread_keeps_its_screen_and_takes_no_input(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started_thread) =
        op(fixture.run("/bin/sh -c \"printf KEPT; sleep 30\""), cx)
            .await
            .unwrap()
    else {
        panic!("thread")
    };
    let id = started_thread.id.clone();
    for _ in 0..200 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        cx.run_until_parked();
        if screen_of(&id, true, cx).await.contains("KEPT") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    assert!(screen_of(&id, true, cx).await.contains("KEPT"));
    let error = op(
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            text: "x".into(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("ended"), "{error}");
}

#[gpui_kit::test]
async fn stop_ends_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let pid = pid_of(&id, cx).unwrap();
    assert!(alive(pid));
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    assert!(pid_of(&id, cx).is_none());
    assert!(gone(pid));
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
}

#[gpui_kit::test]
async fn a_program_that_exits_leaves_the_thread_idle(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c echo done"
    } else {
        "/bin/sh -c \"printf done\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    for _ in 0..200 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if thread(&started.id, cx).await.status == ThreadStatus::Idle {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Idle);
    assert!(pid_of(&started.id, cx).is_none());
    let kinds: Vec<EventKind> = events(&started.id, cx)
        .await
        .into_iter()
        .map(|event| event.kind)
        .collect();
    assert!(
        kinds.windows(2).any(|pair| {
            matches!(
                pair[0],
                EventKind::Exited {
                    code: Some(0),
                    signal: None
                }
            ) && matches!(pair[1], EventKind::Idle)
        }),
        "{kinds:?}"
    );
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, EventKind::Failed { .. } | EventKind::TurnEnded { .. }))
    );
}

#[gpui_kit::test]
async fn a_program_that_exits_with_seven_fails_and_asks_for_attention(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = if cfg!(windows) {
        "cmd.exe /c \"echo REVIEW_FAILURE & exit 7\""
    } else {
        "/bin/sh -c \"echo REVIEW_FAILURE; exit 7\""
    };
    let OpResult::Thread(started) = op(fixture.run(line), cx).await.unwrap() else {
        panic!("thread")
    };
    let created = started.latest_attention_at;
    for _ in 0..200 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if thread(&started.id, cx).await.status == ThreadStatus::Error {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let failed = thread(&started.id, cx).await;
    assert_eq!(failed.status, ThreadStatus::Error);
    assert!(failed.latest_attention_at.0 > created.0, "{failed:?}");
    let kinds: Vec<EventKind> = events(&started.id, cx)
        .await
        .into_iter()
        .map(|event| event.kind)
        .collect();
    assert!(kinds.contains(&EventKind::Exited {
        code: Some(7),
        signal: None,
    }));
    assert!(kinds.iter().any(|kind| matches!(
        kind,
        EventKind::Failed { error: Some(text) } if text == "the program exited with code 7"
    )));
    assert!(!kinds.iter().any(|kind| matches!(kind, EventKind::Idle)));
    assert!(pid_of(&started.id, cx).is_none());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn scrollback_lines_are_a_tail_of_the_transcript(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "rows",
        "i=0\nwhile [ $i -lt 30 ]; do echo row-$i; i=$((i+1)); done\nexit 0\n",
    );
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let full = screen_lines(&started.id, true, None, cx).await.unwrap();
    let rows: Vec<_> = full.lines().collect();
    assert!(rows.len() > 20, "{full:?}");
    let tail = screen_lines(&started.id, true, Some(20), cx).await.unwrap();
    assert_eq!(tail.lines().collect::<Vec<_>>(), rows[rows.len() - 20..]);
    assert_eq!(
        screen_lines(&started.id, true, Some(10_000), cx)
            .await
            .unwrap(),
        full
    );
    let missing = screen_lines(&started.id, false, Some(20), cx)
        .await
        .unwrap_err();
    assert!(
        missing.to_string().contains("--lines needs --scrollback"),
        "{missing}"
    );
    for count in [0, 10_001] {
        let err = screen_lines(&started.id, true, Some(count), cx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("1 to 10000"), "{err}");
    }
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_paste_of_quotes_and_backticks_reaches_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/cat", cx).await;
    let text = "say \"hi\" `date`\n";
    op(
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: Some(text.into()),
            file: None,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
    let frame = screen_with(&id, "`date`", cx).await;
    assert!(frame.contains("say \"hi\""), "{frame:?}");
    let refused = op(
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: Some("NOPE".into()),
            file: Some("note.txt".into()),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(refused.to_string().contains("not both"), "{refused}");
    for _ in 0..20 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let after = screen(&id, cx).await.unwrap();
    assert!(!after.contains("NOPE"), "{after:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn paste_into_an_ended_program_sends_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf done\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let before = screen_of(&started.id, false, cx).await;
    let err = op(
        Op::Thread(ThreadOp::Paste {
            id: started.id.clone(),
            text: Some("NOPE".into()),
            file: None,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("the program has ended"), "{err}");
    let after = screen_of(&started.id, false, cx).await;
    assert_eq!(after, before);
    assert!(!after.contains("NOPE"), "{after:?}");
}

#[gpui_kit::test]
async fn send_refuses_empty_and_oversized_text(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let id = ThreadId("thread".into());
    let empty = op(
        Op::Thread(ThreadOp::Send {
            id: id.clone(),
            text: String::new(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(empty.to_string().contains("send needs text"), "{empty}");
    let over = op(
        Op::Thread(ThreadOp::Send {
            id,
            text: "x".repeat(1024 * 1024 + 1),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(over.to_string().contains("1 MiB"), "{over}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn send_submits_a_line_and_records_the_user_message(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "echo-line",
        "while IFS= read -r line; do printf 'got:%s\\n' \"$line\"; done",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    op(
        Op::Thread(ThreadOp::Send {
            id: id.clone(),
            text: "hello\u{1}".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let frame = screen_with(&id, "got:hello", cx).await;
    assert!(frame.contains("got:hello"), "{frame}");
    assert_eq!(sent_text(&events(&id, cx).await), ["hello"]);
    let before = events(&id, cx).await.len();
    op(
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: Some("note".into()),
            file: None,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
    let after = events(&id, cx).await;
    assert_eq!(after.len(), before);
    assert_eq!(sent_text(&after), ["hello"]);
    let bare = op(
        Op::Thread(ThreadOp::Send {
            id: id.clone(),
            text: "\u{1}".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(bare.to_string().contains("send needs text"), "{bare}");
    for text in ["\r", "\n", "\t", "\r\n\t"] {
        let before = events(&id, cx).await.len();
        let error = op(
            Op::Thread(ThreadOp::Send {
                id: id.clone(),
                text: text.into(),
            }),
            cx,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("send needs text"),
            "{text:?}: {error}"
        );
        assert_eq!(events(&id, cx).await.len(), before);
        assert_eq!(sent_text(&events(&id, cx).await), ["hello"]);
    }
}

#[cfg(unix)]
#[gpui_kit::test]
async fn send_into_an_ended_program_records_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf done\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let before = events(&started.id, cx).await.len();
    let err = op(
        Op::Thread(ThreadOp::Send {
            id: started.id.clone(),
            text: "NOPE".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("the program has ended"), "{err}");
    assert_eq!(events(&started.id, cx).await.len(), before);
    assert!(!screen_of(&started.id, false, cx).await.contains("NOPE"));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn send_with_embedded_newlines_follows_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "echo-lines",
        "while IFS= read -r line; do printf 'got:%s\\n' \"$line\"; done",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    op(
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: Some("draft".into()),
            file: None,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
    let _ = screen_with(&id, "draft", cx).await;
    op(
        Op::Thread(ThreadOp::Send {
            id: id.clone(),
            text: "first\nsecond".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let frame = screen_with(&id, "got:second", cx).await;
    assert!(frame.contains("got:draftfirst"), "{frame}");
    assert!(frame.contains("got:second"), "{frame}");
    assert_eq!(sent_text(&events(&id, cx).await), ["first\nsecond"]);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn send_uses_bracketed_paste_bytes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let got = fixture.dir.path().join("got");
    let got_path = got.display().to_string();
    let program = script(
        fixture.dir.path(),
        "bracket-send",
        &format!(
            "stty raw -echo\n\
             printf '\\033[?2004h'\n\
             printf 'ready\\n'\n\
             dd bs=1 count=25 2>/dev/null > '{got_path}'\n\
             printf 'done\\n'\n\
             sleep 60\n"
        ),
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    let _ = screen_with(&id, "ready", cx).await;
    op(
        Op::Thread(ThreadOp::Send {
            id: id.clone(),
            text: "first\nsecond".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let _ = screen_with(&id, "done", cx).await;
    let received = std::fs::read(&got).unwrap();
    assert_eq!(received, b"\x1b[200~first\nsecond\x1b[201~\r");
    assert_eq!(sent_text(&events(&id, cx).await), ["first\nsecond"]);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn send_while_busy_records_one_user_message(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "busy-send",
        "spin=$(printf '\\342\\240\\213')\n\
         printf '\\033]0;%s working\\007' \"$spin\"\n\
         printf 'ready\\n'\n\
         while IFS= read -r line; do printf 'got:%s\\n' \"$line\"; done\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    let _ = screen_with(&id, "ready", cx).await;
    wait_until_busy(&fixture, &id, cx);
    op(
        Op::Thread(ThreadOp::Send {
            id: id.clone(),
            text: "hello".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let frame = screen_with(&id, "got:hello", cx).await;
    assert!(frame.contains("got:hello"), "{frame}");
    assert_eq!(sent_text(&events(&id, cx).await), ["hello"]);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn pending_sends_do_not_block_the_workers(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    // Raw mode with no reader fills the pty write buffer so checked writes stay pending.
    let program = script(
        fixture.dir.path(),
        "hold-send",
        "stty raw -echo\nprintf 'ready\\n'\nsleep 5\n",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    let _ = screen_with(&id, "ready", cx).await;
    let chunk = "x".repeat(64 * 1024);
    let pending: Vec<_> = (0..4)
        .map(|n| dispatch_send(&id, &format!("{n}-{chunk}"), cx))
        .collect();
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let before = events(&id, cx).await;
    assert!(
        sent_text(&before).is_empty(),
        "pending writes must not record a message yet: {before:?}"
    );
    op(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx)
        .await
        .unwrap();
    let listed = events(&id, cx).await;
    assert!(
        listed
            .iter()
            .any(|event| matches!(event.kind, EventKind::Started)),
        "{listed:?}"
    );
    assert!(
        sent_text(&listed).is_empty(),
        "MarkRead must run while sends are still pending: {listed:?}"
    );
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    for task in pending {
        let _ = task.await;
    }
    let after = events(&id, cx).await;
    assert!(
        sent_text(&after).is_empty(),
        "stopped pending sends must record nothing: {after:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_fast_failure_keeps_its_last_lines_when_nothing_showed_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "noisy",
        "i=0\nwhile [ $i -lt 3000 ]; do echo line $i >&2; i=$((i+1)); done\necho LAST\nexit 7",
    );
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Error, cx).await;
    let kept = screen_of(&started.id, true, cx).await;
    assert!(kept.contains("line 2999"), "{kept}");
    assert!(kept.contains("LAST"), "{kept}");
    assert!(fixture.workspace.read_with(cx, |workspace, _| {
        workspace.grid(&started.id, None).is_some()
    }));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_that_ends_leaves_no_child_behind(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "spawner",
        "sleep 300 &\necho CHILD $!\nexit 0",
    );
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let screen = screen_of(&started.id, true, cx).await;
    let child: u32 = screen
        .lines()
        .find_map(|line| line.strip_prefix("CHILD "))
        .and_then(|pid| pid.trim().parse().ok())
        .unwrap_or_else(|| panic!("no child pid in {screen}"));
    let _reap = Reap(child);
    assert!(gone(child), "the child outlived its program");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn stop_on_an_ended_thread_keeps_its_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf KEEP; exit 0\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(screen_of(&started.id, false, cx).await.contains("KEEP"));
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Idle);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn input_to_an_ended_program_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf done; exit 0\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let error = op(
        Op::Thread(ThreadOp::Input {
            id: started.id.clone(),
            text: "x".into(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("ended"), "{error}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn input_past_the_queue_budget_is_refused_as_full(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"stty raw -echo; sleep 30\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let text = "a".repeat(1 << 20);
    for _ in 0..4 {
        op(
            Op::Thread(ThreadOp::Input {
                id: started.id.clone(),
                text: text.clone(),
                terminal: None,
            }),
            cx,
        )
        .await
        .expect("input inside the budget is accepted");
    }
    let refused = op(
        Op::Thread(ThreadOp::Input {
            id: started.id.clone(),
            text: "x".into(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(refused, OpError::Full(_)), "{refused}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn input_over_the_text_cap_is_invalid_even_to_an_idle_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"stty raw -echo; sleep 30\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let refused = op(
        Op::Thread(ThreadOp::Input {
            id: started.id,
            text: "a".repeat(5 << 20),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(refused, OpError::Invalid(_)), "{refused}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn deleting_an_ended_thread_releases_its_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf done; exit 0\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let deleting = dispatch_delete(&started.id, cx);
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(fixture.workspace.read_with(cx, |workspace, _| {
        workspace.grid(&started.id, None).is_none()
    }));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_archived_ended_thread_releases_its_screen_after_the_grace(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf done; exit 0\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(fixture.workspace.read_with(cx, |workspace, _| {
        workspace.grid(&started.id, None).is_some()
    }));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    assert!(fixture.workspace.read_with(cx, |workspace, _| {
        workspace.grid(&started.id, None).is_none()
    }));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn stop_of_a_running_program_keeps_its_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"printf MARKER; sleep 30\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    for _ in 0..200 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        cx.run_until_parked();
        if screen_of(&started.id, true, cx).await.contains("MARKER") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Idle);
    assert!(screen_of(&started.id, true, cx).await.contains("MARKER"));
    let kinds = kinds_of(&started.id, cx).await;
    assert!(kinds.contains(&EventKind::Stopped));
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, EventKind::Exited { .. } | EventKind::Failed { .. }))
    );
    assert!(fixture.workspace.read_with(cx, |workspace, _| {
        workspace.grid(&started.id, None).is_some()
    }));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_that_exits_inside_an_unfinished_update_keeps_its_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(
        fixture.dir.path(),
        "held",
        "printf '\\033[?2026h'\nprintf HELD\nexit 0",
    );
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    assert!(screen_of(&started.id, true, cx).await.contains("HELD"));
    assert!(screen_of(&started.id, false, cx).await.contains("HELD"));
}

#[gpui_kit::test]
async fn a_screen_change_is_announced_to_windows(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let workspace = fixture.workspace.clone();
    let _subscription = cx.update({
        let seen = seen.clone();
        move |cx| {
            cx.subscribe(&workspace, move |_, event: &crate::WorkspaceEvent, _| {
                if let crate::WorkspaceEvent::Screen(id, None) = event {
                    seen.borrow_mut().push(id.clone());
                }
            })
        }
    });
    let (id, _reap) = started(&fixture, program(), cx).await;
    #[cfg(windows)]
    type_in(&id, "echo kagan-out\r\n", cx).await;
    #[cfg(unix)]
    type_in(&id, "printf kagan-out\n", cx).await;
    screen_with(&id, "kagan-out", cx).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert!(seen.borrow().contains(&id), "{:?}", seen.borrow());
    let before = seen.borrow().len();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(
        seen.borrow().len(),
        before,
        "an unchanged screen was announced"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_cursor_query_is_answered_with_no_window_open(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let line = script(
        fixture.dir.path(),
        "ask.sh",
        "stty raw -echo\nprintf 'ab\\033[6n'\nreply=$(dd bs=1 count=6 2>/dev/null | od -An -c | tr -d ' \\n')\nprintf 'got:%s:end' \"$reply\"\nsleep 30",
    );
    let (id, _reap) = started(&fixture, &line, cx).await;
    let frame = screen_with(&id, ":end", cx).await;
    assert!(frame.contains("got:033[1;3R:end"), "{frame:?}");
}

#[gpui_kit::test]
async fn quitting_ends_every_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (first, _a) = started(&fixture, program(), cx).await;
    let (second, _b) = started(&fixture, program(), cx).await;
    let pids = [pid_of(&first, cx).unwrap(), pid_of(&second, cx).unwrap()];
    cx.update(gpui_kit::App::shutdown);
    for pid in pids {
        assert!(gone(pid), "{pid} survived quit");
    }
}

#[gpui_kit::test]
async fn deleting_or_archiving_after_stop_needs_no_terminal(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    op(Op::Thread(ThreadOp::Archive { id: id.clone() }), cx)
        .await
        .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    assert!(pid_of(&id, cx).is_none());
}

#[gpui_kit::test]
async fn deleting_a_thread_ends_its_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let pid = pid_of(&id, cx).unwrap();
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: id.clone(),
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
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
    deleting.await.unwrap();
    assert!(pid_of(&id, cx).is_none());
    assert!(gone(pid), "delete left the program running");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn stop_ends_the_background_jobs_a_shell_started(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    type_in(&id, "sleep 1000 &\n", cx).await;
    type_in(&id, "echo job:$! end\n", cx).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let job = loop {
        let screen = screen(&id, cx).await.unwrap();
        if let Some(job) = job_pid(&screen) {
            break job;
        }
        assert!(std::time::Instant::now() < deadline, "no job: {screen:?}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let _reap_job = Reap(job);
    assert!(alive(job));
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while alive(job) {
        assert!(
            std::time::Instant::now() < deadline,
            "job {job} survived Stop"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[gpui_kit::test]
async fn the_watch_runs_only_while_a_program_does(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let watching =
        |cx: &mut TestAppContext| cx.update(|cx| Workspace::global(cx).read(cx).watching);
    assert!(!watching(cx));
    let (first, _a) = started(&fixture, program(), cx).await;
    assert!(watching(cx));
    op(Op::Thread(ThreadOp::Stop { id: first.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&first, ThreadStatus::Idle, cx).await;
    for _ in 0..200 {
        if !watching(cx) {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!watching(cx));
    let (second, _b) = started(&fixture, program(), cx).await;
    assert!(watching(cx));
    op(Op::Thread(ThreadOp::Stop { id: second }), cx)
        .await
        .unwrap();
}

#[gpui_kit::test]
async fn a_terminal_cannot_be_resized_past_its_limits(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    for (cols, rows) in [(65535, 65535), (1001, 24), (80, 501), (1, 24), (80, 0)] {
        let refused = op(
            Op::Thread(ThreadOp::Resize {
                id: id.clone(),
                cols,
                rows,
                terminal: None,
            }),
            cx,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(refused, OpError::Invalid(_)),
            "{cols}x{rows}: {refused}"
        );
    }
    let frame = screen(&id, cx).await.unwrap();
    assert!(
        frame.contains("\x1b[24;1H") && !frame.contains("\x1b[25;1H"),
        "{frame:?}"
    );
    op(
        Op::Thread(ThreadOp::Resize {
            id,
            cols: 1000,
            rows: 500,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_selected_word_copies_its_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    type_in(&id, "printf 'COPY''-ME\\n'\n", cx).await;
    let frame = screen_with(&id, "COPY-ME", cx).await;
    let at = frame.find("COPY-ME").unwrap();
    let before = &frame[..at];
    let place = &before[before.rfind("\u{1b}[").unwrap() + 2..];
    let line: u16 = place.split(';').next().unwrap().parse().unwrap();
    let row = line - 1;
    op(
        Op::Thread(ThreadOp::Select {
            id: id.clone(),
            terminal: None,
            mode: SelectMode::Word,
            column: 0,
            row,
            right: false,
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Text(text) = op(
        Op::Thread(ThreadOp::Copy {
            id: id.clone(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("text")
    };
    assert!(text.contains("COPY-ME"), "{text:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_report_reaches_a_running_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let answered = op(
        Op::Thread(ThreadOp::Report {
            id: id.clone(),
            terminal: None,
            bytes: b"\x1b[0n".to_vec(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(matches!(answered, OpResult::Done), "{answered:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn resize_and_report_to_a_stopped_program_do_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    let resized = op(
        Op::Thread(ThreadOp::Resize {
            id: id.clone(),
            cols: 80,
            rows: 24,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(matches!(resized, OpResult::Done), "{resized:?}");
    let reported = op(
        Op::Thread(ThreadOp::Report {
            id: id.clone(),
            terminal: None,
            bytes: b"\x1b[0n".to_vec(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(matches!(reported, OpResult::Done), "{reported:?}");
}

#[gpui_kit::test]
async fn send_accepts_text_up_to_the_cap(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let error = op(
        Op::Thread(ThreadOp::Send {
            id: ThreadId("thread".into()),
            text: "x".repeat(kg_domain::TEXT_MAX),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
}

// Typed text with no Enter must come back as an echo. Untitled Personal threads on Windows failed
// this in kg-ui, so the probe is a Personal thread titled like its program, and the control has
// another title. A miss reports what the screen showed, so "slow" and "never" can be told apart.
async fn echo_after_typing(title: &str, cx: &mut TestAppContext) -> Option<String> {
    let fixture = Fixture::personal(cx).await;
    let OpResult::Thread(started) =
        op(personal_run(&fixture, title, EnvironmentSpec::Personal), cx)
            .await
            .unwrap()
    else {
        panic!("thread")
    };
    let id = started.id.clone();
    let begin = std::time::Instant::now();
    type_in(&id, "typed-probe", cx).await;
    loop {
        let frame = screen(&id, cx).await.unwrap();
        if frame.contains("typed-probe") {
            return None;
        }
        if begin.elapsed() > std::time::Duration::from_secs(30) {
            let status = thread(&id, cx).await.status;
            let folder = environment_of(started.environment_id.as_ref().unwrap(), cx)
                .await
                .path;
            return Some(format!(
                "typed text never echoed after {:?}; status {status:?}; folder {}; screen:\n{frame}",
                begin.elapsed(),
                folder.display()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(windows)]
const PROGRAM_STEM: &str = "cmd";
#[cfg(not(windows))]
const PROGRAM_STEM: &str = "sh";

#[gpui_kit::test]
async fn the_probe_titled_like_its_program_echoes_typed_text(cx: &mut TestAppContext) {
    let miss = echo_after_typing(PROGRAM_STEM, cx).await;
    assert!(miss.is_none(), "{}", miss.unwrap_or_default());
}

#[gpui_kit::test]
async fn the_control_titled_notes_echoes_typed_text(cx: &mut TestAppContext) {
    let miss = echo_after_typing("notes", cx).await;
    assert!(miss.is_none(), "{}", miss.unwrap_or_default());
}
