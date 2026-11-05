#![allow(clippy::unwrap_used, clippy::expect_used)]

mod agent;
#[cfg(unix)]
mod details;
mod diagnostics;
mod diff;
mod environment;
#[cfg(unix)]
mod extra;
mod guide;
mod plugin;
mod program;
mod project;
#[cfg(unix)]
mod resume;
mod screen;
mod search;
mod settings;
mod terminal;
mod thread;
mod turn;
mod update;
mod workspace;
mod worktree;

use std::path::Path;

use gpui_kit::{Entity, TestAppContext};

use kg_domain::*;

use serde_json::json;

use crate::Workspace;

use crate::database::Result;

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
            }],
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
