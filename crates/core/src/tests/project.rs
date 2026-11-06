use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, SshRewrite, dispatch_delete, empty_repo, environment_of, git_in, hold_removal,
    hold_run, op, pending_delete, program, repo_dir, started, started_personal, thread,
    wait_for_status, worktree_run,
};
#[cfg(unix)]
use super::{stopped_worktree, wait_until_gone};

#[gpui_kit::test]
async fn adding_a_folder_makes_one_project(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let folder = fixture.dir.path().join("site");
    std::fs::create_dir(&folder).unwrap();
    let add = |path: std::path::PathBuf| Op::Project(ProjectOp::Add { path });

    let OpResult::Project(project) = op(add(folder.clone()), cx).await.unwrap() else {
        panic!()
    };
    assert_eq!(project.name, "site");
    assert_eq!(project.kind, ProjectKind::Standard);
    assert_eq!(project.path, dunce::canonicalize(&folder).unwrap());
    assert!(cx.read(|cx| fixture.workspace.read(cx).project(&project.id).is_some()));

    let again = op(add(folder.clone()), cx).await.unwrap_err();
    assert!(matches!(again, OpError::Invalid(_)), "{again}");
    let file = folder.join("notes.txt");
    std::fs::write(&file, "").unwrap();
    let not_folder = op(add(file), cx).await.unwrap_err();
    assert!(matches!(not_folder, OpError::Invalid(_)), "{not_folder}");
    let missing = op(add(folder.join("gone")), cx).await.unwrap_err();
    assert!(matches!(missing, OpError::Invalid(_)), "{missing}");
    let long = op(add(folder.join("a".repeat(300))), cx).await.unwrap_err();
    assert_eq!(long.to_string(), "the project path is too long");
}

#[gpui_kit::test]
async fn adding_a_repo_records_its_origin_url_without_credentials(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let _rewrite = SshRewrite::install(fixture.dir.path());
    let cases = [
        (
            "https://github.com/example/repo.git",
            "https://github.com/example/repo.git",
        ),
        (
            "https://user:token@github.com/example/repo.git",
            "https://github.com/example/repo.git",
        ),
        (
            "git@github.com:example/app.git",
            "git@github.com:example/app.git",
        ),
        (
            "ssh://git:secret@host:2222/org/repo.git",
            "ssh://git@host:2222/org/repo.git",
        ),
        (
            "ssh://git@host:2222/org/repo.git",
            "ssh://git@host:2222/org/repo.git",
        ),
        (
            "git://user@host/org/repo.git",
            "git://user@host/org/repo.git",
        ),
    ];
    for (index, (origin, stored)) in cases.into_iter().enumerate() {
        let folder = fixture.dir.path().join(format!("repo{index}"));
        std::fs::create_dir(&folder).unwrap();
        git_in(&folder, &["init"]);
        git_in(&folder, &["remote", "add", "origin", origin]);
        let OpResult::Project(project) = op(Op::Project(ProjectOp::Add { path: folder }), cx)
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(project.git_remote_url.as_deref(), Some(stored), "{origin}");
    }
}

#[gpui_kit::test]
async fn personal_is_last_in_both_sidebars(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let zebra = fixture.dir.path().join("Zebra");
    std::fs::create_dir(&zebra).unwrap();
    op(Op::Project(ProjectOp::Add { path: zebra }), cx)
        .await
        .unwrap();
    let OpResult::Sidebar(from_op) = op(Op::Sidebar, cx).await.unwrap() else {
        panic!("sidebar")
    };
    let OpResult::Projects(from_list) = op(Op::Project(ProjectOp::List), cx).await.unwrap() else {
        panic!("projects")
    };
    let from_workspace = cx.read(|cx| fixture.workspace.read(cx).sidebar(cx));
    for projects in [from_op.projects, from_list, from_workspace.projects] {
        let names: Vec<_> = projects
            .iter()
            .map(|project| project.name.as_str())
            .collect();
        assert_eq!(names, ["Project", "Zebra", "Personal"]);
        assert_eq!(
            projects.last().map(|project| project.kind),
            Some(ProjectKind::Personal)
        );
    }
}

