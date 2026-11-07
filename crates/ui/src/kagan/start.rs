use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Entity, FocusHandle, Subscription, Task,
    WeakEntity,
};
use kg_core::Workspace;
use kg_domain::{BranchList, Op, ProjectId, ProjectKind, ThreadId, ThreadOp};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::Kagan;

pub(super) const PLUGIN_PROMPT: &str = "Build a kagan plugin that ";
const NO_AGENT: &str = "No agent is installed, so kagan cannot start a plugin thread. Install Claude Code or Codex, then try again.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartFolder {
    Project,
    Worktree,
}

/// Which branch a new thread uses. `Current` is the checked-out branch, or HEAD when it is detached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum BranchPick {
    #[default]
    Current,
    /// A worktree's base, or the branch to check out in the project folder.
    Named(String),
    /// A new branch in the project folder, created from this one.
    NewFrom(String),
}

/// Which list the project-folder menu is showing.
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

struct SetupSeen {
    commit: String,
    mode: String,
    blob: String,
    text: String,
    truncated: bool,
}

impl SetupSeen {
    fn announcement(&self) -> Option<&'static str> {
        if self.blob.is_empty() {
            return None;
        }
        match self.mode.as_str() {
            "120000" => Some(".kagan/setup is a link, so kagan will not run it."),
            "100644" | "100755" => {
                Some("Runs .kagan/setup in the new worktree before the program starts.")
            }
            _ => None,
        }
    }

    fn script(&self) -> Option<(&str, bool)> {
        (!self.text.is_empty()).then_some((self.text.as_str(), self.truncated))
    }
}

#[derive(Default)]
pub(super) struct StartState {
    /// The folder the person picked; None means the default for the project
    folder: Option<StartFolder>,
    pick: BranchPick,
    intent: ProjectIntent,
    panel: Panel,
    pub(super) branch_focus: Option<FocusHandle>,
    branch_query: Option<Entity<InputState>>,
    branch_watch: Option<Subscription>,
    branches: Branches,
    /// Bumps when the pinned commit should be read again, so an older read cannot replace it.
    setup_epoch: u64,
    setup_task: Option<Task<()>>,
    setup: Option<SetupSeen>,
    setup_open: bool,
    setup_failure: Option<String>,
    /// A thread is being prepared, so a second start must wait for it
    starting: bool,
    /// The branch read for the shown project. Replacing it cancels a read for an earlier one.
    load: Option<Task<()>>,
    /// The start's result handler. Core keeps preparing the thread even if this is dropped.
    run: Option<Task<()>>,
    /// A plugin thread is being created. Set until it exists, so a second click waits.
    pub(super) plugin_opening: bool,
    pub(super) plugin_start: Option<Task<()>>,
    pub(super) plugin_thread: Option<kg_domain::ThreadId>,
}

impl StartState {
    /// Forgets the last project's pick. A start still preparing is kept, so a second one waits.
    pub(super) fn reset(&mut self) {
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

fn git(project: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn read_setup(project: &Path, rev: &str) -> Option<SetupSeen> {
    let commit = resolve_commit(project, rev)?;
    let listing = git(project, &["ls-tree", commit.as_str(), "--", ".kagan/setup"])?;
    let mut parts = listing.lines().next().unwrap_or("").split_whitespace();
    let mode = parts.next().unwrap_or("").to_owned();
    let kind = parts.next().unwrap_or("");
    let blob = parts.next().unwrap_or("").to_owned();
    let (mode, blob) = if kind == "blob" {
        (mode, blob)
    } else {
        (String::new(), String::new())
    };
    let (text, truncated) = if matches!(mode.as_str(), "100644" | "100755") {
        cap_script(&script_text(project, &blob))
    } else {
        (String::new(), false)
    };
    Some(SetupSeen {
        commit,
        mode,
        blob,
        text,
        truncated,
    })
}

fn script_changed(failure: &str) -> bool {
    failure.contains("Start will use the version shown on screen now")
}

const SETUP_LINES: usize = 40;
const SETUP_CHARS: usize = 2_000;
const SETUP_LINE: usize = 200;

fn cap_script(raw: &str) -> (String, bool) {
    let mut out = String::new();
    let mut truncated = false;
    for (index, line) in raw.lines().enumerate() {
        if index == SETUP_LINES {
            truncated = true;
            break;
        }
        let short: String = line.chars().take(SETUP_LINE).collect();
        if short.chars().count() < line.chars().count() {
            truncated = true;
        }
        let gap = usize::from(!out.is_empty());
        if out.len() + gap + short.len() > SETUP_CHARS {
            truncated = true;
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&short);
    }
    if !truncated && raw.lines().nth(out.lines().count()).is_some() {
        truncated = true;
    }
    (out, truncated)
}

fn script_text(project: &Path, blob: &str) -> String {
    git(project, &["cat-file", "blob", blob]).unwrap_or_default()
}

fn resolve_commit(project: &Path, rev: &str) -> Option<String> {
    let object = format!("{rev}^{{commit}}");
    let commit = git(
        project,
        &["rev-parse", "--verify", "--quiet", object.as_str()],
    )?
    .trim()
    .to_owned();
    (!commit.is_empty()).then_some(commit)
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

/// Waits for the new plugin thread to show its prompt, then pastes the prompt once the screen
/// settles. After 15 seconds, the window offers the prompt to copy instead.
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
