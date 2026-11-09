use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, empty_repo, environment_branch, environment_of, events, kinds_of, op, pid_of,
    plant_old_turn, program, socket_rename, started, thread, wait_for_status, worktree_run,
};
#[cfg(unix)]
use super::{script, stop_archive_and_settle};

#[gpui_kit::test]
async fn a_bad_program_does_not_start_a_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let quote = op(fixture.run("\"kg-no-such"), cx).await.unwrap_err();
    assert!(
        matches!(quote, OpError::Invalid(ref message) if message.contains("unfinished quote")),
        "{quote}"
    );
    let missing = op(fixture.run("kg-no-such-program"), cx).await.unwrap_err();
    assert!(
        matches!(missing, OpError::Invalid(ref message) if message.contains("was not found")),
        "{missing}"
    );
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
        panic!()
    };
    assert!(threads.is_empty());
}

#[gpui_kit::test]
async fn run_starts_a_thread_and_stop_leaves_it_idle(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    assert_eq!(started.status, ThreadStatus::Active);
    assert_eq!(started.provider, program());
    assert!(
        events(&started.id, cx)
            .await
            .iter()
            .any(|event| event.kind == EventKind::Started)
    );
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
}

#[gpui_kit::test]
async fn an_explicit_run_title_is_cleaned_and_a_blank_one_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let titled = |title: &str| {
        let Op::Thread(ThreadOp::Run(mut run)) = fixture.run(program()) else {
            panic!()
        };
        run.title = Some(title.into());
        Op::Thread(ThreadOp::Run(run))
    };
    let refused = op(titled("\n\t\x1b"), cx).await.unwrap_err();
    assert_eq!(refused.to_string(), "a thread needs a title");
    let OpResult::Thread(cleaned) = op(titled("  Ship\nit\t"), cx).await.unwrap() else {
        panic!()
    };
    assert_eq!(cleaned.title, "Shipit");
}

