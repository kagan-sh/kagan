#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use gpui_kit::{Entity, TestAppContext};
use kg_domain::*;
use serde_json::json;

use crate::Workspace;
use crate::database::Result;

#[cfg(unix)]
mod details;
#[cfg(unix)]
mod extra;
#[cfg(unix)]
mod resume;
mod search;
mod update;

struct Fixture {
    dir: tempfile::TempDir,
    project: ProjectId,
    workspace: Entity<Workspace>,
}

impl Fixture {
    async fn new(cx: &mut TestAppContext) -> Self {
        Self::seeded(cx, |_| {}).await
    }

    async fn seeded(cx: &mut TestAppContext, seed: impl FnOnce(&kg_store::Store)) -> Self {
        Self::with_kind(cx, ProjectKind::Standard, seed).await
    }

    async fn personal(cx: &mut TestAppContext) -> Self {
        Self::with_kind(cx, ProjectKind::Personal, |_| {}).await
    }

    async fn with_kind(
        cx: &mut TestAppContext,
        kind: ProjectKind,
        seed: impl FnOnce(&kg_store::Store),
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let project = ProjectId("project".into());
        let store = kg_store::Store::open(dir.path().join("kagan.sqlite3")).unwrap();
        seed(&store);
        store
            .put_project(&Project {
                id: project.clone(),
                kind,
                name: "Project".into(),
                path: dunce::canonicalize(dir.path()).unwrap(),
                git_remote_url: None,
                sort: 0,
            })
            .unwrap();
        drop(store);
        let workspace = cx
            .update(|cx| Workspace::open(dir.path().into(), std::env::current_exe().unwrap(), cx))
            .await
            .unwrap();
        Self {
            dir,
            project,
            workspace,
        }
    }

    /// Closes the app, lets `plant` edit the saved data, and opens it again.
    async fn restart(
        self,
        cx: &mut TestAppContext,
        plant: impl FnOnce(&mut kg_store::Store),
    ) -> Self {
        let Self {
            dir,
            project,
            workspace,
        } = self;
        cx.update(gpui_kit::App::remove_global::<crate::workspace::WorkspaceGlobal>);
        drop(workspace);
        cx.run_until_parked();
        let mut store = kg_store::Store::open(dir.path().join("kagan.sqlite3")).unwrap();
        plant(&mut store);
        drop(store);
        let workspace = cx
            .update(|cx| Workspace::open(dir.path().into(), std::env::current_exe().unwrap(), cx))
            .await
            .unwrap();
        Self {
            dir,
            project,
            workspace,
        }
    }

    fn run(&self, program: &str) -> Op {
        Op::Thread(ThreadOp::Run(Run {
            project_id: self.project.clone(),
            program: program.into(),
            title: None,
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        }))
    }
}

fn program() -> &'static str {
    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

async fn op(op: Op, cx: &mut TestAppContext) -> Result<OpResult> {
    cx.update(|cx| Workspace::dispatch(op, cx)).await
}

#[gpui_kit::test]
async fn the_guide_contains_its_map(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let OpResult::Text(guide) = op(Op::Guide { chapter: None }, cx).await.unwrap() else {
        panic!("guide")
    };
    assert!(guide.contains(include_str!("guide-map.md")));
}

