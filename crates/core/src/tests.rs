#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test stops at the first broken step"
)]

mod agent;
#[cfg(unix)]
mod details;
mod diagnostics;
mod diff;
mod environment;
#[cfg(unix)]
mod extra;
mod guide;
#[cfg(unix)]
mod notify;
mod plugin;
mod preview;
mod program;
mod project;
mod read;
#[cfg(unix)]
mod resume;
mod screen;
mod search;
mod settings;
mod skill;
mod skill_names;
mod terminal;
mod thread;
mod turn;
mod update;
mod workspace;
mod worktree;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gpui_kit::{Entity, TestAppContext};

use kg_domain::*;

use serde_json::json;

use crate::Workspace;

use crate::host::database::Result;

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
        let workspace = open_at(&dir, cx, |_| {}).await.unwrap();
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
        let workspace = open_at(&dir, cx, |_| {}).await.unwrap();
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
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        }))
    }
}

// Opens the app on `dir`, with every program's folder in it, before the restart's resume runs.
fn open_at(
    dir: &tempfile::TempDir,
    cx: &mut TestAppContext,
    prepare: impl FnOnce(&mut Workspace) + 'static,
) -> gpui_kit::Task<Result<gpui_kit::Entity<Workspace>>> {
    let home = dir.path().to_path_buf();
    cx.update(|cx| {
        Workspace::open_with(
            dir.path().into(),
            std::env::current_exe().unwrap(),
            Some("/tmp/kagan-test.sock".into()),
            move |workspace| {
                insert_homes(&mut workspace.homes, &home);
                prepare(workspace);
            },
            cx,
        )
    })
}

// Points every program's folder into the test's folder, so no test reads the person's own.
fn insert_homes(homes: &mut BTreeMap<&'static str, PathBuf>, dir: &Path) {
    homes.insert("CLAUDE_CONFIG_DIR", dir.join("claude"));
    homes.insert("CODEX_HOME", dir.join("codex-home"));
    homes.insert("KIMI_CODE_HOME", dir.join("kimi-home"));
    homes.insert("CURSOR_CONFIG_DIR", dir.join("cursor"));
}

fn program() -> &'static str {
    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

async fn op(op: Op, cx: &mut TestAppContext) -> Result<OpResult> {
    cx.update(|cx| Workspace::dispatch(op, cx)).await
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
                role: None,
            }],
            fields: None,
            created_at: Timestamp(1),
        })
        .unwrap();
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

fn personal_run(fixture: &Fixture, title: &str, environment: EnvironmentSpec) -> Op {
    Op::Thread(ThreadOp::Run(Run {
        project_id: fixture.project.clone(),
        program: program().into(),
        title: Some(title.into()),
        environment,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
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
        fixture.workspace.update(cx, |workspace, _| {
            workspace
                .holds
                .insert(crate::workspace::Hold::Removal, hold)
        });
    });
    release
}

fn hold_delete(fixture: &Fixture, cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        fixture.workspace.update(cx, |workspace, _| {
            workspace.holds.insert(crate::workspace::Hold::Delete, hold)
        });
    });
    release
}

fn hold_run(fixture: &Fixture, cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        fixture.workspace.update(cx, |workspace, _| {
            workspace.holds.insert(crate::workspace::Hold::Run, hold)
        });
    });
    release
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
            role: None,
        },
        InteractionOption {
            id: "no".into(),
            label: "No".into(),
            role: None,
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

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_rejects_duplicate_select_choice_ids(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let error = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Pick".into(),
            options: ask_options(),
            fields: Some(vec![kg_domain::AskField::Select {
                id: "mode".into(),
                label: "Mode".into(),
                choices: vec![
                    InteractionOption {
                        id: "a".into(),
                        label: "A".into(),
                        role: None,
                    },
                    InteractionOption {
                        id: "a".into(),
                        label: "Also A".into(),
                        role: None,
                    },
                ],
                default: None,
                required: false,
            }]),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("select choice id a is repeated"),
        "{error}"
    );
}