#[gpui_kit::test]
async fn run_starts_a_thread_in_a_worktree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    empty_repo(fixture.dir.path());
    let OpResult::Thread(started) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: Some("notes".into()),
            environment: EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None,
            },
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!()
    };
    assert_eq!(started.title, "notes");
    let OpResult::Environment(folder) = op(
        Op::Environment(EnvironmentOp::Get {
            id: started.environment_id.clone().unwrap(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("environment")
    };
    assert_eq!(folder.provider, EnvironmentProvider::Worktree);
    assert_eq!(folder.branch.as_deref(), Some("notes"));
    assert_eq!(
        folder.path,
        dunce::canonicalize(fixture.dir.path().join("worktrees/project/notes")).unwrap()
    );
}

#[gpui_kit::test]
async fn a_socket_rename_locks_once_and_the_window_can_rename_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    empty_repo(fixture.dir.path());
    let OpResult::Thread(started) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: None,
            environment: EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None,
            },
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!("run")
    };
    assert!(!started.title_locked);
    let branch = environment_branch(started.environment_id.as_ref().unwrap(), cx).await;
    let OpResult::Thread(named) = socket_rename(&started.id, "Named", cx).await.unwrap() else {
        panic!("socket rename")
    };
    assert_eq!(named.title, "Named");
    assert!(named.title_locked);
    let kept = socket_rename(&started.id, "Other", cx).await.unwrap_err();
    assert_eq!(
        kept.to_string(),
        "the title is kept; rename it in the window"
    );
    let OpResult::Thread(again) = op(
        Op::Thread(ThreadOp::Get {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    assert_eq!(again.title, "Named");
    let OpResult::Thread(window) = op(
        Op::Thread(ThreadOp::Rename {
            id: started.id.clone(),
            title: "Win\ndow\t\x1b".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("window rename")
    };
    assert_eq!(window.title, "Window");
    assert!(window.title_locked);
    let blank = op(
        Op::Thread(ThreadOp::Rename {
            id: started.id.clone(),
            title: "\n\t\x1b".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert_eq!(blank.to_string(), "a thread needs a title");
    let kept = socket_rename(&started.id, "Agent", cx).await.unwrap_err();
    assert_eq!(
        kept.to_string(),
        "the title is kept; rename it in the window"
    );
    let OpResult::Thread(held) = op(
        Op::Thread(ThreadOp::Get {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    assert_eq!(held.title, "Window");
    assert_eq!(
        environment_branch(started.environment_id.as_ref().unwrap(), cx).await,
        branch
    );
}

#[gpui_kit::test]
async fn a_title_set_at_run_rejects_a_socket_rename(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    empty_repo(fixture.dir.path());
    let OpResult::Thread(started) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: Some("Notes".into()),
            environment: EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None,
            },
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!("run")
    };
    assert!(started.title_locked);
    assert_eq!(
        environment_branch(started.environment_id.as_ref().unwrap(), cx)
            .await
            .as_deref(),
        Some("notes")
    );
    let kept = socket_rename(&started.id, "Other", cx).await.unwrap_err();
    assert_eq!(
        kept.to_string(),
        "the title is kept; rename it in the window"
    );
    let OpResult::Thread(held) = op(
        Op::Thread(ThreadOp::Get {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    assert_eq!(held.title, "Notes");
    assert_eq!(
        environment_branch(started.environment_id.as_ref().unwrap(), cx)
            .await
            .as_deref(),
        Some("notes")
    );
}

#[gpui_kit::test]
async fn rename_stores_a_title_and_leaves_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let before = events(&started.id, cx).await.len();
    let OpResult::Thread(renamed) = op(
        Op::Thread(ThreadOp::Rename {
            id: started.id.clone(),
            title: "  Notes  ".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("rename")
    };
    assert_eq!(renamed.title, "Notes");
    assert!(renamed.title_locked);
    assert!(!started.title_locked);
    assert_eq!(renamed.provider, started.provider);
    assert_eq!(renamed.environment_id, started.environment_id);
    assert_eq!(events(&started.id, cx).await.len(), before);
    let blank = op(
        Op::Thread(ThreadOp::Rename {
            id: started.id.clone(),
            title: "   ".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(blank, OpError::Invalid(_)), "{blank}");
    assert_eq!(thread(&started.id, cx).await.title, "Notes");
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Thread(archived) = op(
        Op::Thread(ThreadOp::Rename {
            id: started.id.clone(),
            title: "Kept".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("archived rename")
    };
    assert_eq!(archived.title, "Kept");
    assert!(archived.archived_at.is_some());
    let missing = op(
        Op::Thread(ThreadOp::Rename {
            id: ThreadId("thr_missing".into()),
            title: "Gone".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
}

#[gpui_kit::test]
async fn the_sidebar_omits_an_archived_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Sidebar(sidebar) = op(Op::Sidebar, cx).await.unwrap() else {
        panic!("sidebar")
    };
    assert!(sidebar.threads.iter().all(|thread| thread.id != started.id));
    let OpResult::Threads(archived) = op(
        Op::Thread(ThreadOp::List {
            project_id: None,
            archived: Some(true),
            limit: None,
            before: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("list")
    };
    assert!(archived.iter().any(|thread| thread.id == started.id));
    let shown = cx.read(|cx| fixture.workspace.read(cx).sidebar(cx));
    assert!(shown.threads.iter().all(|thread| thread.id != started.id));
}

#[gpui_kit::test]
async fn pin_and_archive_keep_the_saved_history(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    {
        let mut store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
        store
            .append_events(
                &started.id,
                Timestamp(1),
                [EventKind::Message {
                    role: Role::User,
                    text: "kept".into(),
                    message_id: None,
                    attachments: Vec::new(),
                }],
            )
            .unwrap();
    }
    op(
        Op::Thread(ThreadOp::SetPinned {
            id: started.id.clone(),
            pinned: true,
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(thread(&started.id, cx).await.pinned);
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(thread(&started.id, cx).await.archived_at.is_some());
    assert!(
        events(&started.id, cx).await.iter().any(|event| {
            matches!(&event.kind, EventKind::Message { text, .. } if text == "kept")
        })
    );
}

#[gpui_kit::test]
async fn deleting_a_thread_waits_for_a_person(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(caller) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let OpResult::Thread(victim) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: Some("victim".into()),
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!()
    };
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: victim.id.clone(),
                approval_thread_id: Some(caller.id.clone()),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let OpResult::Interactions(pending) = op(
        Op::Thread(ThreadOp::Interactions {
            id: caller.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("interactions")
    };
    let interaction = pending
        .into_iter()
        .find(|interaction| interaction.title.contains("victim"))
        .unwrap();
    cx.update(|cx| Workspace::answer(&caller.id, interaction.id, "decline".into(), cx))
        .await
        .unwrap();
    let error = deleting.await.unwrap_err();
    assert!(error.to_string().contains("declined"), "{error}");
    assert!(
        op(
            Op::Thread(ThreadOp::Get {
                id: victim.id.clone()
            }),
            cx
        )
        .await
        .is_ok()
    );

    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: victim.id.clone(),
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
    assert!(
        op(Op::Thread(ThreadOp::Get { id: victim.id }), cx)
            .await
            .is_err()
    );
}

#[gpui_kit::test]
async fn a_stopped_command_takes_its_delete_question_off_the_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(caller) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let OpResult::Thread(victim) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let pending = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            Workspace::dispatch(
                Op::Thread(ThreadOp::Interactions {
                    id: caller.id.clone(),
                }),
                cx,
            )
        })
    };
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(5));
    let (present, gone) = async_channel::bounded::<()>(1);
    let deleting = cx.update(|cx| {
        Workspace::dispatch_for(
            Op::Thread(ThreadOp::Delete {
                id: victim.id.clone(),
                approval_thread_id: Some(caller.id.clone()),
            }),
            Some(crate::Requester::new(gone)),
            cx,
        )
    });
    cx.run_until_parked();
    let OpResult::Interactions(asked) = pending(cx).await.unwrap() else {
        panic!("interactions")
    };
    assert_eq!(asked.len(), 1);
    assert!(thread(&caller.id, cx).await.unread());
    drop(present);
    cx.run_until_parked();
    let OpResult::Interactions(asked) = pending(cx).await.unwrap() else {
        panic!("interactions")
    };
    assert!(
        asked.is_empty(),
        "the card stayed after its command stopped"
    );
    assert!(
        !thread(&caller.id, cx).await.unread(),
        "the needs-you mark stayed after its question went"
    );
    assert!(deleting.await.is_err());
    assert!(
        op(Op::Thread(ThreadOp::Get { id: victim.id }), cx)
            .await
            .is_ok()
    );
}

#[gpui_kit::test]
async fn a_saved_thread_opens_after_a_restart(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(parent) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let OpResult::Thread(child) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: Some("child".into()),
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: Some(parent.id.clone()),
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!()
    };
    let Fixture { dir, workspace, .. } = fixture;
    cx.update(gpui_kit::App::remove_global::<crate::workspace::WorkspaceGlobal>);
    drop(workspace);
    cx.run_until_parked();
    let db = dir.path().join("kagan.sqlite3");
    plant_old_turn(&db, &child.id);
    let _workspace = cx
        .update(|cx| {
            Workspace::open(
                dir.path().into(),
                std::env::current_exe().unwrap(),
                None,
                cx,
            )
        })
        .await
        .unwrap();
    assert!(events(&child.id, cx).await.iter().any(|event| {
        matches!(&event.kind, EventKind::Message { text, .. } if text == "still here")
    }));
    assert!(
        events(&child.id, cx)
            .await
            .iter()
            .any(|event| { matches!(&event.kind, EventKind::Tool { id, .. } if id == "tool-1") })
    );
    assert!(
        events(&child.id, cx)
            .await
            .iter()
            .any(|event| { matches!(event.kind, EventKind::Rewound { .. }) })
    );
    assert!(
        !events(&child.id, cx).await.iter().any(|event| {
            matches!(&event.kind, EventKind::Failed { error: Some(text) } if text == "kagan quit while the program was running")
        }),
        "a running thread was marked failed instead of resumed"
    );
    assert_eq!(thread(&child.id, cx).await.status, ThreadStatus::Active);
    let OpResult::Interactions(pending) = op(
        Op::Thread(ThreadOp::Interactions {
            id: child.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("interactions")
    };
    assert!(pending.is_empty());
    assert!(
        !events(&parent.id, cx).await.iter().any(|event| {
            matches!(&event.kind, EventKind::Notice { text, .. } if text.contains("kagan quit while the program was running"))
        }),
        "a resumed child was reported as failed"
    );
}

#[gpui_kit::test]
async fn stop_of_a_running_program_is_recorded_as_stopped(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let before = thread(&started.id, cx).await.latest_attention_at;
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let stopped = thread(&started.id, cx).await;
    assert_eq!(stopped.status, ThreadStatus::Idle);
    assert_eq!(stopped.latest_attention_at, before);
    let kinds: Vec<EventKind> = events(&started.id, cx)
        .await
        .into_iter()
        .map(|event| event.kind)
        .collect();
    assert!(kinds.contains(&EventKind::Stopped));
    assert!(!kinds.iter().any(|kind| matches!(
        kind,
        EventKind::Failed { .. } | EventKind::Exited { .. } | EventKind::TurnEnded { .. }
    )));
}

#[gpui_kit::test]
async fn an_archived_parent_cannot_own_a_new_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(parent) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("parent")
    };
    op(
        Op::Thread(ThreadOp::Archive {
            id: parent.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let error = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: None,
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: Some(parent.id),
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("unarchived"), "{error}");
}

#[gpui_kit::test]
async fn sweep_stops_a_running_thread_after_archive_grace(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    empty_repo(fixture.dir.path());
    let OpResult::Thread(started) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: Some("notes".into()),
            environment: EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None,
            },
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
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
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Idle);
    let OpResult::Environment(folder) = op(
        Op::Environment(EnvironmentOp::Get {
            id: started.environment_id.clone().unwrap(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("environment")
    };
    assert!(folder.retire_at.is_some(), "{folder:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_new_worktree_survives_cleanup_of_a_retired_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    empty_repo(fixture.dir.path());
    let OpResult::Thread(first) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let old = stop_archive_and_settle(&first.id, &first.environment_id.unwrap(), cx).await;
    assert_eq!(old.status, EnvironmentStatus::Destroyed);
    assert!(!old.path.exists());
    let OpResult::Thread(second) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let new = environment_of(&second.environment_id.unwrap(), cx).await;
    assert!(new.path.exists());
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        cx.run_until_parked();
    }
    assert!(
        new.path.exists(),
        "new worktree was removed: {}",
        new.path.display()
    );
    assert_ne!(new.path, old.path);
    drop(fixture);
}

#[gpui_kit::test]
async fn a_restart_resumes_a_program_that_was_running(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(pid_of(&id, cx).is_some());
    assert!(
        !kinds_of(&id, cx).await.iter().any(|kind| matches!(
            kind,
            EventKind::Failed { error: Some(text) } if text == "kagan quit while the program was running"
        ))
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn a_read_running_thread_stays_read_when_a_restart_resumes_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    op(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx)
        .await
        .unwrap();
    let read = thread(&id, cx).await;
    let read_at = read.last_read_at.expect("read");
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    let recovered = thread(&id, cx).await;
    assert_eq!(recovered.status, ThreadStatus::Active);
    assert!(!recovered.unread(), "{recovered:?}");
    assert!(
        recovered.latest_attention_at.0 <= read_at.0,
        "{recovered:?}"
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn a_restart_adds_no_turn_to_a_terminal_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    let kinds = kinds_of(&id, cx).await;
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, EventKind::TurnStarted | EventKind::TurnEnded { .. }))
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn a_restart_recovers_a_thread_left_stopping_by_a_quit(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let fixture = fixture
        .restart(cx, |store| {
            store
                .set_thread_status(&id, ThreadStatus::Stopping, Timestamp(1))
                .unwrap();
        })
        .await;
    cx.run_until_parked();
    let kinds = kinds_of(&id, cx).await;
    let error = "kagan quit while the program was running";
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            EventKind::Failed { error: Some(text) } if text == error
        )),
        "{kinds:?}"
    );
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id))
            .is_some_and(|line| line.starts_with("Ended when kagan quit"))
    );
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_ended_by_a_signal_fails_the_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"kill -9 $$\""), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Error, cx).await;
    let kinds = kinds_of(&started.id, cx).await;
    assert!(
        kinds.contains(&EventKind::Exited {
            code: None,
            signal: Some(9),
        }),
        "{kinds:?}"
    );
    assert!(kinds.iter().any(|kind| matches!(
        kind,
        EventKind::Failed { error: Some(text) } if text == "the program was ended by signal 9"
    )));
    assert!(!kinds.iter().any(|kind| matches!(kind, EventKind::Idle)));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_that_closes_its_pty_but_keeps_running_stays_active(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = script(fixture.dir.path(), "quiet", "exec >&- 2>&- <&-\nsleep 30");
    let OpResult::Thread(started) = op(fixture.run(&program), cx).await.unwrap() else {
        panic!("thread")
    };
    for _ in 0..60 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Active);
    assert!(
        !kinds_of(&started.id, cx)
            .await
            .iter()
            .any(|kind| matches!(kind, EventKind::Exited { .. } | EventKind::Failed { .. }))
    );
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
    let kinds = kinds_of(&started.id, cx).await;
    assert!(
        kinds
            .windows(2)
            .any(|pair| matches!(pair, [EventKind::Stopped, EventKind::Idle])),
        "{kinds:?}"
    );
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, EventKind::Exited { .. }))
    );
}

#[gpui_kit::test]
async fn an_interrupted_recovery_rolls_back_and_the_next_start_retries(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let Fixture { dir, workspace, .. } = fixture;
    cx.update(gpui_kit::App::remove_global::<crate::workspace::WorkspaceGlobal>);
    drop(workspace);
    cx.run_until_parked();
    let db = dir.path().join("kagan.sqlite3");
    kg_store::Store::open(&db)
        .unwrap()
        .set_thread_status(&id, ThreadStatus::Stopping, Timestamp(1))
        .unwrap();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER abort_failed BEFORE INSERT ON events
             WHEN json_extract(NEW.kind, '$.type') = 'failed'
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let interrupted = cx
        .update(|cx| {
            Workspace::open(
                dir.path().into(),
                std::env::current_exe().unwrap(),
                None,
                cx,
            )
        })
        .await;
    assert!(interrupted.is_err());
    let check = rusqlite::Connection::open(&db).unwrap();
    let status: String = check
        .query_row(
            "SELECT status FROM threads WHERE id = ?1",
            [id.0.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "stopping");
    let failed: i64 = check
        .query_row(
            "SELECT COUNT(*) FROM events WHERE thread_id = ?1
             AND json_extract(kind, '$.type') = 'failed'",
            [id.0.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(failed, 0);
    check.execute_batch("DROP TRIGGER abort_failed").unwrap();
    drop(check);
    let _workspace = cx
        .update(|cx| {
            Workspace::open(
                dir.path().into(),
                std::env::current_exe().unwrap(),
                None,
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(kinds_of(&id, cx).await.iter().any(|kind| matches!(
        kind,
        EventKind::Failed { error: Some(text) } if text == "kagan quit while the program was running"
    )));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_runs_the_program_in_the_same_thread_and_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let before = thread(&id, cx).await;
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    let OpResult::Thread(again) = op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    assert_eq!(again.id, id);
    assert_eq!(again.environment_id, before.environment_id);
    assert_eq!(again.status, ThreadStatus::Active);
    wait_for_status(&id, ThreadStatus::Active, cx).await;
    assert!(pid_of(&id, cx).is_some());
    let starts = kinds_of(&id, cx)
        .await
        .iter()
        .filter(|kind| **kind == EventKind::Started)
        .count();
    assert_eq!(starts, 2);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_refuses_a_running_or_archived_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let running = op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap_err();
    assert!(running.to_string().contains("still running"), "{running}");
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    op(Op::Thread(ThreadOp::Archive { id: id.clone() }), cx)
        .await
        .unwrap();
    let archived = op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap_err();
    assert!(archived.to_string().contains("restore"), "{archived}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_restart_resumes_a_running_program_without_start_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(pid_of(&id, cx).is_some());
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_needs_its_folder_to_be_ready(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let environment = thread(&id, cx).await.environment_id.expect("folder");
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    let fixture = fixture
        .restart(cx, |store| {
            let mut found = store.environment(&environment).unwrap().unwrap();
            found.status = EnvironmentStatus::Error;
            store.put_environment(&found).unwrap();
        })
        .await;
    cx.run_until_parked();
    let refused = op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("not ready"), "{refused}");
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_that_vanishes_before_start_refuses_the_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    empty_repo(fixture.dir.path());
    let program = script(fixture.dir.path(), "vanishing", "sleep 30");
    // The worktree folder appears after the program resolves. Removing the
    // program then makes the start fail once the folder is committed.
    let watch = fixture.dir.path().join("worktrees");
    let gone = std::path::PathBuf::from(&program);
    std::thread::spawn(move || {
        for _ in 0..5_000 {
            if std::fs::read_dir(&watch).is_ok_and(|entries| entries.count() > 0) {
                let _ = std::fs::remove_file(&gone);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    let run = Op::Thread(ThreadOp::Run(Run {
        project_id: fixture.project.clone(),
        program,
        title: None,
        environment: EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        },
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }));
    let error = op(run, cx).await.unwrap_err();
    assert!(matches!(error, OpError::Invalid(_)), "{error}");
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
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].status, ThreadStatus::Error);
    let failed = events(&threads[0].id, cx).await;
    assert!(!failed.iter().any(|event| event.kind == EventKind::Started));
    assert!(
        failed.iter().any(|event| matches!(
            &event.kind,
            EventKind::Failed { error: Some(message) } if !message.is_empty()
        )),
        "{failed:?}"
    );
    assert!(pid_of(&threads[0].id, cx).is_none());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_stop_after_the_program_ended_records_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(started) = op(fixture.run("/bin/sh -c \"exit 0\""), cx).await.unwrap()
    else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let before = kinds_of(&started.id, cx).await;
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(kinds_of(&started.id, cx).await, before);
    assert_eq!(thread(&started.id, cx).await.status, ThreadStatus::Idle);
}

#[gpui_kit::test]
async fn a_new_run_forgets_the_saved_screen_of_the_last_one(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    fixture.workspace.update(cx, |workspace, _| {
        workspace.run_mut(&id).saved = true;
    });
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();

    let saved = fixture
        .workspace
        .read_with(cx, |workspace, _| workspace.runs[&id].saved);
    assert!(!saved);
}

#[gpui_kit::test]
async fn unarchive_waits_for_an_archived_lifecycle_owner(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(owner) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let OpResult::Thread(dependent) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: fixture.project.clone(),
            program: program().into(),
            title: None,
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: Some(owner.id.clone()),
            metadata: None,
            quiet: false,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!()
    };
    op(
        Op::Thread(ThreadOp::Archive {
            id: owner.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(thread(&dependent.id, cx).await.archived_at.is_some());
    assert_eq!(
        op(
            Op::Thread(ThreadOp::Unarchive {
                id: dependent.id.clone()
            }),
            cx
        )
        .await,
        Err(OpError::Invalid(
            "unarchive the lifecycle owner first".into()
        ))
    );
    assert!(thread(&dependent.id, cx).await.archived_at.is_some());
    op(
        Op::Thread(ThreadOp::Unarchive {
            id: owner.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(thread(&dependent.id, cx).await.archived_at.is_some());
}