#[gpui_kit::test]
async fn reorder_moves_a_standard_project_and_keeps_personal_last(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let zebra = fixture.dir.path().join("Zebra");
    std::fs::create_dir(&zebra).unwrap();
    let OpResult::Project(zebra) = op(Op::Project(ProjectOp::Add { path: zebra }), cx)
        .await
        .unwrap()
    else {
        panic!("zebra")
    };
    op(
        Op::Project(ProjectOp::Reorder {
            id: zebra.id.clone(),
            before: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Sidebar(sidebar) = op(Op::Sidebar, cx).await.unwrap() else {
        panic!("sidebar")
    };
    let names: Vec<_> = sidebar
        .projects
        .iter()
        .map(|project| project.name.as_str())
        .collect();
    assert_eq!(names, ["Zebra", "Project", "Personal"]);
    let refused = op(
        Op::Project(ProjectOp::Reorder {
            id: ProjectId("personal".into()),
            before: None,
        }),
        cx,
    )
    .await;
    assert!(matches!(refused, Err(OpError::Invalid(_))), "{refused:?}");
}

#[gpui_kit::test]
async fn a_project_cannot_be_added_to_a_folder_that_is_being_removed(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let release = hold_removal(&fixture, cx);
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(folder.exists());
    let error = op(
        Op::Project(ProjectOp::Add {
            path: folder.clone(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("being removed"), "{error}");
    release.try_send(()).unwrap();
    cx.run_until_parked();
    assert!(!folder.exists());
}

#[gpui_kit::test]
async fn a_project_in_a_personal_folder_keeps_it_when_its_thread_is_deleted(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    let linked = folder.clone();
    let fixture = fixture
        .restart(cx, |store| {
            store
                .put_project(&Project {
                    id: ProjectId("linked".into()),
                    kind: ProjectKind::Standard,
                    name: "Linked".into(),
                    path: linked,
                    git_remote_url: None,
                    sort: 0,
                })
                .unwrap();
        })
        .await;
    op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: ProjectId("linked".into()),
            program: program().into(),
            title: Some("linked".into()),
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap();
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    let (approval, question) = pending_delete(cx);
    assert!(!question.contains("permanently"), "{question}");
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    drop(fixture);
}

#[gpui_kit::test]
async fn remove_project_asks_and_keeps_the_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let folder = fixture.dir.path().join("keep.txt");
    std::fs::write(&folder, "keep").unwrap();
    let remove = || {
        Op::Project(ProjectOp::Remove {
            id: fixture.project.clone(),
        })
    };
    let cancel = cx.update(|cx| Workspace::dispatch(remove(), cx));
    cx.run_until_parked();
    let (approval, question) = pending_delete(cx);
    assert!(
        question.contains("Project")
            && !question.contains("0 threads")
            && question.contains("The folder stays on disk."),
        "{question}"
    );
    cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx))
        .await
        .unwrap();
    assert!(cancel.await.is_err());
    assert!(
        op(
            Op::Project(ProjectOp::Get {
                id: fixture.project.clone()
            }),
            cx
        )
        .await
        .is_ok()
    );
    let confirm = cx.update(|cx| Workspace::dispatch(remove(), cx));
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    confirm.await.unwrap();
    assert!(folder.exists());
    assert!(
        op(
            Op::Project(ProjectOp::Get {
                id: fixture.project.clone()
            }),
            cx
        )
        .await
        .is_err()
    );
    assert!(cx.read(|cx| {
        fixture
            .workspace
            .read(cx)
            .project(&fixture.project)
            .is_none()
    }));
}

#[gpui_kit::test]
async fn a_serialized_request_runs_while_a_removal_waits_for_a_person(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let remove = Op::Project(ProjectOp::Remove {
        id: fixture.project.clone(),
    });
    assert!(!crate::dispatch::serialized(&remove));
    let snapshot = Op::Ui(UiOp::Snapshot { window: 0 });
    assert!(crate::dispatch::serialized(&snapshot));
    let removing = cx.update(|cx| Workspace::dispatch(remove, cx));
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);

    // The queue is one request wide: a handler that awaited this answer would hold every
    // `kg` command until the person answered.
    assert!(op(Op::Sidebar, cx).await.is_ok());
    assert!(matches!(
        op(snapshot, cx).await,
        Err(OpError::Failed(message)) if message == UI_UNAVAILABLE
    ));

    cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx))
        .await
        .unwrap();
    assert!(removing.await.is_err());
}