async fn events(id: &ThreadId, cx: &mut TestAppContext) -> Vec<Event> {
    let OpResult::Events(events) = op(
        Op::Thread(ThreadOp::Events {
            id: id.clone(),
            after_seq: None,
            limit: None,
            order: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!()
    };
    events
}

async fn thread(id: &ThreadId, cx: &mut TestAppContext) -> Thread {
    let OpResult::Thread(thread) = op(Op::Thread(ThreadOp::Get { id: id.clone() }), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    thread
}

fn git_in(path: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn empty_repo(path: &Path) {
    git_in(path, &["init"]);
    git_in(
        path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "--allow-empty",
            "-m",
            "Initial",
        ],
    );
}

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

struct SshRewrite {
    previous_global: Option<std::ffi::OsString>,
    previous_nosystem: Option<std::ffi::OsString>,
}

impl SshRewrite {
    fn install(dir: &Path) -> Self {
        let path = dir.join("gitconfig");
        std::fs::write(
            &path,
            "[url \"https://github.com/\"]\n\tinsteadOf = git@github.com:\n",
        )
        .unwrap();
        let previous_global = std::env::var_os("GIT_CONFIG_GLOBAL");
        let previous_nosystem = std::env::var_os("GIT_CONFIG_NOSYSTEM");
        // SAFETY: nextest runs each test in its own process, so this test owns these
        // process-global variables while the guard is alive and restores them on drop.
        unsafe {
            std::env::set_var("GIT_CONFIG_GLOBAL", &path);
            std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        }
        Self {
            previous_global,
            previous_nosystem,
        }
    }
}

impl Drop for SshRewrite {
    fn drop(&mut self) {
        // SAFETY: nextest runs each test in its own process, so nothing else reads these
        // variables while the guard restores them.
        unsafe {
            match self.previous_global.take() {
                Some(value) => std::env::set_var("GIT_CONFIG_GLOBAL", value),
                None => std::env::remove_var("GIT_CONFIG_GLOBAL"),
            }
            match self.previous_nosystem.take() {
                Some(value) => std::env::set_var("GIT_CONFIG_NOSYSTEM", value),
                None => std::env::remove_var("GIT_CONFIG_NOSYSTEM"),
            }
        }
    }
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

async fn socket_rename(id: &ThreadId, title: &str, cx: &mut TestAppContext) -> Result<OpResult> {
    let id = id.clone();
    let title = title.to_owned();
    cx.update(|cx| Workspace::rename_once(id, title, cx)).await
}

async fn environment_branch(id: &EnvironmentId, cx: &mut TestAppContext) -> Option<String> {
    let OpResult::Environment(folder) =
        op(Op::Environment(EnvironmentOp::Get { id: id.clone() }), cx)
            .await
            .unwrap()
    else {
        panic!("environment")
    };
    folder.branch
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

fn plant_old_turn(db: &Path, id: &ThreadId) {
    let mut store = kg_store::Store::open(db).unwrap();
    store
        .set_thread_status(id, ThreadStatus::Active, Timestamp(1))
        .unwrap();
    let planted = store
        .append_events(
            id,
            Timestamp(1),
            [
                EventKind::Message {
                    role: Role::Agent,
                    text: "still here".into(),
                    message_id: None,
                    attachments: Vec::new(),
                },
                EventKind::Tool {
                    id: "tool-1".into(),
                    name: Some("read".into()),
                    status: ToolStatus::Completed,
                },
            ],
        )
        .unwrap();
    store
        .append_events(
            id,
            Timestamp(1),
            [EventKind::Rewound {
                before_seq: planted.last().map_or(1, |event| event.seq) + 1,
            }],
        )
        .unwrap();
    store
        .put_pending_interaction(&PendingInteraction {
            id: PendingInteractionId("ask".into()),
            thread_id: id.clone(),
            title: "Allow".into(),
            options: vec![InteractionOption {
                id: "allow".into(),
                label: "Allow".into(),
            }],
            fields: None,
            created_at: Timestamp(1),
        })
        .unwrap();
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
        .update(|cx| Workspace::open(dir.path().into(), std::env::current_exe().unwrap(), cx))
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
    assert!(events(&child.id, cx).await.iter().any(|event| {
        matches!(&event.kind, EventKind::Failed { error: Some(text) } if text == "kagan quit while the program was running")
    }));
    assert_eq!(thread(&child.id, cx).await.status, ThreadStatus::Error);
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
    assert!(events(&parent.id, cx).await.iter().any(|event| {
        matches!(&event.kind, EventKind::Notice { text, .. } if text.contains("child") && text.contains("kagan quit while the program was running"))
    }));
}

#[gpui_kit::test]
async fn a_diff_lists_the_uncommitted_file_and_the_branch_commit(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    commit(root, &["commit", "--allow-empty", "-m", "base"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    git_in(root, &["add", "a.txt"]);
    commit(root, &["commit", "-m", "one"]);
    git_in(root, &["checkout", "-b", "feature"]);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    commit(root, &["commit", "-am", "two"]);
    std::fs::write(root.join("b.txt"), "new\n").unwrap();
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let uncommitted = diff_paths(&started.id, DiffScope::Uncommitted, cx).await;
    assert!(
        uncommitted.iter().any(|path| path == "b.txt"),
        "{uncommitted:?}"
    );
    let branch = diff_paths(&started.id, DiffScope::Branch, cx).await;
    assert!(branch.iter().any(|path| path == "a.txt"), "{branch:?}");
}

#[gpui_kit::test]
async fn a_staged_deletion_stays_in_the_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    commit(root, &["commit", "--allow-empty", "-m", "base"]);
    std::fs::write(root.join("old.txt"), "gone\n").unwrap();
    std::fs::write(root.join("keep.txt"), "stay\n").unwrap();
    git_in(root, &["add", "old.txt", "keep.txt"]);
    commit(root, &["commit", "-m", "add"]);
    std::fs::write(root.join("keep.txt"), "changed\n").unwrap();
    git_in(root, &["rm", "old.txt"]);
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    let OpResult::Diff(list) = op(
        Op::Diff(DiffOp::List {
            query: DiffQuery {
                thread_id: started.id,
                scope: DiffScope::Uncommitted,
                base: None,
                rev: None,
            },
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("diff")
    };
    let deleted = list
        .files
        .iter()
        .find(|file| file.path == "old.txt")
        .unwrap_or_else(|| panic!("{list:?}"));
    assert_eq!(deleted.status, DiffStatus::Deleted);
    let kept = list
        .files
        .iter()
        .find(|file| file.path == "keep.txt")
        .unwrap_or_else(|| panic!("{list:?}"));
    assert_eq!(kept.status, DiffStatus::Modified);
}

fn commit(path: &Path, args: &[&str]) {
    let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.test"];
    full.extend_from_slice(args);
    git_in(path, &full);
}

async fn diff_paths(id: &ThreadId, scope: DiffScope, cx: &mut TestAppContext) -> Vec<String> {
    let OpResult::Diff(list) = op(
        Op::Diff(DiffOp::List {
            query: DiffQuery {
                thread_id: id.clone(),
                scope,
                base: None,
                rev: None,
            },
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("diff")
    };
    list.files.into_iter().map(|file| file.path).collect()
}

#[gpui_kit::test]
async fn preference_set_stores_a_boolean(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let refused = op(
        Op::Preference(PreferenceOp::Set {
            key: "confirm-archive".into(),
            value: json!("yes"),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(refused, OpError::Invalid(ref message) if message.contains("boolean")),
        "{refused}"
    );
    op(
        Op::Preference(PreferenceOp::Set {
            key: "confirm-archive".into(),
            value: json!(true),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Setting(value) = op(
        Op::Preference(PreferenceOp::Get {
            key: "confirm-archive".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("preference")
    };
    assert_eq!(value, json!(true));
}

#[gpui_kit::test]
async fn the_preference_keys_named_in_help_are_accepted(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for key in [
        "confirm-archive",
        "agent-integration",
        "notify",
        "terminal-font-size",
        "main.left-sidebar",
        "main.right-sidebar",
        "main.left-width",
        "main.right-width",
        "main.review-wrap",
    ] {
        let result = op(Op::Preference(PreferenceOp::Get { key: key.into() }), cx).await;
        assert!(result.is_ok(), "help names {key}, which is refused");
    }
}

#[gpui_kit::test]
async fn terminal_font_size_accepts_only_whole_points_in_range(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for value in [json!(8), json!(25), json!(12.5), json!("12")] {
        let result = op(
            Op::Preference(PreferenceOp::Set {
                key: "terminal-font-size".into(),
                value: value.clone(),
            }),
            cx,
        )
        .await;
        assert!(
            matches!(result, Err(OpError::Invalid(_))),
            "accepted {value}"
        );
    }
    for value in [9, 24] {
        op(
            Op::Preference(PreferenceOp::Set {
                key: "terminal-font-size".into(),
                value: json!(value),
            }),
            cx,
        )
        .await
        .unwrap();
    }
}

#[gpui_kit::test]
async fn a_theme_set_after_the_migration_stays(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(
        Op::Theme(ThemeOp::Set {
            theme: "ember".into(),
            mode: Appearance::Dark,
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Theme(settings) = op(Op::Theme(ThemeOp::Get), cx).await.unwrap() else {
        panic!("theme")
    };
    assert_eq!(settings.theme, "ember");
}

#[test]
fn a_stop_waits_in_the_queue_and_a_run_does_not() {
    assert!(crate::dispatch::serialized(&Op::Thread(ThreadOp::Stop {
        id: ThreadId("thr_a".into()),
    })));
    assert!(!crate::dispatch::serialized(&Op::Thread(ThreadOp::Run(
        Run {
            project_id: ProjectId("project".into()),
            program: "sh".into(),
            title: None,
            environment: EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None,
            },
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        },
    ))));
}

#[test]
fn a_program_splits_on_spaces_outside_quotes() {
    let Err(error) = crate::thread::resolve_program("kg-no-such-program trailing", None) else {
        panic!("resolved");
    };
    assert_eq!(error, "program kg-no-such-program was not found");
    let line = if cfg!(windows) {
        "cmd.exe /c echo"
    } else {
        "/bin/sh -c echo"
    };
    let resolved = crate::thread::resolve_program(line, None).unwrap();
    let expected = if cfg!(windows) {
        ["/c", "echo"]
    } else {
        ["-c", "echo"]
    };
    assert_eq!(
        resolved.args,
        expected
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    );
}

#[test]
#[cfg(unix)]
fn quotes_keep_spaces_and_empty_words_and_backslashes_are_plain() {
    let args = |line: &str| {
        crate::thread::resolve_program(line, None)
            .unwrap()
            .args
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(args("/bin/sh -c 'echo a b'"), ["-c", "echo a b"]);
    assert_eq!(args("/bin/sh \"\" x"), ["", "x"]);
    assert_eq!(args("/bin/sh -c a\"b c\"d"), ["-c", "ab cd"]);
    assert_eq!(args("/bin/sh -c 'it\"s'"), ["-c", "it\"s"]);
    assert_eq!(args(r"/bin/sh -c C:\tools\x"), ["-c", r"C:\tools\x"]);
}

#[test]
fn a_program_with_a_bad_quote_or_no_name_is_refused() {
    assert_eq!(
        crate::thread::resolve_program("'/bin/sh -c x", None)
            .err()
            .as_deref(),
        Some("program has an unfinished quote")
    );
    assert_eq!(
        crate::thread::resolve_program("\"\" -c x", None)
            .err()
            .as_deref(),
        Some("program is empty")
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

fn personal_run(fixture: &Fixture, title: &str, environment: EnvironmentSpec) -> Op {
    Op::Thread(ThreadOp::Run(Run {
        project_id: fixture.project.clone(),
        program: program().into(),
        title: Some(title.into()),
        environment,
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }))
}

async fn environment_of(id: &EnvironmentId, cx: &mut TestAppContext) -> Environment {
    let OpResult::Environment(environment) =
        op(Op::Environment(EnvironmentOp::Get { id: id.clone() }), cx)
            .await
            .unwrap()
    else {
        panic!("environment")
    };
    environment
}

async fn started_personal(fixture: &Fixture, cx: &mut TestAppContext) -> (ThreadId, EnvironmentId) {
    let OpResult::Thread(started) = op(
        personal_run(fixture, "notes", EnvironmentSpec::Personal),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    (started.id, started.environment_id.unwrap())
}

fn pending_delete(cx: &mut TestAppContext) -> (PendingInteractionId, String) {
    cx.read(|cx| {
        let workspace = Workspace::global(cx);
        let approval = workspace.read(cx).approvals().next().unwrap();
        (approval.id.clone(), approval.title.clone())
    })
}

#[gpui_kit::test]
async fn archived_personal_files_survive_the_grace_and_a_restart(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    op(Op::Thread(ThreadOp::Archive { id: thread.clone() }), cx)
        .await
        .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    let kept = environment_of(&environment_id, cx).await;
    assert_eq!(kept.status, EnvironmentStatus::Ready, "{kept:?}");
    assert!(kept.retire_at.is_none(), "{kept:?}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    op(Op::Thread(ThreadOp::Unarchive { id: thread }), cx)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    drop(fixture);
}

#[gpui_kit::test]
async fn an_old_destroyed_personal_folder_stays_while_its_thread_is_archived(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    op(Op::Thread(ThreadOp::Archive { id: thread.clone() }), cx)
        .await
        .unwrap();
    let fixture = fixture
        .restart(cx, |store| {
            let mut environment = store.environment(&environment_id).unwrap().unwrap();
            environment.status = EnvironmentStatus::Destroyed;
            environment.retire_at = Some(Timestamp(0));
            store.put_environment(&environment).unwrap();
        })
        .await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    op(Op::Thread(ThreadOp::Unarchive { id: thread }), cx)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    drop(fixture);
}

#[gpui_kit::test]
async fn deleting_a_personal_thread_names_its_folder_and_removes_it_on_approval(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
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
    let (approval, question) = pending_delete(cx);
    assert!(
        question.contains(&folder.display().to_string()),
        "{question}"
    );
    assert!(question.contains("valuable-note.txt"), "{question}");
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(!folder.exists());
    assert!(
        op(Op::Thread(ThreadOp::Get { id: thread }), cx)
            .await
            .is_err()
    );
}

#[gpui_kit::test]
async fn declining_a_personal_delete_keeps_the_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
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
    cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx))
        .await
        .unwrap();
    let error = deleting.await.unwrap_err();
    assert!(error.to_string().contains("declined"), "{error}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    assert!(
        op(Op::Thread(ThreadOp::Get { id: thread }), cx)
            .await
            .is_ok()
    );
}

#[gpui_kit::test]
async fn deleting_a_thread_keeps_a_personal_folder_another_thread_uses(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    op(
        personal_run(
            &fixture,
            "other",
            EnvironmentSpec::Reuse {
                environment_id: environment_id.clone(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: thread,
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, question) = pending_delete(cx);
    assert!(!question.contains("permanently"), "{question}");
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
}

fn dispatch_delete(id: &ThreadId, cx: &mut TestAppContext) -> gpui_kit::Task<Result<OpResult>> {
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: id.clone(),
                approval_thread_id: None,
            }),
            cx,
        )
    })
}

fn hold_removal(fixture: &Fixture, cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        fixture
            .workspace
            .update(cx, |workspace, _| workspace.removal_hold = Some(hold));
    });
    release
}

fn hold_delete(fixture: &Fixture, cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        fixture
            .workspace
            .update(cx, |workspace, _| workspace.delete_hold = Some(hold));
    });
    release
}

fn hold_run(fixture: &Fixture, cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        fixture
            .workspace
            .update(cx, |workspace, _| workspace.run_hold = Some(hold));
    });
    release
}

#[gpui_kit::test]
async fn a_run_that_read_a_folder_before_its_delete_cannot_attach_to_it(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    std::fs::write(folder.join("valuable-note.txt"), "keep me").unwrap();
    let release_removal = hold_removal(&fixture, cx);
    let release_delete = hold_delete(&fixture, cx);
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    let release_run = hold_run(&fixture, cx);
    let running = cx.update(|cx| {
        Workspace::dispatch(
            personal_run(
                &fixture,
                "late",
                EnvironmentSpec::Reuse {
                    environment_id: environment_id.clone(),
                },
            ),
            cx,
        )
    });
    cx.run_until_parked();
    release_delete.try_send(()).unwrap();
    for _ in 0..500 {
        cx.run_until_parked();
        let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
        let destroyed = store
            .environment(&environment_id)
            .unwrap()
            .is_some_and(|environment| environment.status == EnvironmentStatus::Destroyed);
        drop(store);
        if destroyed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    release_run.try_send(()).unwrap();
    let error = running.await.unwrap_err();
    assert!(error.to_string().contains("ready"), "{error}");
    deleting.await.unwrap();
    release_removal.try_send(()).unwrap();
    cx.run_until_parked();
    assert!(!folder.exists());
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
    assert!(threads.iter().all(|thread| thread.title != "late"));
}

#[gpui_kit::test]
async fn a_thread_that_reuses_the_folder_during_the_approval_keeps_it(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    op(
        personal_run(
            &fixture,
            "late",
            EnvironmentSpec::Reuse {
                environment_id: environment_id.clone(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    let error = deleting.await.unwrap_err();
    assert!(error.to_string().contains("delete again"), "{error}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    assert!(
        op(Op::Thread(ThreadOp::Get { id: thread }), cx)
            .await
            .is_ok()
    );
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
async fn a_delete_interrupted_before_its_folder_is_removed_finishes_at_restart(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    std::fs::write(folder.join("valuable-note.txt"), "keep me").unwrap();
    let release = hold_removal(&fixture, cx);
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(folder.exists());
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert!(!folder.exists());
    assert!(
        op(
            Op::Environment(EnvironmentOp::Get { id: environment_id }),
            cx
        )
        .await
        .is_err()
    );
    drop(release);
    drop(fixture);
}

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|word| word == pid.to_string())
            })
    }
}

fn gone(pid: u32) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while alive(pid) {
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    true
}

struct Reap(u32);

impl Drop for Reap {
    fn drop(&mut self) {
        // `kill -9 -- -1` signals every process the user owns.
        if self.0 <= 1 {
            return;
        }
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill")
                // Without `--`, Linux `kill -9 -7551` signalled group 7, not 7551.
                .args(["-9", "--", &format!("-{}", self.0)])
                .stderr(std::process::Stdio::null())
                .status();
            let _ = std::process::Command::new("kill")
                .args(["-9", &self.0.to_string()])
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &self.0.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

async fn started(fixture: &Fixture, program: &str, cx: &mut TestAppContext) -> (ThreadId, Reap) {
    let OpResult::Thread(started) = op(fixture.run(program), cx).await.unwrap() else {
        panic!("thread")
    };
    let pid = pid_of(&started.id, cx).expect("the host started the program");
    (started.id, Reap(pid))
}

fn pid_of(id: &ThreadId, cx: &mut TestAppContext) -> Option<u32> {
    cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(id))
}

async fn screen(id: &ThreadId, cx: &mut TestAppContext) -> Result<String> {
    match op(
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback: false,
            lines: None,
            terminal: None,
        }),
        cx,
    )
    .await?
    {
        OpResult::Screen(frame) => Ok(frame),
        other => panic!("{other:?}"),
    }
}

async fn screen_with(id: &ThreadId, needle: &str, cx: &mut TestAppContext) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let frame = screen(id, cx).await.unwrap();
        if frame.contains(needle) || std::time::Instant::now() > deadline {
            return frame;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

async fn type_in(id: &ThreadId, text: &str, cx: &mut TestAppContext) {
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

#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

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

async fn kinds_of(id: &ThreadId, cx: &mut TestAppContext) -> Vec<EventKind> {
    events(id, cx)
        .await
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn busy_of(fixture: &Fixture, id: &ThreadId, cx: &mut TestAppContext) -> bool {
    cx.read(|cx| fixture.workspace.read(cx).thread(id).unwrap().read(cx).busy)
}

async fn wait_for_status(id: &ThreadId, status: ThreadStatus, cx: &mut TestAppContext) {
    for _ in 0..400 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if thread(id, cx).await.status == status {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the thread never became {status:?}");
}

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

fn turn_counts(kinds: &[EventKind]) -> (usize, usize) {
    let started = kinds
        .iter()
        .filter(|kind| matches!(kind, EventKind::TurnStarted))
        .count();
    let ended = kinds
        .iter()
        .filter(|kind| {
            matches!(
                kind,
                EventKind::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    error: None
                }
            )
        })
        .count();
    (started, ended)
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
async fn finish_quit(cx: &mut TestAppContext, id: &ThreadId, quitting: gpui_kit::Task<Result<()>>) {
    // stop_for_quit's test-clock deadline is 3s; past that it fails with
    // "timed out stopping programs" while a slow kill is still finishing.
    let wall = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut advanced = std::time::Duration::ZERO;
    while std::time::Instant::now() < wall {
        cx.run_until_parked();
        if thread(id, cx).await.status == ThreadStatus::Idle {
            break;
        }
        if advanced < std::time::Duration::from_secs(2) {
            let step = std::time::Duration::from_millis(20);
            cx.executor().advance_clock(step);
            advanced += step;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    quitting.await.unwrap();
}

#[cfg(unix)]
fn assert_quit_leaves_the_turn(kinds: &[EventKind]) {
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| matches!(kind, EventKind::TurnStarted))
            .count(),
        1,
        "{kinds:?}"
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| matches!(kind, EventKind::TurnEnded { .. }))
            .count(),
        0,
        "{kinds:?}"
    );
    assert!(kinds.contains(&EventKind::Stopped), "{kinds:?}");
    assert!(kinds.contains(&EventKind::Idle), "{kinds:?}");
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, EventKind::Failed { .. })),
        "{kinds:?}"
    );
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

#[cfg(unix)]
fn set_row_status(fixture: &Fixture, id: &ThreadId, status: ThreadStatus, cx: &mut TestAppContext) {
    cx.update(|cx| {
        fixture.workspace.update(cx, |workspace, cx| {
            workspace.thread(id).unwrap().update(cx, |thread, cx| {
                thread.record.status = status;
                cx.notify();
            });
        });
    });
}

#[cfg(unix)]
async fn row_end_after_its_event(
    fixture: &Fixture,
    kind: EventKind,
    status: ThreadStatus,
    reason: WaitOutcome,
    cx: &mut TestAppContext,
) {
    let (id, _reap) = started(fixture, "/bin/sh -c \"sleep 30\"", cx).await;
    let this = fixture.workspace.downgrade();
    let logged = id.clone();
    cx.update(|cx| {
        cx.spawn(async move |cx| Workspace::events(&this, logged, vec![kind], cx).await)
    })
    .await
    .unwrap();
    let task = begin_wait(&id, 10_000, cx);
    cx.run_until_parked();
    assert!(!task.is_ready(), "{reason} settled from the old event");
    set_row_status(fixture, &id, status, cx);
    cx.run_until_parked();
    assert!(task.is_ready(), "{reason} waited out the end of the row");
    assert_eq!(outcome(&task.await.unwrap()), reason);
}

#[cfg(unix)]
fn begin_wait(
    id: &ThreadId,
    timeout_ms: u64,
    cx: &mut TestAppContext,
) -> gpui_kit::Task<Result<OpResult>> {
    let id = id.clone();
    cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Wait { id, timeout_ms }), cx))
}

#[cfg(unix)]
fn finish_wait(task: &gpui_kit::Task<Result<OpResult>>, cx: &mut TestAppContext) {
    for _ in 0..400 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if task.is_ready() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the wait never settled");
}

#[cfg(unix)]
async fn settle(id: &ThreadId, timeout_ms: u64, cx: &mut TestAppContext) -> WaitOutcome {
    outcome(
        &op(
            Op::Thread(ThreadOp::Wait {
                id: id.clone(),
                timeout_ms,
            }),
            cx,
        )
        .await
        .unwrap(),
    )
}

fn outcome(result: &OpResult) -> WaitOutcome {
    let OpResult::Wait(outcome) = result else {
        panic!("wait returned {result:?}");
    };
    *outcome
}

fn ask_options() -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: "yes".into(),
            label: "Yes".into(),
        },
        InteractionOption {
            id: "no".into(),
            label: "No".into(),
        },
    ]
}

fn ask_op(id: ThreadId, title: &str, options: Vec<InteractionOption>) -> Op {
    Op::Thread(ThreadOp::Ask {
        id,
        title: title.into(),
        options,
        fields: None,
    })
}

#[cfg(unix)]
async fn pending(id: &ThreadId, cx: &mut TestAppContext) -> Vec<PendingInteraction> {
    let OpResult::Interactions(pending) =
        op(Op::Thread(ThreadOp::Interactions { id: id.clone() }), cx)
            .await
            .unwrap()
    else {
        panic!("interactions");
    };
    pending
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

#[cfg(unix)]
async fn sleeping(cx: &mut TestAppContext) -> (Fixture, ThreadId) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Thread(thread) = op(fixture.run(r#"/bin/sh -c "sleep 30""#), cx)
        .await
        .unwrap()
    else {
        panic!("thread");
    };
    cx.run_until_parked();
    let id = thread.id;
    (fixture, id)
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
            fields: std::collections::BTreeMap::new(),
        }
    );
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_with_fields_stores_them_and_answered_carries_values(cx: &mut TestAppContext) {
    let (fixture, id) = sleeping(cx).await;
    let fields = vec![
        kg_domain::AskField::String {
            id: "name".into(),
            label: "Name".into(),
            placeholder: None,
            default: Some("Ada".into()),
        },
        kg_domain::AskField::Boolean {
            id: "notify".into(),
            label: "Notify".into(),
            default: Some(true),
        },
    ];
    let OpResult::Text(interaction) = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some(fields.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("ask");
    };
    let open = pending(&id, cx).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].fields, Some(fields));
    assert!(cx.read(|cx| fixture.workspace.read(cx).needs_a_person(&open[0].id)));
    let mut values = std::collections::BTreeMap::new();
    values.insert("name".into(), serde_json::json!("Grace"));
    values.insert("notify".into(), serde_json::json!(false));
    cx.update(|cx| {
        Workspace::answer_fields(
            &id,
            PendingInteractionId(interaction.clone()),
            "yes".into(),
            values.clone(),
            cx,
        )
    })
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
            fields: values,
        }
    );
}

#[cfg(unix)]
async fn waiting_count(id: &ThreadId, cx: &mut TestAppContext) -> usize {
    kinds_of(id, cx)
        .await
        .iter()
        .filter(|kind| matches!(kind, EventKind::Waiting))
        .count()
}

#[cfg(unix)]
async fn wait_for_waiting(id: &ThreadId, count: usize, cx: &mut TestAppContext) {
    for _ in 0..400 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if waiting_count(id, cx).await == count {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!(
        "waiting events never reached {count}: {:?}",
        kinds_of(id, cx).await
    );
}

#[cfg(unix)]
fn wait_until_busy(fixture: &Fixture, id: &ThreadId, cx: &mut TestAppContext) {
    for _ in 0..400 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if busy_of(fixture, id, cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the spinner title never made the thread busy");
}

#[gpui_kit::test]
async fn a_restart_while_a_program_runs_marks_the_thread_failed(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(kinds_of(&id, cx).await.iter().any(|kind| matches!(
        kind,
        EventKind::Failed { error: Some(text) } if text == "kagan quit while the program was running"
    )));
    drop(fixture);
}

#[gpui_kit::test]
async fn a_read_running_thread_is_unread_after_a_restart(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    op(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx)
        .await
        .unwrap();
    let read = thread(&id, cx).await;
    let read_at = read.last_read_at.expect("read");
    std::thread::sleep(std::time::Duration::from_millis(10));
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    let recovered = thread(&id, cx).await;
    assert_eq!(recovered.status, ThreadStatus::Error);
    assert!(recovered.latest_attention_at.0 > read_at.0, "{recovered:?}");
    assert!(recovered.updated_at.0 > read.updated_at.0, "{recovered:?}");
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
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER abort_failed BEFORE INSERT ON events
             WHEN json_extract(NEW.kind, '$.type') = 'failed'
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let interrupted = cx
        .update(|cx| Workspace::open(dir.path().into(), std::env::current_exe().unwrap(), cx))
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
    assert_eq!(status, "active");
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
        .update(|cx| Workspace::open(dir.path().into(), std::env::current_exe().unwrap(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(kinds_of(&id, cx).await.iter().any(|kind| matches!(
        kind,
        EventKind::Failed { error: Some(text) } if text == "kagan quit while the program was running"
    )));
}

async fn screen_of(id: &ThreadId, scrollback: bool, cx: &mut TestAppContext) -> String {
    let OpResult::Screen(screen) = op(
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback,
            lines: None,
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("screen")
    };
    screen
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
    assert!(refused.to_string().contains("not a path"), "{refused}");
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
fn sent_text(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Message {
                role: Role::User,
                text,
                message_id: None,
                attachments,
            } if attachments.is_empty() => Some(text.as_str()),
            EventKind::Message { .. } => panic!("unexpected message {event:?}"),
            _ => None,
        })
        .collect()
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
fn dispatch_send(
    id: &ThreadId,
    text: &str,
    cx: &mut TestAppContext,
) -> gpui_kit::Task<Result<OpResult>> {
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Send {
                id: id.clone(),
                text: text.into(),
            }),
            cx,
        )
    })
}

#[cfg(unix)]
async fn screen_lines(
    id: &ThreadId,
    scrollback: bool,
    lines: Option<u32>,
    cx: &mut TestAppContext,
) -> Result<String> {
    match op(
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback,
            lines,
            terminal: None,
        }),
        cx,
    )
    .await?
    {
        OpResult::Screen(text) => Ok(text),
        other => panic!("{other:?}"),
    }
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
async fn a_restart_does_not_resume_a_program_but_start_again_does_on_request(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(pid_of(&id, cx).is_none());
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Active, cx).await;
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

#[gpui_kit::test]
async fn a_saved_shortcut_is_kept_and_announced(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let announced = std::rc::Rc::new(std::cell::Cell::new(false));
    let workspace = fixture.workspace.clone();
    let _subscription = cx.update({
        let announced = announced.clone();
        move |cx| {
            cx.subscribe(&workspace, move |_, event: &crate::WorkspaceEvent, _| {
                if matches!(event, crate::WorkspaceEvent::Changed) {
                    announced.set(true);
                }
            })
        }
    });
    op(
        Op::Keymap(kg_domain::KeymapOp::Set {
            action: "NewThread".into(),
            key: "secondary-j".into(),
            reset: false,
        }),
        cx,
    )
    .await
    .unwrap();
    cx.run_until_parked();
    assert!(
        announced.get(),
        "the window was not told of the saved shortcut"
    );
    let saved = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .keymap()
            .binding("NewThread")
            .map(str::to_owned)
    });
    let expected = crate::parse_key("secondary-j")
        .expect("secondary-j parses")
        .unparse(crate::host_os());
    assert_eq!(saved.as_deref(), Some(expected.as_str()));
}

#[gpui_kit::test]
async fn shortcut_keys_are_checked_before_they_are_saved(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for key in ["$(rm)", "cmd-🚀", "f01", "f+1", "f0", "f36", "cmd-f1x"] {
        let result = op(
            Op::Keymap(kg_domain::KeymapOp::Set {
                action: "NewThread".into(),
                key: key.into(),
                reset: false,
            }),
            cx,
        )
        .await;
        assert!(result.is_err(), "accepted {key}");
    }
    for key in [
        "secondary-alt-n",
        "cmd-shift-k",
        "cmd-,",
        "f1",
        "f35",
        "cmd-ä",
    ] {
        let result = op(
            Op::Keymap(kg_domain::KeymapOp::Set {
                action: "NewThread".into(),
                key: key.into(),
                reset: false,
            }),
            cx,
        )
        .await;
        assert!(result.is_ok(), "refused {key}");
    }
}

#[gpui_kit::test]
async fn every_default_shortcut_passes_the_key_check(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for shortcut in kg_domain::SHORTCUTS {
        let result = op(
            Op::Keymap(kg_domain::KeymapOp::Set {
                action: shortcut.name.into(),
                key: shortcut.chord.into(),
                reset: false,
            }),
            cx,
        )
        .await;
        assert!(result.is_ok(), "default {} is refused", shortcut.chord);
    }
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

#[test]
fn a_data_dir_that_cannot_be_made_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    let data_dir = file.join("kagan");
    let Err(error) = crate::workspace::open_data(&data_dir) else {
        panic!("opened");
    };
    let message = error.to_string();
    assert!(
        message.contains(&data_dir.display().to_string()),
        "{message}"
    );
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
fn job_pid(screen: &str) -> Option<u32> {
    screen.match_indices("job:").find_map(|(at, _)| {
        let digits: String = screen[at + 4..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    })
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
fn install_plugin(dir: &Path, enabled: bool) {
    let root = dir.join("demo-plugin");
    let skill = root.join("skills/demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(root.join("kagan.json"), r#"{"skills": ["skills/demo"]}"#).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: A demo skill.\n---\n\nDo the demo.\n",
    )
    .unwrap();
    std::fs::write(skill.join("notes.md"), "notes\n").unwrap();
    let store = kg_store::Store::open(dir.join("kagan.sqlite3")).unwrap();
    let id = PluginId("demo-plugin".into());
    if store.plugins().unwrap().iter().any(|row| row.id == id) {
        store.set_plugin_enabled(&id, enabled).unwrap();
        return;
    }
    store
        .insert_plugin(&Plugin {
            id,
            root: dunce::canonicalize(&root).unwrap(),
            source: "test".into(),
            commit: None,
            enabled,
            approved: json!({}),
            error: None,
        })
        .unwrap();
}

/// A script named `name` that records its arguments and environment under `run`.
#[cfg(unix)]
fn fake_agent(run: &Path, name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = run.join("bin");
    let out = run.join("out");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let program = bin.join(name);
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{out}/args'\nenv > '{out}/env.tmp' && mv '{out}/env.tmp' '{out}/env'\nsleep 30\n",
            out = out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

/// Waits for the fake agent to write its environment, then returns its arguments and environment.
#[cfg(unix)]
fn ran(run: &Path) -> (Vec<String>, std::collections::BTreeMap<String, String>) {
    let out = run.join("out");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !out.join("env").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the program never ran"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let args = std::fs::read_to_string(out.join("args"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let env = std::fs::read_to_string(out.join("env"))
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    (args, env)
}

#[cfg(unix)]
#[gpui_kit::test]
async fn thread_programs_receive_a_utf8_locale(cx: &mut TestAppContext) {
    if let Ok(case) = std::env::var("KAGAN_LOCALE_TEST") {
        let fixture = Fixture::new(cx).await;
        if case == "prompt" {
            let (id, _reap) = started(&fixture, "/bin/zsh -f", cx).await;
            op(
                Op::Thread(ThreadOp::Resize {
                    id: id.clone(),
                    cols: 40,
                    rows: 8,
                    terminal: None,
                }),
                cx,
            )
            .await
            .unwrap();
            assert!(screen_with(&id, "demo", cx).await.contains("demo"));
            type_in(
                &id,
                "echo hi from kagan; echo extra >> notes.txt; echo new > added.txt",
                cx,
            )
            .await;
            screen_with(&id, "added", cx).await;
            let grid = cx.update(|cx| Workspace::global(cx).read(cx).grid(&id, None).unwrap());
            let row: String = grid.lines[0].iter().map(|cell| cell.c).collect();
            assert!(row.starts_with("➜ demo (main) ✗ echo hi"), "{row}");
            return;
        }
        let run = fixture.dir.path().join("locale");
        let program = fake_agent(&run, "locale-test");
        let (_id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
        let (_, env) = ran(&run);
        let lang = env.get("LANG").expect("LANG was set");
        if case == "existing" {
            assert_eq!(lang, "fr_FR.UTF-8");
        } else {
            assert!(
                lang.to_ascii_lowercase().replace('-', "").contains("utf8"),
                "LANG={lang}"
            );
        }
        return;
    }
    for case in ["missing", "existing", "prompt"] {
        if case == "prompt" && !Path::new("/bin/zsh").exists() {
            continue;
        }
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "tests::thread_programs_receive_a_utf8_locale",
                "--nocapture",
            ])
            .env("KAGAN_LOCALE_TEST", case)
            .env_remove("LC_ALL")
            .env_remove("LC_CTYPE")
            .env_remove("LANG");
        if case == "existing" {
            command.env("LANG", "fr_FR.UTF-8");
        }
        if case == "prompt" {
            command.env("PROMPT", "➜ demo (main) ✗ ");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "{case}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[cfg(unix)]
fn claude_line(run: &Path) -> String {
    format!("{} --person-arg", fake_agent(run, "claude").display())
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_claude_thread_gets_kagan_flags_env_and_skills(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::SocketLocation("/tmp/kagan-test.sock".into())));
    let fixture = Fixture::new(cx).await;
    install_plugin(fixture.dir.path(), true);
    let run = fixture.dir.path().join("first");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, env) = ran(&run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let files = agent.join("threads").join(&id.0);
    assert_eq!(args[0], "--mcp-config");
    assert_eq!(args[1], files.join("mcp.json").display().to_string());
    assert_eq!(args[2], "--plugin-dir");
    assert_eq!(args[3], agent.display().to_string());
    assert_eq!(args[4], "--append-system-prompt-file");
    assert_eq!(args[5], files.join("note.txt").display().to_string());
    assert_eq!(args[6], "--session-id");
    assert_eq!(args[8], "--person-arg");
    let mcp = std::fs::read_to_string(files.join("mcp.json")).unwrap();
    let mcp: serde_json::Value = serde_json::from_str(&mcp).unwrap();
    assert_eq!(
        mcp["mcpServers"]["kagan"]["args"],
        json!(["mcp", "--thread", id.0.clone()])
    );
    assert_eq!(
        mcp["mcpServers"]["kagan"]["env"]["KG_SOCKET"],
        "/tmp/kagan-test.sock"
    );
    let note = std::fs::read_to_string(files.join("note.txt")).unwrap();
    assert!(note.contains(&id.0), "{note}");
    let environment = thread(&id, cx).await.environment_id.expect("environment").0;
    let kg = std::env::current_exe().unwrap();
    assert_eq!(env["KG_THREAD_ID"], id.0);
    assert_eq!(env["KG_PROJECT_ID"], "project");
    assert_eq!(env["KG_ENVIRONMENT_ID"], environment);
    assert_eq!(env["KG_SOCKET"], "/tmp/kagan-test.sock");
    assert_eq!(env["KG_CLI"], kg.display().to_string());
    assert!(
        env["PATH"].starts_with(&kg.parent().unwrap().display().to_string()),
        "{}",
        env["PATH"]
    );
    assert!(agent.join(".claude-plugin/plugin.json").exists());
    assert!(agent.join("skills/kg-cli/SKILL.md").exists());
    assert!(agent.join("skills/kg-plugin-port/SKILL.md").exists());
    assert!(agent.join("skills/demo/notes.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn the_next_claude_thread_drops_a_deleted_file_and_a_disabled_plugin(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let agent = fixture.dir.path().join("agent");
    install_plugin(fixture.dir.path(), true);
    let (_id, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("one")), cx).await;
    ran(&fixture.dir.path().join("one"));
    std::fs::remove_file(fixture.dir.path().join("demo-plugin/skills/demo/notes.md")).unwrap();
    let (_id, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("two")), cx).await;
    ran(&fixture.dir.path().join("two"));
    assert!(agent.join("skills/demo/SKILL.md").exists());
    assert!(!agent.join("skills/demo/notes.md").exists());
    install_plugin(fixture.dir.path(), false);
    let (_id, _reap) = started(
        &fixture,
        &claude_line(&fixture.dir.path().join("three")),
        cx,
    )
    .await;
    ran(&fixture.dir.path().join("three"));
    assert!(!agent.join("skills/demo").exists());
    assert!(agent.join("skills/kg-cli/SKILL.md").exists());
    assert!(agent.join("skills/kg-threads/SKILL.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_subcommand_line_gets_no_kagan_flags(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("daemon");
    let program = fake_agent(&run, "claude");
    let (_id, _reap) = started(
        &fixture,
        &format!("{} daemon status", program.display()),
        cx,
    )
    .await;
    let (args, _env) = ran(&run);
    assert_eq!(args, vec!["daemon", "status"]);
    assert!(!fixture.dir.path().join("agent").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_broken_agent_folder_starts_the_program_without_plugin_flags(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("agent"), "not a folder").unwrap();
    let run = fixture.dir.path().join("broken");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, env) = ran(&run);
    assert_eq!(args, vec!["--person-arg"]);
    assert_eq!(env["KG_THREAD_ID"], id.0);
    let recorded = events(&id, cx).await;
    assert!(
        recorded.iter().any(|event| matches!(
            &event.kind,
            EventKind::Notice { severity: NoticeSeverity::Warning, text }
                if text.contains("could not set up the agent integration")
        )),
        "{recorded:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_codex_thread_gets_the_server_and_note_as_overrides(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    assert!(
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(fixture.dir.path())
            .status()
            .unwrap()
            .success()
    );
    let run = fixture.dir.path().join("codex");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    let kg = std::env::current_exe().unwrap().display().to_string();
    assert!(
        args.contains(&format!("mcp_servers.kagan.command='{kg}'")),
        "{args:?}"
    );
    assert!(
        args.contains(&format!(
            "mcp_servers.kagan.args=['mcp', '--thread', '{}']",
            id.0
        )),
        "{args:?}"
    );
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains("developer_instructions")),
        "{args:?}"
    );
    assert_eq!(args.last().map(String::as_str), Some("--person-arg"));
    let root = dunce::canonicalize(fixture.dir.path()).unwrap();
    let skill = std::fs::read_to_string(root.join(".agents/skills/kg-threads/SKILL.md")).unwrap();
    assert!(skill.contains("name: kg-threads"), "{skill}");
    assert!(skill.contains("kg thread run"), "{skill}");
    let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
    assert!(exclude.contains(".agents/skills/kg-threads"), "{exclude}");
    assert_eq!(env["KG_THREAD_ID"], id.0);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_cursor_agent_thread_receives_the_threads_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("cursor");
    let program = fake_agent(&run, "cursor-agent");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let cursor = agent.join("threads").join(&id.0).join("cursor");
    assert_eq!(args[0], "--plugin-dir");
    assert_eq!(args[1], cursor.display().to_string());
    assert_eq!(args.last().map(String::as_str), Some("--person-arg"));
    let skill = std::fs::read_to_string(agent.join("skills/kg-threads/SKILL.md")).unwrap();
    assert!(skill.contains("name: kg-threads"), "{skill}");
    assert_eq!(
        std::fs::read_link(cursor.join("skills")).unwrap(),
        dunce::canonicalize(agent.join("skills")).unwrap()
    );
    let rule = std::fs::read_to_string(cursor.join("rules/kagan.mdc")).unwrap();
    assert!(rule.contains("alwaysApply: true"), "{rule}");
    assert!(rule.contains("kg-threads"), "{rule}");
    assert!(cursor.join(".cursor-plugin/plugin.json").exists());
    assert!(!agent.join("rules/kagan.mdc").exists());
    assert_eq!(env["KG_THREAD_ID"], id.0);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn each_cursor_thread_keeps_its_own_rule_file(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let first_run = fixture.dir.path().join("cursor-a");
    let second_run = fixture.dir.path().join("cursor-b");
    let first = fake_agent(&first_run, "cursor-agent");
    let second = fake_agent(&second_run, "cursor-agent");
    let (first_id, _reap_a) = started(&fixture, first.to_str().unwrap(), cx).await;
    ran(&first_run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let first_rule = agent
        .join("threads")
        .join(&first_id.0)
        .join("cursor/rules/kagan.mdc");
    std::fs::write(&first_rule, "keep-first\n").unwrap();
    let (second_id, _reap_b) = started(&fixture, second.to_str().unwrap(), cx).await;
    ran(&second_run);
    let second_rule = agent
        .join("threads")
        .join(&second_id.0)
        .join("cursor/rules/kagan.mdc");
    assert_ne!(first_rule, second_rule);
    assert_eq!(
        std::fs::read_to_string(&first_rule).unwrap(),
        "keep-first\n"
    );
    assert!(
        std::fs::read_to_string(&second_rule)
            .unwrap()
            .contains("alwaysApply: true")
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_non_agent_start_clears_prior_instruct(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("claude-then-clear");
    let program = fake_agent(&run, "claude");
    let (id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    ran(&run);
    cx.update(|cx| {
        let entity = Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .cloned()
            .expect("thread");
        entity.update(cx, |thread, _| {
            thread.instruct = Some("old persona".into());
        });
    });
    op(
        Op::Preference(PreferenceOp::Set {
            key: "agent-integration".into(),
            value: json!(false),
        }),
        cx,
    )
    .await
    .unwrap();
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Active, cx).await;
    let instruct = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .expect("thread")
            .read(cx)
            .instruct
            .clone()
    });
    assert_eq!(instruct, None);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn agent_integration_off_leaves_the_person_line_alone(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(
        Op::Preference(PreferenceOp::Set {
            key: "agent-integration".into(),
            value: json!(false),
        }),
        cx,
    )
    .await
    .unwrap();
    let run = fixture.dir.path().join("off");
    let program = fake_agent(&run, "claude");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    assert_eq!(args, vec!["--person-arg"]);
    assert_eq!(env["KG_THREAD_ID"], id.0);
    assert!(!fixture.dir.path().join("agent").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_codex_resume_line_keeps_the_kagan_server(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("resume");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(
        &fixture,
        &format!("{} resume --last", program.display()),
        cx,
    )
    .await;
    let (args, env) = ran(&run);
    let kg = std::env::current_exe().unwrap().display().to_string();
    assert!(
        args.contains(&format!("mcp_servers.kagan.command='{kg}'")),
        "{args:?}"
    );
    assert_eq!(
        args[args.len() - 2..].to_vec(),
        vec!["resume".to_owned(), "--last".to_owned()]
    );
    assert_eq!(env["KG_THREAD_ID"], id.0);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn archive_and_delete_remove_the_thread_agent_files(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = fixture.dir.path().join("agent");
    let (archived, _reap) =
        started(&fixture, &claude_line(&fixture.dir.path().join("a")), cx).await;
    ran(&fixture.dir.path().join("a"));
    assert!(agent.join("threads").join(&archived.0).exists());
    op(
        Op::Thread(ThreadOp::Archive {
            id: archived.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(!agent.join("threads").join(&archived.0).exists());

    let (deleted, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("b")), cx).await;
    ran(&fixture.dir.path().join("b"));
    assert!(agent.join("threads").join(&deleted.0).exists());
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: deleted.clone(),
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
    assert!(!agent.join("threads").join(&deleted.0).exists());
}

#[cfg(unix)]
fn busy_and_unread(id: &ThreadId, cx: &mut TestAppContext) -> (bool, bool) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(50));
    cx.run_until_parked();
    cx.update(|cx| {
        let entity = Workspace::global(cx).read(cx).thread(id).cloned().unwrap();
        let thread = entity.read(cx);
        (thread.busy(), thread.record().unread())
    })
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

#[cfg(unix)]
fn fake_program(dir: &std::path::Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn the_program_list_finds_known_agents_and_defaults_to_the_first_installed() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "echo 'claude 9.9.9 (test)'");
    let list = crate::program::discover(
        Some(dir.path().as_os_str()),
        None,
        Some(std::ffi::OsStr::new("/bin/sh")),
        &[
            "codex --resume".into(),
            "claude".into(),
            "'unfinished".into(),
        ],
    );
    let claude = list
        .programs
        .iter()
        .find(|p| p.command == "claude")
        .unwrap();
    assert_eq!(claude.source, kg_domain::ProgramSource::Known);
    assert!(claude.installed());
    let codex = list.programs.iter().find(|p| p.command == "codex").unwrap();
    assert!(!codex.installed());
    assert!(
        list.programs
            .iter()
            .any(|p| p.source == kg_domain::ProgramSource::Shell && p.installed())
    );
    let recent = list
        .programs
        .iter()
        .find(|p| p.command == "codex --resume")
        .unwrap();
    assert_eq!(recent.source, kg_domain::ProgramSource::Recent);
    assert!(list.programs.iter().all(|p| p.command != "'unfinished"));
    assert_eq!(list.default.as_deref(), Some("claude"));
}

#[cfg(unix)]
#[test]
fn the_newest_installed_recent_program_is_the_default() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "true");
    fake_program(dir.path(), "tool", "true");
    let list = crate::program::discover(
        Some(dir.path().as_os_str()),
        None,
        None,
        &["tool --fast".into(), "claude".into()],
    );
    assert_eq!(list.default.as_deref(), Some("tool --fast"));
}

#[cfg(unix)]
#[test]
fn only_installed_known_agents_get_a_version() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "echo 'claude 9.9.9 (test)'");
    fake_program(dir.path(), "tool", "echo 'tool 1.0'");
    let mut list =
        crate::program::discover(Some(dir.path().as_os_str()), None, None, &["tool".into()]);
    crate::program::versions(&mut list);
    let version = |command: &str| {
        list.programs
            .iter()
            .find(|p| p.command == command)
            .and_then(|p| p.version.clone())
    };
    assert_eq!(version("claude").as_deref(), Some("claude 9.9.9 (test)"));
    assert_eq!(version("tool"), None);
    assert_eq!(version("codex"), None);
}

#[test]
fn the_saved_screen_uses_the_same_limits_as_a_resize() {
    assert_eq!(kg_terminal::MIN_COLS, *TERMINAL_COLS.start());
    assert_eq!(kg_terminal::MAX_COLS, *TERMINAL_COLS.end());
    assert_eq!(1, *TERMINAL_ROWS.start());
    assert_eq!(kg_terminal::MAX_ROWS, *TERMINAL_ROWS.end());
    assert_eq!(kg_terminal::SNAPSHOT_MAX, 4 * 1024 * 1024);
}

fn idle_thread(id: &str, project: ProjectId) -> Thread {
    Thread {
        id: ThreadId(id.into()),
        project_id: project,
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: "sh".into(),
        status: ThreadStatus::Idle,
        visibility: Visibility::Visible,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned: false,
        last_read_at: None,
        latest_attention_at: Timestamp(1),
        mark: kg_domain::ThreadMark::None,
        archived_at: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}

fn wait_saved(dir: &Path, id: &ThreadId, cx: &mut TestAppContext) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if let Ok(store) = kg_store::Store::open(dir.join("kagan.sqlite3"))
            && store.screens().is_ok_and(|rows| {
                rows.iter()
                    .any(|row| &row.thread_id == id && row.screen_kept && !row.replay.is_empty())
            })
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the settled screen was not saved"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
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
    let fixture = fixture.restart(cx, |_| {}).await;
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Idle);
    assert!(screen_of(&id, true, cx).await.contains("QUIT-MARKER"));
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id)),
        Some("Stopped.".into())
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
    assert_eq!(thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(pid_of(&id, cx).is_none());
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.end_line(&id)),
        Some("Ended when kagan quit. No output was saved for that run. Use Start again to restart it.".into())
    );
    assert!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&id))
            .is_none()
    );
    let missing = op(
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback: true,
            lines: None,
            terminal: None,
        }),
        cx,
    )
    .await;
    assert!(missing.is_err(), "{missing:?}");
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert!(
        store
            .screens()
            .unwrap()
            .iter()
            .all(|row| row.thread_id != id)
    );
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
            Workspace::global(cx).update(cx, |workspace, _| {
                let Some(crate::thread::Shown::Running(terminal)) = workspace.terminals.get(&id)
                else {
                    return false;
                };
                terminal.set_snapshot_bound(1);
                true
            })
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
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, cx| {
            let seq = workspace.runs.get(&id).copied().expect("current run");
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
async fn diagnostics_writes_a_new_file_without_the_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    #[cfg(windows)]
    let line = {
        let path = fixture.dir.path().join("quiet.cmd");
        std::fs::write(&path, "@echo SECRET-MARKER\r\n").unwrap();
        path.display().to_string()
    };
    #[cfg(unix)]
    let line = script(fixture.dir.path(), "quiet", "printf 'SECRET-MARKER\\n'");
    let OpResult::Thread(started) = op(fixture.run(&line), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &started.id, cx);
    let OpResult::Diagnostics { path } =
        op(Op::Diagnostics(DiagnosticsOp::Export { path: None }), cx)
            .await
            .unwrap()
    else {
        panic!("diagnostics")
    };
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("schema "), "{text}");
    assert!(text.contains(&started.id.0), "{text}");
    assert!(text.contains("no environment variables"), "{text}");
    assert!(!text.contains("SECRET-MARKER"), "{text}");
    assert!(!text.contains("HOME="), "{text}");
    let again = op(
        Op::Diagnostics(DiagnosticsOp::Export {
            path: Some(path.clone()),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(again.to_string().contains("already exists"), "{again}");
    let missing = fixture.dir.path().join("no-such-folder").join("out.txt");
    let error = op(
        Op::Diagnostics(DiagnosticsOp::Export {
            path: Some(missing),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("does not exist"), "{error}");
    drop(fixture);
}

#[gpui_kit::test]
async fn diagnostics_omits_secrets_and_names_bad_geometry(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let dir = fixture.dir.path().to_path_buf();
    let log_dir = dir.join("plugins").join("demo-plugin");
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(
        log_dir.join("logs.jsonl"),
        "\
{\"at\":1,\"level\":\"debug\",\"message\":\"pw-debug-9f3a\"}\n\
{\"at\":2,\"level\":\"info\",\"message\":\"pw-info-9f3a\"}\n\
{\"at\":3,\"level\":\"warn\",\"message\":\"pw-log-9f3a\"}\n\
{\"at\":4,\"level\":\"error\",\"message\":\"bearer pw-error-9f3a\"}\n\
not-json pw-raw-9f3a\n",
    )
    .unwrap();
    let db = dir.join("kagan.sqlite3");
    let root = dir.clone();
    let fixture = fixture
        .restart(cx, move |store| {
            let mut record = idle_thread("thr_secret", project);
            record.title = "kept-prose-9f3a https://dev:pw-title-9f3a@example.com/tool?token=pw-titleq-9f3a https://example.com/a,https://dev:pw-second-9f3a@github.com/org/repo".into();
            store.put_thread(&record).unwrap();
            let id = ThreadId("thr_secret".into());
            let started = store
                .append_events(&id, Timestamp(1), [EventKind::Started])
                .unwrap();
            store
                .append_events(
                    &id,
                    Timestamp(2),
                    [EventKind::Failed {
                        error: Some(
                            "https://dev:pw-end-9f3a@example.com/e?x=pw-endq-9f3a".into(),
                        ),
                    }],
                )
                .unwrap();
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "INSERT INTO terminal_screens \
                    (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
                 VALUES ('thr_secret', ?1, 9999, 24, ?2, 9, 1)",
                rusqlite::params![started[0].seq, b"pw-replay-9f3a"],
            )
            .unwrap();
            plant_secret_plugins(store, &root);
        })
        .await;
    let id = ThreadId("thr_secret".into());
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&id)),
        Some("The screen was outside the geometry or the 4 MiB bound.")
    );
    let OpResult::Diagnostics { path } =
        op(Op::Diagnostics(DiagnosticsOp::Export { path: None }), cx)
            .await
            .unwrap()
    else {
        panic!("diagnostics")
    };
    let text = std::fs::read_to_string(&path).unwrap();
    assert_diagnostics_keeps_secrets_out(&text);
    drop(fixture);
}

fn plant_secret_plugins(store: &mut kg_store::Store, root: &Path) {
    store
        .insert_plugin(&plugin(
            "demo-plugin",
            &root.join("pw-root-9f3a"),
            "git:https://dev:pw-userinfo-9f3a@example.com/repo.git?token=pw-query-9f3a@main",
            Some("a1b2c3d"),
            "pw-approved-9f3a",
        ))
        .unwrap();
    store
        .insert_plugin(&plugin(
            "plain-plugin",
            &root.join("plain-root"),
            "/tmp/pw-path-9f3a/plugin",
            None,
            "",
        ))
        .unwrap();
    store
        .insert_plugin(&plugin(
            "scp-plugin",
            &root.join("scp-root"),
            "git:alice:pw-scp-9f3a@github.com:org/repo@v1",
            Some("pw-commit-9f3a"),
            "",
        ))
        .unwrap();
    store
        .insert_plugin(&plugin(
            "ref-plugin",
            &root.join("ref-root"),
            "git:https://github.com/org/repo.git@main?token=pw-refq-9f3a",
            None,
            "",
        ))
        .unwrap();
}

fn plugin(id: &str, root: &Path, source: &str, commit: Option<&str>, approved: &str) -> Plugin {
    Plugin {
        id: PluginId(id.into()),
        root: root.to_path_buf(),
        source: source.into(),
        commit: commit.map(str::to_owned),
        enabled: true,
        approved: if approved.is_empty() {
            json!({})
        } else {
            json!({ "marker": approved })
        },
        error: None,
    }
}

fn assert_diagnostics_keeps_secrets_out(text: &str) {
    for secret in [
        "pw-userinfo-9f3a",
        "pw-query-9f3a",
        "pw-title-9f3a",
        "pw-titleq-9f3a",
        "pw-end-9f3a",
        "pw-endq-9f3a",
        "pw-log-9f3a",
        "pw-error-9f3a",
        "pw-debug-9f3a",
        "pw-info-9f3a",
        "pw-raw-9f3a",
        "pw-second-9f3a",
        "pw-scp-9f3a",
        "pw-refq-9f3a",
        "pw-path-9f3a",
        "pw-commit-9f3a",
        "pw-root-9f3a",
        "pw-approved-9f3a",
        "pw-replay-9f3a",
        "kept-prose-9f3a",
        "github.com",
        "example.com",
    ] {
        assert!(!text.contains(secret), "{secret} leaked:\n{text}");
    }
    assert!(text.contains("thr_secret idle"), "{text}");
    assert!(text.contains("end: Failed."), "{text}");
    assert!(text.contains("kind=git"), "{text}");
    assert!(text.contains("kind=path"), "{text}");
    assert!(text.contains("version=a1b2c3d"), "{text}");
    assert!(text.contains("version=unknown"), "{text}");
    assert!(text.contains("version=none"), "{text}");
    assert!(text.contains("1 warn, 1 error"), "{text}");
    assert!(
        text.contains("screen: not kept, outside the geometry or 4 MiB bound"),
        "{text}"
    );
    assert!(!text.contains("larger than"), "{text}");
    assert!(!text.contains("no credentials"), "{text}");
    assert!(
        text.contains("Titles, end text, plugin sources, and plugin log messages are omitted."),
        "{text}"
    );
    assert!(!text.contains("removed"), "{text}");
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

fn worktree_run(fixture: &Fixture, title: Option<&str>, base: Option<&str>) -> Op {
    Op::Thread(ThreadOp::Run(Run {
        project_id: fixture.project.clone(),
        program: program().into(),
        title: title.map(str::to_owned),
        environment: EnvironmentSpec::Worktree {
            base_branch: base.map(str::to_owned),
            branch: None,
        },
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }))
}

fn commit_file(repo: &Path, rel: &str, body: &str) {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, body).unwrap();
    git_in(repo, &["add", "--", rel]);
    git_in(
        repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "script",
        ],
    );
}

fn tracks_upstream(path: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .status()
        .is_ok_and(|status| status.success())
}

fn repo_dir(fixture: &Fixture) -> std::path::PathBuf {
    dunce::canonicalize(fixture.dir.path()).unwrap()
}

struct GitTrack {
    cwd: std::path::PathBuf,
}

impl GitTrack {
    fn install(cwd: &Path) -> (std::path::PathBuf, Self) {
        let log = cwd.join("git-argv.log");
        let program = cwd.join("git-track.sh");
        let real = std::process::Command::new("git")
            .arg("--exec-path")
            .output()
            .unwrap();
        assert!(real.status.success(), "git --exec-path");
        let git_exec = std::path::PathBuf::from(String::from_utf8(real.stdout).unwrap().trim());
        let git = git_exec.join("git");
        let git = if git.is_file() { git } else { which_git() };
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexec '{}' \"$@\"\n",
                log.display(),
                git.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut permissions = std::fs::metadata(&program).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&program, permissions).unwrap();
        }
        crate::git::set_program(cwd, "git", Some(program));
        (
            log,
            Self {
                cwd: cwd.to_path_buf(),
            },
        )
    }
}

