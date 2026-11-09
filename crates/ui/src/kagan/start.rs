use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, FocusHandle, Subscription,
    Task, WeakEntity, Window, actions,
};
use kg_core::Workspace;
use kg_domain::{
    BranchList, EnvironmentId, EnvironmentOp, Op, OpResult, PinnedSetup, ProjectId, ProjectKind,
    ThreadId, ThreadOp,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{Kagan, Shown};

pub(super) const PLUGIN_PROMPT: &str = "Build a kagan plugin that ";
const NO_AGENT: &str = "No agent is installed, so kagan cannot start a plugin thread. Install Claude Code or Codex, then try again.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartFolder {
    Project,
    Worktree,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum BranchPick {
    #[default]
    Current,
    Named(String),
    NewFrom(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProjectIntent {
    #[default]
    Current,
    New,
    Checkout,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Panel {
    #[default]
    Closed,
    Open,
}

#[derive(Default)]
enum Branches {
    #[default]
    Loading,
    Loaded(BranchList),
    Failed(String),
}

fn setup_announcement(setup: &PinnedSetup) -> Option<&'static str> {
    match setup.mode.as_deref() {
        Some("120000") => Some(".kagan/setup is a link, so kagan will not run it."),
        Some("100644" | "100755") => {
            Some("Runs .kagan/setup in the new worktree before the program starts.")
        }
        _ => None,
    }
}

fn setup_script(setup: &PinnedSetup) -> Option<(&str, bool)> {
    (!setup.text.is_empty()).then_some((setup.text.as_str(), setup.truncated))
}

// A new thread that runs in the folder of an ended thread. Its folder is not chosen here.
#[derive(Clone)]
pub(super) struct Reuse {
    thread: ThreadId,
    environment: EnvironmentId,
}

#[derive(Default)]
pub(super) struct StartState {
    reuse: Option<Reuse>,
    // The data folder's worktrees, as the line under "Runs in" shows it. Set once when the screen opens.
    pub(super) worktree_root: String,
    folder: Option<StartFolder>,
    pick: BranchPick,
    intent: ProjectIntent,
    panel: Panel,
    pub(super) branch_focus: Option<FocusHandle>,
    branch_query: Option<Entity<InputState>>,
    branch_watch: Option<Subscription>,
    branches: Branches,
    setup_epoch: u64,
    setup_task: Option<Task<()>>,
    setup: Option<PinnedSetup>,
    setup_open: bool,
    setup_failure: Option<String>,
    starting: bool,
    load: Option<Task<()>>,
    run: Option<Task<()>>,
    pub(super) plugin_opening: bool,
    pub(super) plugin_start: Option<Task<()>>,
    pub(super) plugin_thread: Option<kg_domain::ThreadId>,
}

impl StartState {
    pub(super) fn reset(&mut self) {
        self.reuse = None;
        self.folder = None;
        self.pick = BranchPick::Current;
        self.intent = ProjectIntent::Current;
        self.panel = Panel::Closed;
        self.branch_query = None;
        self.branch_watch = None;
        self.branches = Branches::Loading;
        self.setup_epoch = self.setup_epoch.wrapping_add(1);
        self.setup_task = None;
        self.setup = None;
        self.setup_open = false;
        self.setup_failure = None;
    }
}

impl StartState {
    // The ended thread the screen came from, so leaving the screen returns to it.
    pub(super) fn take_reuse_thread(&mut self) -> Option<ThreadId> {
        self.reuse.take().map(|reuse| reuse.thread)
    }
}

fn base_of(branches: &BranchList) -> Option<String> {
    branches.current.clone().or_else(|| branches.head.clone())
}

fn base_label(branches: &BranchList) -> String {
    match &branches.current {
        Some(current) => current.clone(),
        None => branches
            .head
            .as_deref()
            .map_or_else(|| "HEAD".into(), |head| head.chars().take(7).collect()),
    }
}

fn script_changed(failure: &str) -> bool {
    failure.contains("Start will use the version shown on screen now")
}

async fn pinned_setup(project: ProjectId, rev: String, cx: &mut AsyncApp) -> Option<PinnedSetup> {
    let read = cx.update(|cx| {
        Workspace::dispatch(
            Op::Environment(EnvironmentOp::Setup {
                project_id: project,
                rev,
            }),
            cx,
        )
    });
    match read.await {
        Ok(OpResult::Setup(setup)) => setup,
        _ => None,
    }
}

fn project_path(project: &ProjectId, cx: &App) -> Option<PathBuf> {
    Workspace::global(cx)
        .read(cx)
        .project(project)
        .map(|project| project.read(cx).record().path.clone())
}

fn project_kind(project: &ProjectId, cx: &App) -> Option<ProjectKind> {
    Workspace::global(cx)
        .read(cx)
        .project(project)
        .map(|project| project.read(cx).record().kind)
}

// Waits for the new plugin thread to show its prompt, then pastes the prompt once the screen
// settles. After 15 seconds, the window offers the prompt to copy instead.
async fn follow_plugin_prompt(
    this: WeakEntity<Kagan>,
    window: AnyWindowHandle,
    id: ThreadId,
    cx: &mut AsyncApp,
) {
    let started = Instant::now();
    let mut previous = None;
    let mut changed = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        let grid = cx.update(|cx| Workspace::global(cx).read(cx).grid(&id, None));
        if let Some(grid) = grid
            && grid
                .lines
                .iter()
                .flatten()
                .any(|cell| !cell.c.is_whitespace())
        {
            if previous.as_ref() != Some(&grid) {
                changed = Instant::now();
                previous = Some(grid);
            } else if changed.elapsed() >= Duration::from_secs(1) {
                let task = cx.update(|cx| {
                    Workspace::dispatch(
                        Op::Thread(ThreadOp::Paste {
                            id: id.clone(),
                            text: Some(PLUGIN_PROMPT.into()),
                            file: None,
                            terminal: None,
                        }),
                        cx,
                    )
                });
                if task.await.is_ok() {
                    _ = this.update(cx, |this, _| this.cancel_plugin_start());
                    return;
                }
                break;
            }
        }
    }
    _ = cx.update_window(window, |_, _, cx| {
        _ = this.update(cx, |this, cx| {
            this.plugin_prompt_notice = Some(id);
            this.cancel_plugin_start();
            cx.notify();
        });
    });
}

mod branches;
mod folder;
mod menu;
mod plugin;
mod render;
mod setup;

pub(super) use menu::{BRANCH_CONTEXT, CloseBranchMenu};

pub(crate) const START_CONTEXT: &str = "NewThreadScreen";

actions!(kagan, [LeaveStart]);

impl Kagan {
    // Escape and Cancel both leave the screen here. A branch menu open over it closes first.
    pub(super) fn leave_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_branch_menu(window, cx) {
            cx.notify();
            return;
        }
        if let Some(thread) = self.start.take_reuse_thread() {
            self.select(thread, window, cx);
            cx.notify();
            return;
        }
        self.show_work(Shown::Nothing, window, cx);
        window.focus(&self.focus, cx);
        self.tell_host(window, cx);
        cx.notify();
    }

    // The script failed and was not changed since, so Start runs the setup again.
    pub(super) fn setup_retry_pending(&self) -> bool {
        self.start
            .setup_failure
            .as_deref()
            .is_some_and(|failure| !script_changed(failure))
    }

    pub(super) fn start_heading(&self, project_name: &str, cx: &App) -> String {
        match &self.start.reuse {
            Some(reuse) => {
                let title = Workspace::global(cx)
                    .read(cx)
                    .thread(&reuse.thread)
                    .map(|thread| thread.read(cx).record().title.clone())
                    .unwrap_or_default();
                format!("New thread beside “{title}”")
            }
            None => format!("New thread in {project_name}"),
        }
    }

    pub(super) fn remember_worktree_root(&mut self, cx: &App) {
        let root = Workspace::global(cx).read(cx).data_dir().join("worktrees");
        self.start.worktree_root = folder::shown_path(&root);
    }

    pub(in crate::kagan) fn new_thread_here(
        &mut self,
        thread: &ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = Workspace::global(cx)
            .read(cx)
            .thread(thread)
            .map(|thread| thread.read(cx).record().clone())
        else {
            return;
        };
        let Some(environment) = record.environment_id.clone() else {
            return;
        };
        self.show_start(record.project_id.clone(), window, cx);
        self.start.reuse = Some(Reuse {
            thread: thread.clone(),
            environment,
        });
        cx.notify();
    }
}