#[gpui_kit::test]
async fn a_stopped_command_takes_its_project_removal_question_with_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let remove = |requester| {
        let op = Op::Project(ProjectOp::Remove {
            id: fixture.project.clone(),
        });
        move |cx: &mut gpui_kit::App| Workspace::dispatch_for(op, Some(requester), cx)
    };
    let project = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            Workspace::dispatch(
                Op::Project(ProjectOp::Get {
                    id: fixture.project.clone(),
                }),
                cx,
            )
        })
    };
    let approvals =
        |cx: &TestAppContext| cx.read(|cx| Workspace::global(cx).read(cx).approvals().count());

    let (present, gone) = async_channel::bounded::<()>(1);
    let removing = cx.update(remove(crate::Requester::new(gone)));
    cx.run_until_parked();
    assert_eq!(approvals(cx), 1);
    drop(present);
    cx.run_until_parked();
    assert_eq!(
        approvals(cx),
        0,
        "the card stayed after its command stopped"
    );
    assert!(removing.await.is_err());
    assert!(project(cx).await.is_ok());

    let opened = std::rc::Rc::new(std::cell::Cell::new(0));
    let _opens = cx.update(|cx| {
        let opened = opened.clone();
        cx.subscribe(&fixture.workspace, move |_, event, _| {
            if matches!(event, crate::WorkspaceEvent::Open) {
                opened.set(opened.get() + 1);
            }
        })
    });
    let (present, gone) = async_channel::bounded::<()>(1);
    let removing = cx.update(remove(crate::Requester::new(gone)));
    drop(present);
    cx.run_until_parked();
    assert_eq!(
        opened.get(),
        0,
        "a card was raised for a command already gone"
    );
    assert!(removing.await.is_err());
    assert!(project(cx).await.is_ok());

    let (_present, gone) = async_channel::bounded::<()>(1);
    let removing = cx.update(remove(crate::Requester::new(gone)));
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    removing.await.unwrap();
    assert!(project(cx).await.is_err());
}