#[cfg(unix)]
async fn ask_refused(id: &ThreadId, fields: Vec<AskField>, cx: &mut TestAppContext) -> String {
    op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some(fields),
        }),
        cx,
    )
    .await
    .unwrap_err()
    .to_string()
}

fn text_field(id: &str, label: &str) -> AskField {
    AskField::String {
        id: id.into(),
        label: label.into(),
        placeholder: None,
        default: None,
    }
}

fn select_field(choices: Vec<InteractionOption>, default: Option<&str>) -> AskField {
    AskField::Select {
        id: "mode".into(),
        label: "Mode".into(),
        choices,
        default: default.map(Into::into),
        required: false,
    }
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_refuses_repeated_field_ids_text_over_its_caps_and_a_default_outside_the_choices(
    cx: &mut TestAppContext,
) {
    let (_fixture, id) = sleeping(cx).await;
    let long_id = "i".repeat(65);
    let long_label = "l".repeat(129);
    let long_text = "t".repeat(4097);
    let cases = [
        (
            vec![text_field("same", "One"), text_field("same", "Two")],
            "ask field id same is repeated",
        ),
        (
            vec![text_field(&long_id, "Name")],
            "field id is at most 64 characters",
        ),
        (
            vec![text_field("name", &long_label)],
            "field label is at most 128 characters",
        ),
        (
            vec![AskField::String {
                id: "name".into(),
                label: "Name".into(),
                placeholder: Some(long_text.clone()),
                default: None,
            }],
            "ask placeholder is at most 4096 characters",
        ),
        (
            vec![AskField::String {
                id: "name".into(),
                label: "Name".into(),
                placeholder: None,
                default: Some(long_text.clone()),
            }],
            "ask default is at most 4096 characters",
        ),
        (
            vec![select_field(
                vec![InteractionOption {
                    id: "fast".into(),
                    label: "Fast".into(),
                    role: None,
                }],
                Some("slow"),
            )],
            "a select field default must be one of its choices",
        ),
    ];
    for (fields, expected) in cases {
        let error = ask_refused(&id, fields, cx).await;
        assert!(error.contains(expected), "{error} does not name {expected}");
    }
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_refuses_more_fields_or_choices_than_the_caps_and_takes_a_full_form(
    cx: &mut TestAppContext,
) {
    let (_fixture, id) = sleeping(cx).await;
    let fields: Vec<_> = (0..33)
        .map(|n| text_field(&format!("f{n:02}"), "Field"))
        .collect();
    let error = ask_refused(&id, fields, cx).await;
    assert!(error.contains("at most 32 fields"), "{error}");

    let choices: Vec<_> = (0..33)
        .map(|n| InteractionOption {
            id: format!("c{n:02}"),
            label: "Choice".into(),
            role: None,
        })
        .collect();
    let error = ask_refused(&id, vec![select_field(choices, None)], cx).await;
    assert!(error.contains("at most 32 choices"), "{error}");

    let fields: Vec<_> = (0..31)
        .map(|n| text_field(&format!("f{n:02}"), "Field"))
        .collect();
    let choices: Vec<_> = (0..32)
        .map(|n| InteractionOption {
            id: format!("c{n:02}"),
            label: "Choice".into(),
            role: None,
        })
        .collect();
    op(
        Op::Thread(ThreadOp::Ask {
            id,
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some([fields, vec![select_field(choices, None)]].concat()),
        }),
        cx,
    )
    .await
    .unwrap();
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_answer_refuses_a_missing_unknown_or_wrong_kind_field(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let OpResult::Text(interaction) = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some(vec![
                text_field("name", "Name"),
                AskField::Boolean {
                    id: "notify".into(),
                    label: "Notify".into(),
                    default: None,
                },
            ]),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("ask");
    };
    let cases = [
        (
            std::collections::BTreeMap::from([("notify".to_owned(), json!(true))]),
            "ask answer is missing field name",
        ),
        (
            std::collections::BTreeMap::from([
                ("name".to_owned(), json!("Ada")),
                ("notify".to_owned(), json!(true)),
                ("surprise".to_owned(), json!(1)),
            ]),
            "ask answer has unknown field surprise",
        ),
        (
            std::collections::BTreeMap::from([
                ("name".to_owned(), json!(1)),
                ("notify".to_owned(), json!(true)),
            ]),
            "ask answer field name has the wrong kind of value",
        ),
    ];
    for (values, expected) in cases {
        let error = cx
            .update(|cx| {
                Workspace::answer_fields(
                    &id,
                    PendingInteractionId(interaction.clone()),
                    "yes".into(),
                    values,
                    cx,
                )
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
    assert_eq!(pending(&id, cx).await.len(), 1, "the question stays");
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_answer_requires_a_whole_number(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let OpResult::Text(interaction) = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some(vec![kg_domain::AskField::Number {
                id: "retries".into(),
                label: "Retries".into(),
                default: Some(1),
            }]),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("ask");
    };
    let mut values = std::collections::BTreeMap::new();
    values.insert("retries".into(), serde_json::json!(1.5));
    let error = cx
        .update(|cx| {
            Workspace::answer_fields(
                &id,
                PendingInteractionId(interaction),
                "yes".into(),
                values,
                cx,
            )
        })
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ask answer field retries has the wrong kind of value"),
        "{error}"
    );
}

#[gpui_kit::test]
#[cfg(unix)]
async fn a_required_select_refuses_none_and_takes_a_choice(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let fields = serde_json::from_value(json!([{
        "type": "select",
        "id": "mode",
        "label": "Mode",
        "required": true,
        "choices": [{ "id": "fast", "label": "Fast" }]
    }]))
    .unwrap();
    let OpResult::Text(interaction) = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some(fields),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("ask");
    };
    let mut values = std::collections::BTreeMap::new();
    values.insert("mode".into(), serde_json::Value::Null);
    let error = cx
        .update(|cx| {
            Workspace::answer_fields(
                &id,
                PendingInteractionId(interaction.clone()),
                "yes".into(),
                values,
                cx,
            )
        })
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ask answer field mode needs a choice"),
        "{error}"
    );
    let mut values = std::collections::BTreeMap::new();
    values.insert("mode".into(), json!("fast"));
    cx.update(|cx| {
        Workspace::answer_fields(
            &id,
            PendingInteractionId(interaction),
            "yes".into(),
            values,
            cx,
        )
    })
    .await
    .unwrap();
}

#[gpui_kit::test]
#[cfg(unix)]
async fn ask_answer_accepts_none_for_a_select(cx: &mut TestAppContext) {
    let (_fixture, id) = sleeping(cx).await;
    let OpResult::Text(interaction) = op(
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Ship it?".into(),
            options: ask_options(),
            fields: Some(vec![kg_domain::AskField::Select {
                id: "mode".into(),
                label: "Mode".into(),
                choices: vec![InteractionOption {
                    id: "fast".into(),
                    label: "Fast".into(),
                    role: None,
                }],
                default: None,
                required: false,
            }]),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("ask");
    };
    let mut values = std::collections::BTreeMap::new();
    values.insert("mode".into(), serde_json::Value::Null);
    cx.update(|cx| {
        Workspace::answer_fields(
            &id,
            PendingInteractionId(interaction),
            "yes".into(),
            values.clone(),
            cx,
        )
    })
    .await
    .unwrap();
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
fn install_plugin(dir: &Path, enabled: bool) {
    let root = dir.join("demo-plugin");
    let skill = root.join("skills/demo");
    std::fs::create_dir_all(&skill).unwrap();
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
            skills: vec!["skills/demo".into()],
            error: None,
        })
        .unwrap();
}

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
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{out}/args'\nls -1 .agents/skills > '{out}/agents-skills' 2>/dev/null\nls -1 .opencode/skills > '{out}/opencode-skills' 2>/dev/null\nenv > '{out}/env.tmp' && mv '{out}/env.tmp' '{out}/env'\nsleep 30\n",
            out = out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

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
        .filter(|line| !line.is_empty())
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
fn claude_line(run: &Path) -> String {
    format!("{} --person-arg", fake_agent(run, "claude").display())
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
fn fake_program(dir: &std::path::Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
        quiet: false,
        notify: kg_domain::ThreadNotify::Default,
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
                    .any(|row| &row.thread_id == id && row.screen_kept)
            })
            && store
                .screen_replay(id)
                .is_ok_and(|replay| replay.is_some_and(|replay| !replay.is_empty()))
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
        skills: Vec::new(),
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
    assert!(text.contains("log: not read, no plugin host"), "{text}");
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
        quiet: false,
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
        crate::host::git::set_program(cwd, "git", Some(program));
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
        crate::host::git::set_program(&self.cwd, "git", None);
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
                skills: Vec::new(),
                error: None,
            })
            .unwrap();
    })
    .await
}

