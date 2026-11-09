use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, GitTrack, commit_file, empty_repo, environment_of, events, git_in, op, pending_delete,
    pid_of, program, repo_dir, started_personal, tracks_upstream, wait_for_status, worktree_count,
    worktree_run,
};
#[cfg(unix)]
use super::{error_rows, stop_archive_and_settle};

#[gpui_kit::test]
fn a_repo_script_may_run_for_ten_minutes(_cx: &mut TestAppContext) {
    assert_eq!(crate::environment::SCRIPT_LIMIT.as_secs(), 600);
}

#[gpui_kit::test]
fn a_missing_shell_names_the_setup_script(_cx: &mut TestAppContext) {
    let error = crate::environment::shell_from(
        Some(std::ffi::OsStr::new("/kg-no-such-path")),
        ".kagan/setup",
    )
    .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("sh is not on PATH"), "{message}");
    assert!(message.contains(".kagan/setup"), "{message}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_setup_script_that_runs_too_long_fails(_cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("slow.sh");
    std::fs::write(&script, "sleep 30\n").unwrap();
    let started = std::time::Instant::now();
    let error = crate::environment::run_script(
        dir.path(),
        &script,
        ".kagan/setup",
        std::time::Duration::from_millis(200),
    )
    .unwrap_err();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the script was not stopped"
    );
    assert!(error.to_string().contains("timed out"), "{error}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_worktree_from_origin_main_does_not_track(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    let origin = tempfile::tempdir().unwrap();
    git_in(origin.path(), &["init", "--bare"]);
    empty_repo(&repo);
    git_in(&repo, &["branch", "-M", "main"]);
    git_in(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            &origin.path().display().to_string(),
        ],
    );
    git_in(&repo, &["push", "origin", "HEAD:refs/heads/main"]);
    git_in(&repo, &["fetch", "origin"]);
    let (log, _track) = GitTrack::install(&repo);
    let OpResult::Thread(started) = op(
        worktree_run(&fixture, Some("notes"), Some("origin/main")),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert_eq!(folder.status, EnvironmentStatus::Ready);
    assert!(
        !tracks_upstream(&folder.path),
        "the new branch tracks upstream"
    );
    let argv = std::fs::read_to_string(&log).unwrap();
    let add = argv
        .lines()
        .find(|line| line.contains("worktree add"))
        .expect("worktree add");
    let track = add.find("--no-track").expect(add);
    let dash = add.find(" -- ").expect(add);
    assert!(track < dash, "{add}");
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_local_base_worktree_does_not_track_and_needs_no_setup(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    git_in(&repo, &["branch", "-M", "main"]);
    let (log, _track) = GitTrack::install(&repo);
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), Some("main")), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert_eq!(folder.provider, EnvironmentProvider::Worktree);
    assert_eq!(folder.status, EnvironmentStatus::Ready);
    assert!(!tracks_upstream(&folder.path));
    let argv = std::fs::read_to_string(&log).unwrap();
    assert!(
        argv.lines()
            .any(|line| line.contains("worktree add") && line.contains("--no-track")),
        "{argv}"
    );
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn the_start_screen_reads_the_setup_script_at_a_revision(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch setup-ran\n");
    let read = EnvironmentOp::Setup {
        project_id: fixture.project.clone(),
        rev: "HEAD".into(),
    };
    let OpResult::Setup(Some(setup)) = op(Op::Environment(read), cx).await.unwrap() else {
        panic!("setup")
    };
    assert_eq!(setup.mode, Some(SetupFile::Script));
    assert_eq!(setup.text, "touch setup-ran");
    assert!(!setup.truncated);
    assert_eq!(setup.commit.len(), 40);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn the_start_screen_reads_a_setup_link_as_a_link(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    std::fs::create_dir_all(repo.join(".kagan")).unwrap();
    std::os::unix::fs::symlink("touch-nothing", repo.join(".kagan/setup")).unwrap();
    git_in(&repo, &["add", "--", ".kagan/setup"]);
    git_in(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "link",
        ],
    );
    let read = EnvironmentOp::Setup {
        project_id: fixture.project.clone(),
        rev: "HEAD".into(),
    };
    let OpResult::Setup(Some(setup)) = op(Op::Environment(read), cx).await.unwrap() else {
        panic!("setup")
    };
    assert_eq!(setup.mode, Some(SetupFile::Link));
    assert_eq!(setup.text, "");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_setup_script_that_exits_zero_starts_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch setup-ran\n");
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert_eq!(folder.status, EnvironmentStatus::Ready);
    assert!(folder.path.join("setup-ran").is_file());
    assert!(!repo.join("setup-ran").exists());
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_failed_setup_keeps_the_worktree_and_retry_starts_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(
        &repo,
        ".kagan/setup",
        "if [ -f .kagan/once ]; then exit 0; fi\ntouch .kagan/once\nexit 1\n",
    );
    let error = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains(".kagan/setup"), "{error}");
    assert_eq!(worktree_count(fixture.dir.path()), 1);
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    let failed = store
        .environments(&fixture.project)
        .unwrap()
        .into_iter()
        .find(|environment| environment.status == EnvironmentStatus::Error)
        .expect("the failed setup");
    assert!(failed.path.exists());
    drop(store);
    let OpResult::Threads(threads) = op(
        Op::Thread(ThreadOp::List {
            project_id: Some(fixture.project.clone()),
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
    assert!(threads.is_empty());
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    assert_eq!(started.environment_id.as_ref(), Some(&failed.id));
    let folder = environment_of(&failed.id, cx).await;
    assert_eq!(folder.status, EnvironmentStatus::Ready);
    assert_eq!(worktree_count(fixture.dir.path()), 1);
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_failing_teardown_is_followed_by_worktree_remove_without_force(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/teardown", "touch teardown-ran\nexit 1\n");
    let (log, _track) = GitTrack::install(&repo);
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let environment_id = started.environment_id.clone().unwrap();
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
    let mut folder = environment_of(&environment_id, cx).await;
    for _ in 0..40 {
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        folder = environment_of(&environment_id, cx).await;
        if folder.status == EnvironmentStatus::Error {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(folder.status, EnvironmentStatus::Error, "{folder:?}");
    let reason = folder.error.as_deref().unwrap_or("");
    assert!(reason.contains("teardown-ran"), "{reason}");
    assert!(reason.contains("git-ignore"), "{reason}");
    assert!(reason.contains("remove the worktree again"), "{reason}");
    assert!(folder.path.join("teardown-ran").is_file());
    assert!(folder.path.exists());
    let argv = std::fs::read_to_string(&log).unwrap();
    let removals: Vec<_> = argv
        .lines()
        .filter(|line| line.contains("worktree remove"))
        .collect();
    assert!(!removals.is_empty(), "{argv}");
    assert!(
        removals.iter().all(|line| !line.contains("--force")),
        "{removals:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_ignored_setup_file_is_removed_with_the_worktree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".gitignore", "marker\n");
    commit_file(&repo, ".kagan/setup", "touch marker\n");
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let environment_id = started.environment_id.clone().unwrap();
    let folder = stop_archive_and_settle(&started.id, &environment_id, cx).await;
    assert!(!folder.path.exists(), "{folder:?}");
    assert_eq!(folder.status, EnvironmentStatus::Destroyed, "{folder:?}");
    assert!(folder.error.is_none(), "{folder:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_untracked_file_names_a_capped_list_and_remove_again_clears_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(
        &repo,
        ".kagan/setup",
        "touch block-1 block-2 block-3 block-4 block-5 block-6 block-7\n",
    );
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let environment_id = started.environment_id.clone().unwrap();
    let folder = stop_archive_and_settle(&started.id, &environment_id, cx).await;
    assert!(folder.path.exists(), "{folder:?}");
    let refused = op(
        Op::Environment(EnvironmentOp::Remove {
            id: environment_id.clone(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(refused.to_string().contains("block-1"), "{refused}");
    assert!(refused.to_string().contains("and 2 more"), "{refused}");
    assert!(folder.path.exists(), "{folder:?}");
    let still = environment_of(&environment_id, cx).await;
    assert_eq!(still.status, EnvironmentStatus::Error, "{still:?}");
    assert!(
        still.error.as_deref().unwrap_or("").contains("block-1"),
        "{still:?}"
    );
    let reason = still.error.as_deref().unwrap_or("");
    assert!(reason.contains("block-1"), "{reason}");
    assert!(reason.contains("block-5"), "{reason}");
    assert!(reason.contains("and 2 more"), "{reason}");
    assert!(!reason.contains("block-6"), "{reason}");
    assert!(!reason.contains("block-7"), "{reason}");
    assert!(
        reason.contains("Commit, delete, or git-ignore them"),
        "{reason}"
    );
    for name in 1..=7 {
        let _ = std::fs::remove_file(folder.path.join(format!("block-{name}")));
    }
    op(
        Op::Environment(EnvironmentOp::Remove {
            id: environment_id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let cleared = environment_of(&environment_id, cx).await;
    assert!(!cleared.path.exists(), "{cleared:?}");
    assert_eq!(cleared.status, EnvironmentStatus::Destroyed, "{cleared:?}");
    assert!(cleared.error.is_none(), "{cleared:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn restoring_a_blocked_worktree_clears_the_reason_and_remove_is_refused(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch kept-marker\n");
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let environment_id = started.environment_id.clone().unwrap();
    let folder = stop_archive_and_settle(&started.id, &environment_id, cx).await;
    assert_eq!(folder.status, EnvironmentStatus::Error, "{folder:?}");
    op(
        Op::Thread(ThreadOp::Unarchive {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let restored = environment_of(&environment_id, cx).await;
    assert_eq!(restored.status, EnvironmentStatus::Ready, "{restored:?}");
    assert!(restored.error.is_none(), "{restored:?}");
    assert!(restored.path.exists(), "{restored:?}");
    let error = op(
        Op::Environment(EnvironmentOp::Remove { id: environment_id }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("not waiting to be removed"),
        "{error}"
    );
    assert!(restored.path.exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn deleting_the_thread_is_refused_until_the_worktree_is_removed(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch kept-marker\n");
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let environment_id = started.environment_id.clone().unwrap();
    let folder = stop_archive_and_settle(&started.id, &environment_id, cx).await;
    assert!(folder.error.is_some(), "{folder:?}");
    let error = op(
        Op::Thread(ThreadOp::Delete {
            id: started.id.clone(),
            approval_thread_id: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("remove the worktree again"),
        "{error}"
    );
    op(
        Op::Thread(ThreadOp::Get {
            id: started.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let kept = environment_of(&environment_id, cx).await;
    assert_eq!(kept.status, EnvironmentStatus::Error, "{kept:?}");
    assert!(kept.path.exists(), "{kept:?}");
    assert!(kept.error.is_some(), "{kept:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_removal_refusal_is_not_reused_for_the_next_worktree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch kept-marker\n");
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let environment_id = started.environment_id.clone().unwrap();
    let folder = stop_archive_and_settle(&started.id, &environment_id, cx).await;
    assert!(folder.path.exists(), "{folder:?}");
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    store.delete_thread(&started.id, Timestamp(1), &[]).unwrap();
    drop(store);
    let OpResult::Thread(fresh) = op(worktree_run(&fixture, Some("again"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    assert_ne!(fresh.environment_id.as_ref(), Some(&environment_id));
    let kept = environment_of(&environment_id, cx).await;
    assert!(kept.path.exists(), "{kept:?}");
    assert_eq!(kept.status, EnvironmentStatus::Error, "{kept:?}");
    assert!(
        kept.error.as_deref().unwrap_or("").contains("kept-marker"),
        "{kept:?}"
    );
    let created = environment_of(fresh.environment_id.as_ref().unwrap(), cx).await;
    assert_eq!(created.status, EnvironmentStatus::Ready, "{created:?}");
    assert!(created.error.is_none(), "{created:?}");
}

#[test]
fn become_ready_clears_a_removal_reason() {
    let dir = tempfile::tempdir().unwrap();
    empty_repo(dir.path());
    let environment = Environment {
        id: EnvironmentId("env".into()),
        project_id: ProjectId("project".into()),
        path: dir.path().to_path_buf(),
        status: EnvironmentStatus::Error,
        provider: EnvironmentProvider::Worktree,
        branch: Some("kg".into()),
        base_branch: None,
        retire_at: None,
        error: Some("The worktree still has files git would keep.".into()),
    };
    let crate::environment::SetupEnd::Ready(done) = crate::environment::become_ready(environment)
    else {
        panic!("ready");
    };
    assert_eq!(done.environment.status, EnvironmentStatus::Ready);
    assert!(done.environment.error.is_none());
}

#[gpui_kit::test]
async fn a_project_folder_does_not_run_setup(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch setup-ran\nexit 1\n");
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    assert!(pid_of(&started.id, cx).is_some());
    assert!(!repo.join("setup-ran").exists());
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert_eq!(folder.provider, EnvironmentProvider::Project);
}

#[gpui_kit::test]
async fn a_personal_folder_does_not_run_teardown(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let marker = fixture.dir.path().join("teardown-marker");
    let script = folder.join(".kagan/teardown");
    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, format!("touch '{}'\nexit 1\n", marker.display())).unwrap();
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: thread.clone(),
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(!marker.exists());
    assert!(!folder.exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_setup_symlink_is_not_run_and_is_a_notice(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let marker = std::env::temp_dir().join(format!("kg-setup-link-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let target = std::env::temp_dir().join(format!("kg-setup-target-{}", std::process::id()));
    std::fs::write(&target, format!("touch '{}'\n", marker.display())).unwrap();
    std::fs::create_dir_all(repo.join(".kagan")).unwrap();
    std::os::unix::fs::symlink(&target, repo.join(".kagan/setup")).unwrap();
    git_in(&repo, &["add", "--", ".kagan/setup"]);
    git_in(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "link",
        ],
    );
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    assert!(!marker.exists());
    assert!(pid_of(&started.id, cx).is_some());
    let notice = events(&started.id, cx).await.into_iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Notice { ref text, .. } if text.contains(".kagan/setup") && text.contains("link")
        )
    });
    assert!(notice);
    let _ = std::fs::remove_file(&marker);
    let _ = std::fs::remove_file(&target);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_deleted_setup_in_the_working_tree_still_runs(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch setup-ran\n");
    std::fs::remove_file(repo.join(".kagan/setup")).unwrap();
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert!(folder.path.join("setup-ran").is_file());
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_uncommitted_setup_symlink_does_not_replace_the_committed_script(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch committed-ran\n");
    std::fs::remove_file(repo.join(".kagan/setup")).unwrap();
    let target = fixture.dir.path().join("other-setup.sh");
    std::fs::write(&target, "touch linked-ran\n").unwrap();
    std::os::unix::fs::symlink(&target, repo.join(".kagan/setup")).unwrap();
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert!(folder.path.join("committed-ran").is_file());
    assert!(!folder.path.join("linked-ran").exists());
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_uncommitted_setup_is_not_run(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    let path = repo.join(".kagan/setup");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "touch setup-ran\nexit 1\n").unwrap();
    let OpResult::Thread(started) = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    assert!(!repo.join("setup-ran").exists());
    let folder = environment_of(&started.environment_id.clone().unwrap(), cx).await;
    assert!(!folder.path.join("setup-ran").exists());
    assert_eq!(folder.status, EnvironmentStatus::Ready);
    assert!(pid_of(&started.id, cx).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_setup_blob_that_does_not_match_is_not_run(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "touch setup-ran\n");
    git_in(
        &repo,
        &["config", "filter.kgmismatch.smudge", "sed 's/^/X/'"],
    );
    std::fs::write(
        repo.join(".gitattributes"),
        ".kagan/setup filter=kgmismatch\n",
    )
    .unwrap();
    git_in(&repo, &["add", "--", ".gitattributes"]);
    git_in(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "attributes",
        ],
    );
    let error = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains(".kagan/setup"), "{message}");
    assert!(message.contains(".kagan/setup changed"), "{message}");
    assert!(
        message.contains("Start will use the version shown on screen now"),
        "{message}"
    );
    assert!(matches!(error, OpError::SetupChanged(_)), "{error}");
    assert_eq!(
        serde_json::to_value(&error).unwrap()["error"]["kind"],
        "setup-changed"
    );
    assert_eq!(worktree_count(fixture.dir.path()), 0);
    assert!(error_rows(&fixture).is_empty());
    let OpResult::Threads(threads) = op(
        Op::Thread(ThreadOp::List {
            project_id: Some(fixture.project.clone()),
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
    assert!(threads.is_empty());
    let again = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    assert!(
        again
            .to_string()
            .contains("Start will use the version shown on screen now"),
        "{again}"
    );
    assert_eq!(worktree_count(fixture.dir.path()), 0);
    assert!(
        error_rows(&fixture).is_empty(),
        "a mismatch stayed reusable"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_failing_setup_that_mentions_removal_keeps_its_worktree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(
        &repo,
        ".kagan/setup",
        "echo 'The worktree was removed.' >&2\nexit 1\n",
    );
    let error = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("failed (exit 1)"), "{message}");
    assert!(message.contains("The worktree was removed"), "{message}");
    assert_eq!(worktree_count(fixture.dir.path()), 1);
    let kept = error_rows(&fixture);
    assert_eq!(kept.len(), 1);
    assert!(kept[0].path.exists());
    let again = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    assert!(again.to_string().contains("failed (exit 1)"), "{again}");
    let still = error_rows(&fixture);
    assert_eq!(still.len(), 1);
    assert_eq!(still[0].id, kept[0].id);
    assert!(still[0].path.exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn checking_out_a_branch_moves_the_project_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = clean_repo(&fixture);
    git_in(&repo, &["checkout", "-q", "-b", "topic"]);
    git_in(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "--allow-empty",
            "-qm",
            "topic",
        ],
    );
    let topic = rev(&repo);
    git_in(&repo, &["checkout", "-q", "main"]);
    op(
        project_on(
            &fixture,
            Some("Notes"),
            ProjectBranch::Checkout {
                name: "topic".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(branch_name(&repo), "topic");
    assert_eq!(rev(&repo), topic);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_dirty_checkout_refuses_a_new_branch_and_stays_for_the_current_one(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let repo = clean_repo(&fixture);
    let stay = rev(&repo);
    std::fs::write(repo.join("dirty.txt"), "changed\n").unwrap();
    let error = op(
        project_on(
            &fixture,
            Some("Notes"),
            ProjectBranch::New {
                base: "main".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("uncommitted changes in this checkout"),
        "{error}"
    );
    assert_eq!(rev(&repo), stay);
    op(
        project_on(
            &fixture,
            None,
            ProjectBranch::Checkout {
                name: "main".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    op(fixture.run(super::program()), cx).await.unwrap();
    assert_eq!(branch_name(&repo), "main");
    assert_eq!(rev(&repo), stay);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_new_project_branch_takes_the_title_and_a_live_thread_blocks_the_next(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let repo = clean_repo(&fixture);
    git_in(&repo, &["checkout", "-q", "-b", "topic"]);
    git_in(&repo, &["checkout", "-q", "main"]);
    op(
        project_on(
            &fixture,
            Some("Ship the notes"),
            ProjectBranch::New {
                base: "main".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(branch_name(&repo), "ship-the-notes");
    let error = op(
        project_on(
            &fixture,
            Some("Elsewhere"),
            ProjectBranch::Checkout {
                name: "topic".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Another thread is using this folder"),
        "{error}"
    );
    assert_eq!(branch_name(&repo), "ship-the-notes");
    std::fs::write(repo.join("dirty.txt"), "changed\n").unwrap();
    let error = op(
        project_on(
            &fixture,
            Some("Elsewhere"),
            ProjectBranch::Checkout {
                name: "topic".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("uncommitted changes in this checkout"),
        "{error}"
    );
    assert_eq!(branch_name(&repo), "ship-the-notes");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_new_project_branch_refuses_a_base_that_is_not_local(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = clean_repo(&fixture);
    git_in(&repo, &["tag", "shipped"]);
    let stay = rev(&repo);
    for base in ["shipped", stay.as_str()] {
        let error = op(
            project_on(
                &fixture,
                Some("Notes"),
                ProjectBranch::New {
                    base: base.to_owned(),
                },
            ),
            cx,
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("branch {base} is not in this repository")),
            "{error}"
        );
        assert_eq!(branch_name(&repo), "main");
        assert_eq!(rev(&repo), stay);
    }
}

#[cfg(unix)]
#[gpui_kit::test]
async fn checking_out_the_unborn_branch_does_not_move_head(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    git_in(&repo, &["init", "-b", "main"]);
    op(
        project_on(
            &fixture,
            None,
            ProjectBranch::Checkout {
                name: "main".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(show_current(&repo), "main");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_branch_named_head_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = clean_repo(&fixture);
    let stay = rev(&repo);
    let error = op(
        project_on(
            &fixture,
            Some("HEAD"),
            ProjectBranch::New {
                base: "main".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("branch name head is not allowed"),
        "{error}"
    );
    assert_eq!(branch_name(&repo), "main");
    assert_eq!(rev(&repo), stay);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_new_project_branch_refuses_a_name_that_exists(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = clean_repo(&fixture);
    git_in(&repo, &["checkout", "-q", "-b", "ship-the-notes"]);
    git_in(&repo, &["checkout", "-q", "main"]);
    let error = op(
        project_on(
            &fixture,
            Some("Ship the notes"),
            ProjectBranch::New {
                base: "main".into(),
            },
        ),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("branch ship-the-notes already exists"),
        "{error}"
    );
    assert_eq!(branch_name(&repo), "main");
}

/// The fixture folder holds the database, so a git repo there ignores everything but the files a
/// test means to commit. `dirty.txt` stays visible to `git status`.
fn clean_repo(fixture: &Fixture) -> std::path::PathBuf {
    let repo = repo_dir(fixture);
    empty_repo(&repo);
    git_in(&repo, &["branch", "-M", "main"]);
    std::fs::write(repo.join(".gitignore"), "*\n!.gitignore\n!dirty.txt\n").unwrap();
    git_in(&repo, &["add", "--", ".gitignore"]);
    git_in(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "ignore",
        ],
    );
    repo
}

fn project_on(fixture: &Fixture, title: Option<&str>, branch: ProjectBranch) -> Op {
    Op::Thread(ThreadOp::Run(Run {
        project_id: fixture.project.clone(),
        program: super::program().into(),
        title: title.map(str::to_owned),
        environment: EnvironmentSpec::ProjectOn { branch },
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }))
}

fn rev(repo: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn show_current(repo: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "--show-current"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn branch_name(repo: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_retry_after_the_setup_file_changes_drops_the_worktree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let repo = repo_dir(&fixture);
    empty_repo(&repo);
    commit_file(&repo, ".kagan/setup", "exit 1\n");
    op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    let kept = error_rows(&fixture);
    assert_eq!(kept.len(), 1);
    let old_id = kept[0].id.clone();
    let old_path = kept[0].path.clone();
    std::fs::write(old_path.join(".kagan/setup"), "echo changed\n").unwrap();
    let error = op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Start will use the version shown on screen now"),
        "{error}"
    );
    assert!(!old_path.exists());
    assert!(error_rows(&fixture).is_empty());
    op(worktree_run(&fixture, Some("notes"), None), cx)
        .await
        .unwrap_err();
    let fresh = error_rows(&fixture);
    assert_eq!(fresh.len(), 1);
    assert_ne!(fresh[0].id, old_id);
    assert_eq!(fresh[0].path, old_path);
    assert!(fresh[0].path.exists());
}
