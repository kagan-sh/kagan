use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, FocusHandle, Subscription,
    Task, WeakEntity, Window, actions,
};
use kg_core::Workspace;
use kg_domain::{
    BranchList, EnvironmentId, EnvironmentOp, EnvironmentStatus, Op, OpError, OpResult,
    PinnedSetup, ProjectId, ProjectKind, SetupFile, ThreadId, ThreadOp,
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
    setup.mode.map(|mode| match mode {
        SetupFile::Link => ".kagan/setup is a link, so kagan will not run it.",
        SetupFile::Script => "Runs .kagan/setup in the new worktree before the program starts.",
    })
}

fn setup_script(setup: &PinnedSetup) -> Option<(&str, bool)> {
    (!setup.text.is_empty()).then_some((setup.text.as_str(), setup.truncated))
}

// A new thread that runs in the folder of an ended thread. Its folder is not chosen here.
#[derive(Clone)]
pub(super) struct Reuse {
    project: ProjectId,
    thread: ThreadId,
    environment: EnvironmentId,
    title: String,
    name: String,
    shown: String,
}

impl Reuse {
    // The ended thread's folder, when core would run a new thread in it: the folder must exist,
    // belong to the thread's project, and be ready.
    pub(super) fn of(thread: &ThreadId, cx: &App) -> Option<Self> {
        let workspace = Workspace::global(cx).read(cx);
        let record = workspace.thread(thread)?.read(cx).record().clone();
        let environment_id = record.environment_id.clone()?;
        let environment = workspace.environment(&environment_id)?.read(cx).record();
        if environment.project_id != record.project_id
            || environment.status != EnvironmentStatus::Ready
        {
            return None;
        }
        let name = environment
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Some(Self {
            project: record.project_id,
            thread: thread.clone(),
            environment: environment_id,
            title: record.title,
            name,
            shown: folder::shown_path(&environment.path),
        })
    }
}

/// Whether a thread runs in the chosen project's folder, as of the last window refresh.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum FolderUse {
    #[default]
    Free,
    Busy,
}

impl FolderUse {
    pub(super) fn of(busy: bool) -> Self {
        if busy { Self::Busy } else { Self::Free }
    }

    pub(super) fn is_busy(self) -> bool {
        self == Self::Busy
    }
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
    pub(super) segments_focus: Option<FocusHandle>,
    pub(super) form_focus: Option<FocusHandle>,
    branch_query: Option<Entity<InputState>>,
    branch_watch: Option<Subscription>,
    branches: Branches,
    pub(super) busy: FolderUse,
    setup_epoch: u64,
    setup_task: Option<Task<()>>,
    setup: Option<PinnedSetup>,
    setup_open: bool,
    setup_failure: Option<OpError>,
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
// The form's controls. Enter is theirs (a focused button activates itself), so this context
// stops the screen's Enter binding from reaching them.
pub(crate) const CONTROLS_CONTEXT: &str = "NewThreadControls";

actions!(kagan, [LeaveStart, StartThread]);

#[cfg(test)]
impl Kagan {
    pub(crate) fn focus_start_form(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = &self.start.form_focus {
            window.focus(form, cx);
        }
    }
}

impl Kagan {
    // Enter starts the thread only when Start is enabled, the same test that draws it enabled.
    pub(super) fn start_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Shown::Start(project) = self.screen.shown() else {
            return;
        };
        let kind = project_kind(project, cx);
        if self.start_ready(kind, cx) {
            self.start(window, cx);
        }
    }

    pub(super) fn start_ready(&self, kind: Option<ProjectKind>, cx: &App) -> bool {
        kind.is_some_and(|kind| self.start_environment(kind).is_some())
            && self.setup_ready()
            && !self.start.starting
            && self.picker.read(cx).command(cx).is_ok()
    }

    // Escape first closes an open branch menu, and leaves the screen only when none was open.
    pub(super) fn escape_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_branch_menu(window, cx) {
            cx.notify();
            return;
        }
        self.leave_start(window, cx);
    }

    // Cancel leaves the screen, and a branch menu open over it closes with it.
    pub(super) fn leave_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_branch_menu(window, cx);
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
            .as_ref()
            .is_some_and(|failure| !matches!(failure, OpError::SetupChanged(_)))
    }

    pub(super) fn start_heading(&self, project_name: &str) -> String {
        match &self.start.reuse {
            Some(reuse) => format!("New thread beside “{}”", reuse.title),
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
        let Some(reuse) = Reuse::of(thread, cx) else {
            return;
        };
        // The folder is the only choice here, so the branches are not read.
        self.open_start(&reuse.project, window, cx);
        self.start.reuse = Some(reuse);
        cx.notify();
    }
}