fn queue_folder_write(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    permit: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
) -> impl std::future::Future<Output = crate::host::database::Result<()>> + use<> {
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
const TOOL_THREAD: &str = "thr_tool";

async fn tool_thread(cx: &mut TestAppContext) -> (Fixture, ThreadId) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let fixture = fixture
        .restart(cx, move |store| {
            store
                .put_thread(&idle_thread(TOOL_THREAD, project))
                .unwrap();
        })
        .await;
    (fixture, ThreadId(TOOL_THREAD.into()))
}

fn set_tool_host(
    fixture: &Fixture,
    cx: &mut TestAppContext,
    reply: impl Fn(&str) -> Result<OpResult> + 'static,
) {
    let reply = std::rc::Rc::new(reply);
    fixture.workspace.update(cx, |workspace, _| {
        workspace.set_plugin_host(std::rc::Rc::new(move |request, _| {
            let result = match request {
                crate::PluginRequest::CallTool { name, .. } => reply(&name),
                _ => Err(OpError::Failed("unexpected plugin request".into())),
            };
            gpui_kit::Task::ready(result)
        }));
    });
}

async fn call_tool(
    id: &ThreadId,
    name: &str,
    input: serde_json::Value,
    cx: &mut TestAppContext,
) -> Result<OpResult> {
    op(
        Op::Thread(ThreadOp::CallTool {
            id: id.clone(),
            name: name.into(),
            input,
        }),
        cx,
    )
    .await
}