fn which_git() -> std::path::PathBuf {
    let output = std::process::Command::new("which")
        .arg("git")
        .output()
        .unwrap();
    assert!(output.status.success(), "which git");
    std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

impl Drop for GitTrack {
    fn drop(&mut self) {
        crate::git::set_program(&self.cwd, "git", None);
    }
}

fn worktree_count(data: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(data.join("worktrees")) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| std::fs::read_dir(entry.path()).ok())
        .flat_map(std::iter::Iterator::flatten)
        .count()
}

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

#[cfg(unix)]
async fn stop_archive_and_settle(
    thread: &ThreadId,
    environment_id: &EnvironmentId,
    cx: &mut TestAppContext,
) -> Environment {
    op(Op::Thread(ThreadOp::Stop { id: thread.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(thread, ThreadStatus::Idle, cx).await;
    op(Op::Thread(ThreadOp::Archive { id: thread.clone() }), cx)
        .await
        .unwrap();
    let mut folder = environment_of(environment_id, cx).await;
    for _ in 0..40 {
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        folder = environment_of(environment_id, cx).await;
        if folder.status == EnvironmentStatus::Error || !folder.path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    folder
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

#[cfg(unix)]
fn error_rows(fixture: &Fixture) -> Vec<Environment> {
    kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3"))
        .unwrap()
        .environments(&fixture.project)
        .unwrap()
        .into_iter()
        .filter(|environment| environment.status == EnvironmentStatus::Error)
        .collect()
}

async fn folder_plugin_fixture(cx: &mut TestAppContext) -> Fixture {
    Fixture::seeded(cx, |store| {
        store
            .insert_plugin(&Plugin {
                id: PluginId("demo-plugin".into()),
                root: Path::new("demo-plugin").to_path_buf(),
                source: "test".into(),
                commit: None,
                enabled: true,
                approved: json!({ "folders": { "notes": "Notes" } }),
                error: None,
            })
            .unwrap();
    })
    .await
}

// The first poll queues the write without running the database worker, so tests can revoke its permit.
fn queue_folder_write(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    permit: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
) -> impl std::future::Future<Output = crate::database::Result<()>> + use<> {
    fixture.workspace.read_with(cx, |workspace, _| {
        workspace.commit_plugin_config(
            PluginId("demo-plugin".into()),
            "notes".into(),
            json!("/notes"),
            Some("/notes".into()),
            crate::Permit::new(1, permit),
        )
    })
}

fn assert_folder_write_landed(fixture: &Fixture) {
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    let id = PluginId("demo-plugin".into());
    assert_eq!(
        store.plugin_value(&id, "notes").unwrap(),
        Some(json!("/notes"))
    );
    let row = store.plugins().unwrap().remove(0);
    assert_eq!(row.approved["paths"]["notes"], json!("/notes"));
}

fn assert_folder_write_absent(fixture: &Fixture) {
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    let id = PluginId("demo-plugin".into());
    assert_eq!(store.plugin_value(&id, "notes").unwrap(), None);
    let row = store.plugins().unwrap().remove(0);
    assert!(row.approved["paths"]["notes"].is_null());
}

#[gpui_kit::test]
async fn a_queued_plugin_write_is_refused_once_its_permit_is_cleared(cx: &mut TestAppContext) {
    let fixture = folder_plugin_fixture(cx).await;
    let permit = std::sync::Arc::new(std::sync::Mutex::new(Some(1)));
    let mut write = std::pin::pin!(queue_folder_write(cx, &fixture, permit.clone()));
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    assert!(write.as_mut().poll(&mut context).is_pending());
    *permit.lock().unwrap() = None;
    assert!(cx.foreground_executor().block_test(write).is_err());
    assert_folder_write_absent(&fixture);
}

#[gpui_kit::test]
async fn a_queued_plugin_write_lands_while_its_permit_holds(cx: &mut TestAppContext) {
    let fixture = folder_plugin_fixture(cx).await;
    let permit = std::sync::Arc::new(std::sync::Mutex::new(Some(1)));
    let mut write = std::pin::pin!(queue_folder_write(cx, &fixture, permit));
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    assert!(write.as_mut().poll(&mut context).is_pending());
    cx.foreground_executor().block_test(write).unwrap();
    assert_folder_write_landed(&fixture);
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
async fn a_browser_command_needs_a_window(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let status = Op::Browser(kg_domain::BrowserOp::Status {
        thread_id: ThreadId("thr_browser".into()),
    });
    assert!(!crate::dispatch::serialized(&status));
    assert!(matches!(
        op(status, cx).await,
        Err(OpError::Failed(message)) if message == UI_UNAVAILABLE
    ));
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
async fn stopped_worktree(
    fixture: &Fixture,
    title: &str,
    cx: &mut TestAppContext,
) -> (ThreadId, std::path::PathBuf) {
    let OpResult::Thread(thread) = op(worktree_run(fixture, Some(title), None), cx)
        .await
        .unwrap()
    else {
        panic!("thread")
    };
    let folder = environment_of(thread.environment_id.as_ref().unwrap(), cx)
        .await
        .path;
    op(
        Op::Thread(ThreadOp::Stop {
            id: thread.id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    wait_for_status(&thread.id, ThreadStatus::Idle, cx).await;
    (thread.id, folder)
}

#[cfg(unix)]
fn wait_until_gone(path: &Path, cx: &mut TestAppContext) {
    for _ in 0..400 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if !path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("{} was never removed", path.display());
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
async fn diff_revision_changes_for_binary_large_and_truncated_files(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    std::fs::write(root.join("patch.txt"), "a".repeat(600_000)).unwrap();
    git_in(root, &["add", "patch.txt"]);
    commit(root, &["commit", "-m", "base"]);
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    let show = |path: &str| {
        Op::Diff(DiffOp::Show {
            query: DiffQuery {
                thread_id: started.id.clone(),
                scope: DiffScope::Uncommitted,
                base: None,
                rev: None,
            },
            path: path.to_owned(),
        })
    };
    for (path, first, second) in [
        ("binary.bin", vec![0, 1, 2], vec![0, 1, 3]),
        ("large.txt", vec![b'x'; 1_100_000], vec![b'y'; 1_100_000]),
        ("patch.txt", vec![b'b'; 600_000], vec![b'c'; 600_000]),
    ] {
        std::fs::write(root.join(path), first).unwrap();
        let OpResult::DiffFile(before) = op(show(path), cx).await.unwrap() else {
            panic!("file")
        };
        std::fs::write(root.join(path), second).unwrap();
        let OpResult::DiffFile(after) = op(show(path), cx).await.unwrap() else {
            panic!("file")
        };
        assert_ne!(before.revision, after.revision, "{path}");
        match path {
            "binary.bin" => assert!(before.binary),
            "large.txt" => assert!(before.too_large),
            _ => assert!(before.truncated),
        }
    }
}