#[gpui_kit::test]
async fn remove_project_refuses_personal_and_running_programs(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (thread_id, _reap) = started(&fixture, program(), cx).await;
    let error = op(
        Op::Project(ProjectOp::Remove {
            id: fixture.project.clone(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("Stop the programs in"),
        "{error}"
    );
    let error = op(
        Op::Project(ProjectOp::Remove {
            id: ProjectId("personal".into()),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Personal"), "{error}");
    assert_eq!(thread(&thread_id, cx).await.status, ThreadStatus::Active);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn remove_project_removes_worktrees_and_archived_threads(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(started.environment_id.as_ref().unwrap(), cx)
        .await
        .path;
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    op(
        Op::Thread(ThreadOp::Archive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, question) = pending_delete(cx);
    assert!(question.contains("and its thread"), "{question}");
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    removing.await.unwrap();
    assert!(!folder.exists());
    assert!(repo.exists());
    assert!(
        op(Op::Thread(ThreadOp::Get { id: started.id }), cx)
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn remove_project_reports_a_worktree_git_keeps(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("dirty"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(started.environment_id.as_ref().unwrap(), cx)
        .await
        .path;
    std::fs::write(folder.join("valuable.txt"), "keep").unwrap();
    op(
        Op::Thread(ThreadOp::Stop {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    let OpResult::Warnings(warnings) = removing.await.unwrap() else {
        panic!("warnings")
    };
    let text = warnings.join("; ");
    assert!(text.contains("valuable.txt"), "{warnings:?}");
    assert!(
        text.contains(&format!("git worktree remove {}", folder.display())),
        "{warnings:?}"
    );
    assert!(!text.contains("remove the worktree again"), "{warnings:?}");
    assert!(folder.join("valuable.txt").exists());
    assert!(
        op(Op::Thread(ThreadOp::Get { id: started.id }), cx)
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_restart_finishes_a_project_removal_cut_between_worktrees(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let (first, first_folder) = stopped_worktree(&fixture, "first", cx).await;
    let (second, second_folder) = stopped_worktree(&fixture, "second", cx).await;
    let mut worktrees = [
        (
            thread(&first, cx).await.environment_id.unwrap(),
            first_folder,
        ),
        (
            thread(&second, cx).await.environment_id.unwrap(),
            second_folder,
        ),
    ];
    worktrees.sort_by(|a, b| a.0.cmp(&b.0));
    let [(_, first), (_, second)] = worktrees;
    let release = hold_removal(&fixture, cx);
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    release.send(()).await.unwrap();
    wait_until_gone(&first, cx);
    assert!(second.exists());
    drop(removing);
    let fixture = fixture.restart(cx, |_| {}).await;
    wait_until_gone(&second, cx);
    assert!(repo.exists());
    assert!(
        op(
            Op::Project(ProjectOp::Get {
                id: fixture.project.clone()
            }),
            cx
        )
        .await
        .is_ok()
    );
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    removing.await.unwrap();
    assert!(
        op(
            Op::Project(ProjectOp::Get {
                id: fixture.project.clone()
            }),
            cx
        )
        .await
        .is_err()
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_run_still_preparing_blocks_the_project_removal(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let release_run = hold_run(&fixture, cx);
    let running =
        cx.update(|cx| Workspace::dispatch(worktree_run(&fixture, Some("late"), None), cx));
    cx.run_until_parked();
    let error = op(
        Op::Project(ProjectOp::Remove {
            id: fixture.project.clone(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("Stop the programs in"),
        "{error}"
    );
    release_run.try_send(()).unwrap();
    running.await.unwrap();
    assert!(
        op(
            Op::Project(ProjectOp::Get {
                id: fixture.project.clone()
            }),
            cx
        )
        .await
        .is_ok()
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_run_after_the_removal_began_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let (_, folder) = stopped_worktree(&fixture, "first", cx).await;
    let release = hold_removal(&fixture, cx);
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    let error = op(worktree_run(&fixture, Some("late"), None), cx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("being removed"), "{error}");
    release.try_send(()).unwrap();
    removing.await.unwrap();
    assert!(!folder.exists());
}

#[gpui_kit::test]
async fn a_refresh_keeps_the_project_and_environment_entities(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (thread_id, _reap) = started(&fixture, program(), cx).await;
    let environment_id = thread(&thread_id, cx)
        .await
        .environment_id
        .expect("thread environment");
    let before = fixture.workspace.read_with(cx, |workspace, _| {
        (
            workspace
                .project(&fixture.project)
                .map(gpui_kit::Entity::entity_id),
            workspace
                .environment(&environment_id)
                .map(gpui_kit::Entity::entity_id),
            workspace
                .thread(&thread_id)
                .map(gpui_kit::Entity::entity_id),
        )
    });
    assert!(before.0.is_some() && before.1.is_some() && before.2.is_some());

    op(
        Op::Project(ProjectOp::Reorder {
            id: fixture.project.clone(),
            before: None,
        }),
        cx,
    )
    .await
    .unwrap();

    let after = fixture.workspace.read_with(cx, |workspace, _| {
        (
            workspace
                .project(&fixture.project)
                .map(gpui_kit::Entity::entity_id),
            workspace
                .environment(&environment_id)
                .map(gpui_kit::Entity::entity_id),
            workspace
                .thread(&thread_id)
                .map(gpui_kit::Entity::entity_id),
        )
    });
    assert_eq!(before, after);
}