async fn tool_pair(id: &ThreadId, cx: &mut TestAppContext) -> (String, String, ToolOutcome) {
    let kinds: Vec<EventKind> = events(id, cx)
        .await
        .into_iter()
        .map(|event| event.kind)
        .collect();
    let [
        EventKind::ToolStarted { id: started, name },
        EventKind::ToolEnded { id: ended, status },
    ] = kinds.as_slice()
    else {
        panic!("tool events: {kinds:?}");
    };
    assert_eq!(started, ended);
    (name.clone(), started.clone(), *status)
}

#[gpui_kit::test]
async fn a_plugin_tool_call_writes_started_then_ended_ok(cx: &mut TestAppContext) {
    let (fixture, id) = tool_thread(cx).await;
    set_tool_host(&fixture, cx, |name| {
        assert_eq!(name, "editor_open");
        Ok(OpResult::ToolReply {
            text: "opened".into(),
            is_error: false,
        })
    });
    let reply = call_tool(&id, "editor_open", json!({}), cx).await.unwrap();
    assert_eq!(
        reply,
        OpResult::ToolReply {
            text: "opened".into(),
            is_error: false,
        }
    );
    let (name, started, status) = tool_pair(&id, cx).await;
    assert_eq!(name, "editor_open");
    assert_eq!(status, ToolOutcome::Ok);
    assert!(started.starts_with("tool_"), "{started}");
}

#[gpui_kit::test]
async fn a_failing_tool_call_writes_ended_error(cx: &mut TestAppContext) {
    let (fixture, id) = tool_thread(cx).await;
    set_tool_host(&fixture, cx, |_| {
        Err(OpError::Invalid("the plugin refused".into()))
    });
    let error = call_tool(&id, "editor_open", json!({}), cx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("refused"), "{error}");
    let (name, _, status) = tool_pair(&id, cx).await;
    assert_eq!(name, "editor_open");
    assert_eq!(status, ToolOutcome::Error);
}

#[gpui_kit::test]
async fn a_plugin_error_reply_writes_ended_error(cx: &mut TestAppContext) {
    let (fixture, id) = tool_thread(cx).await;
    set_tool_host(&fixture, cx, |_| {
        Ok(OpResult::ToolReply {
            text: "no luck".into(),
            is_error: true,
        })
    });
    let reply = call_tool(&id, "editor_open", json!({}), cx).await.unwrap();
    assert_eq!(
        reply,
        OpResult::ToolReply {
            text: "no luck".into(),
            is_error: true,
        }
    );
    let (name, _, status) = tool_pair(&id, cx).await;
    assert_eq!(name, "editor_open");
    assert_eq!(status, ToolOutcome::Error);
}

#[gpui_kit::test]
async fn two_tool_calls_get_distinct_paired_ids(cx: &mut TestAppContext) {
    let (fixture, id) = tool_thread(cx).await;
    set_tool_host(&fixture, cx, |_| {
        Ok(OpResult::ToolReply {
            text: "done".into(),
            is_error: false,
        })
    });
    call_tool(&id, "editor_open", json!({}), cx).await.unwrap();
    call_tool(&id, "editor_open", json!({}), cx).await.unwrap();
    let kinds: Vec<EventKind> = events(&id, cx)
        .await
        .into_iter()
        .map(|event| event.kind)
        .collect();
    let [
        EventKind::ToolStarted {
            id: first_start, ..
        },
        EventKind::ToolEnded { id: first_end, .. },
        EventKind::ToolStarted {
            id: second_start, ..
        },
        EventKind::ToolEnded { id: second_end, .. },
    ] = kinds.as_slice()
    else {
        panic!("tool events: {kinds:?}");
    };
    assert_eq!(first_start, first_end);
    assert_eq!(second_start, second_end);
    assert_ne!(first_start, second_start);
}

#[gpui_kit::test]
async fn a_tool_with_no_host_writes_both_rows_with_error(cx: &mut TestAppContext) {
    let (_fixture, id) = tool_thread(cx).await;
    let error = call_tool(&id, "editor_open", json!({}), cx)
        .await
        .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
    let (name, _, status) = tool_pair(&id, cx).await;
    assert_eq!(name, "editor_open");
    assert_eq!(status, ToolOutcome::Error);
}

#[gpui_kit::test]
async fn a_missing_thread_writes_no_tool_events(cx: &mut TestAppContext) {
    let (fixture, _) = tool_thread(cx).await;
    let missing = ThreadId("thr_missing".into());
    let error = call_tool(&missing, "editor_open", json!({}), cx)
        .await
        .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert!(
        store
            .events_after(&missing, None, None, Order::Asc)
            .unwrap()
            .is_empty()
    );
}

#[gpui_kit::test]
async fn skill_load_writes_tool_events(cx: &mut TestAppContext) {
    let (fixture, id) = tool_thread(cx).await;
    let folder = fixture.dir.path().join("skills").join("probe");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        "---\nname: probe\ndescription: A probe.\n---\n\nBody.\n",
    )
    .unwrap();
    let reply = call_tool(&id, "skill_load", json!({ "name": "probe" }), cx)
        .await
        .unwrap();
    assert!(matches!(
        reply,
        OpResult::ToolReply {
            is_error: false,
            ..
        }
    ));
    let (name, _, status) = tool_pair(&id, cx).await;
    assert_eq!(name, "skill_load");
    assert_eq!(status, ToolOutcome::Ok);
}

#[gpui_kit::test]
async fn a_failing_skill_load_writes_ended_error(cx: &mut TestAppContext) {
    let (_fixture, id) = tool_thread(cx).await;
    let error = call_tool(&id, "skill_load", json!({ "name": "missing" }), cx)
        .await
        .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
    let (name, _, status) = tool_pair(&id, cx).await;
    assert_eq!(name, "skill_load");
    assert_eq!(status, ToolOutcome::Error);
}
