use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, AnyWindowHandle, App, AppContext as _, AsyncApp, Context,
    InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, WeakEntity, Window, div, rems,
};
use kg_core::Workspace;
use kg_domain::{
    BranchList, EnvironmentOp, EnvironmentSpec, Op, OpResult, ProgramOp, ProjectId, ProjectKind,
    ProjectOp, Run, ThreadId, ThreadOp,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::shown::{control, dispatch, note};
use super::{
    Kagan, RetryBranches, RetryPrograms, Shown, ToggleCustomCommand, UseProjectFolder,
    UseWorktreeFolder,
};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) const PLUGIN_PROMPT: &str = "Build a kagan plugin that ";
const NO_AGENT: &str = "No agent is installed, so kagan cannot start a plugin thread. Install Claude Code or Codex, then try again.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartFolder {
    Project,
    Worktree,
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
    branches: Branches,
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
    pub(super) fn show(&mut self) {
        self.folder = None;
        self.branches = Branches::Loading;
        self.setup = None;
        self.setup_open = false;
        self.setup_failure = None;
    }
}

impl Kagan {
    /// Reads the git branches of the project, so the screen offers a worktree only where one can
    /// exist and names the branch it will start from.
    pub(super) fn load_branches(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let path = project_path(&project, cx);
        let task = Workspace::dispatch(
            Op::Environment(EnvironmentOp::Branches {
                id: project.clone(),
            }),
            cx,
        );
        self.start.load = Some(cx.spawn(async move |this, cx| {
            let branches = match task.await {
                Ok(OpResult::Branches(branches)) => Branches::Loaded(branches),
                Ok(_) => Branches::Failed("the reply had no branches".into()),
                Err(error) => Branches::Failed(error.to_string()),
            };
            let rev = match &branches {
                Branches::Loaded(list) => base_of(list),
                Branches::Loading | Branches::Failed(_) => None,
            };
            let setup = match (path, rev) {
                (Some(path), Some(rev)) => {
                    cx.background_spawn(async move { read_setup(&path, &rev) })
                        .await
                }
                _ => None,
            };
            _ = this.update(cx, |this, cx| {
                if matches!(this.screen.shown(), Shown::Start(shown) if *shown == project) {
                    this.start.branches = branches;
                    this.start.setup = setup;
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn retry_branches_action(
        &mut self,
        _: &RetryBranches,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Shown::Start(project) = self.screen.shown() else {
            return;
        };
        let project = project.clone();
        self.start.branches = Branches::Loading;
        self.start.setup = None;
        self.load_branches(project, cx);
        cx.notify();
    }

    fn setup_ready(&self) -> bool {
        match self.chosen_folder() {
            Some(StartFolder::Worktree) => self.start.setup.is_some(),
            Some(StartFolder::Project) | None => true,
        }
    }

    fn chosen_folder(&self) -> Option<StartFolder> {
        match &self.start.branches {
            Branches::Loading | Branches::Failed(_) => None,
            Branches::Loaded(branches) => {
                if branches.head.is_none() {
                    return Some(StartFolder::Project);
                }
                Some(self.start.folder.unwrap_or(StartFolder::Worktree))
            }
        }
    }

    fn start_environment(&self, kind: ProjectKind) -> Option<EnvironmentSpec> {
        match kind {
            ProjectKind::Personal => Some(EnvironmentSpec::Personal),
            ProjectKind::Standard => match self.chosen_folder()? {
                StartFolder::Project => Some(EnvironmentSpec::ProjectDefault),
                StartFolder::Worktree => {
                    if !matches!(&self.start.branches, Branches::Loaded(_)) {
                        return None;
                    }
                    // The commit resolved while branches loaded, not the branch name.
                    let commit = self.start.setup.as_ref()?.commit.clone();
                    Some(EnvironmentSpec::Worktree {
                        base_branch: Some(commit),
                        branch: None,
                    })
                }
            },
        }
    }

    pub(super) fn choose_folder(&mut self, folder: StartFolder, cx: &mut Context<Self>) {
        if !matches!(self.screen.shown(), Shown::Start(_)) || self.start.starting {
            return;
        }
        let offered = match (folder, &self.start.branches) {
            (StartFolder::Project, Branches::Loaded(_)) => true,
            (StartFolder::Worktree, Branches::Loaded(branches)) => branches.head.is_some(),
            (_, Branches::Loading | Branches::Failed(_)) => false,
        };
        if offered {
            self.start.folder = Some(folder);
            cx.notify();
        }
    }

    pub(super) fn use_project_folder(
        &mut self,
        _: &UseProjectFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_folder(StartFolder::Project, cx);
    }

    pub(super) fn use_worktree_folder(
        &mut self,
        _: &UseWorktreeFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_folder(StartFolder::Worktree, cx);
    }

    pub(super) fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Shown::Start(project) = self.screen.shown() else {
            return;
        };
        let project_id = project.clone();
        if self.start.starting {
            return;
        }
        let program = match self.picker.read(cx).command(cx) {
            Ok(program) => program,
            Err(message) => {
                self.note = Some(message.into());
                cx.notify();
                return;
            }
        };
        let Some(kind) = project_kind(&project_id, cx) else {
            return;
        };
        let Some(environment) = self.start_environment(kind) else {
            return;
        };
        let run = ThreadOp::Run(Run {
            project_id,
            program,
            title: None,
            environment,
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        });
        self.start.setup_failure = None;
        self.start.starting = true;
        let task = Workspace::dispatch(Op::Thread(run), cx);
        let window = window.window_handle();
        self.start.run = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.start.starting = false;
                    if let Err(error) = &result
                        && error.to_string().contains(".kagan/setup")
                    {
                        this.start.setup_failure = Some(error.to_string());
                        cx.notify();
                    } else {
                        this.finish(result, "start the thread", window, cx);
                    }
                });
            });
        }));
        cx.notify();
    }

    pub(super) fn toggle_custom_command(
        &mut self,
        _: &ToggleCustomCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker
            .update(cx, |picker, cx| picker.toggle_custom(window, cx));
    }

    pub(super) fn retry_programs(
        &mut self,
        _: &RetryPrograms,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| picker.load(window, cx));
    }

    pub(super) fn render_start(&self, project: &ProjectId, cx: &mut Context<Self>) -> AnyElement {
        let name = self
            .sidebar
            .read(cx)
            .project(project)
            .map_or_else(String::new, |project| project.name.clone());
        let kind = project_kind(project, cx);
        let ready = kind.is_some_and(|kind| self.start_environment(kind).is_some())
            && self.setup_ready()
            && !self.start.starting
            && self.picker.read(cx).command(cx).is_ok();
        let start = control(
            "start",
            Facts {
                disabled: !ready,
                ..Facts::button("Start")
            },
            Button::new("start-button-control")
                .primary()
                .label("Start")
                .loading(self.start.starting)
                .disabled(!ready)
                .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
        );
        v_flex()
            .id("start-form")
            .size_full()
            .child(
                v_flex()
                    .id("start-fields")
                    .ui_container()
                    .w_full()
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .w_full()
                            .min_h_full()
                            .items_center()
                            .justify_center()
                            .child(
                                v_flex()
                                    .w_full()
                                    .max_w(rems(32.))
                                    .gap_3()
                                    .p_4()
                                    .child(
                                        div()
                                            .id("start-prompt")
                                            .ui_container()
                                            .aria_label(format!("New thread in {name}"))
                                            .type_title()
                                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                            .child(format!("New thread in {name}")),
                                    )
                                    .child(self.picker.clone())
                                    .children(kind.map(|kind| self.render_folder(kind, cx)))
                                    .children(self.setup_block(cx))
                                    .child(h_flex().justify_end().child(start)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Adds the folder the person picked. On success its start screen opens, so the first thread
    /// is one step away.
    pub(crate) fn add_picked_project(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let added = Workspace::dispatch(Op::Project(ProjectOp::Add { path }), cx);
        let window = window.window_handle();
        cx.spawn(async move |this, cx| match added.await {
            Ok(OpResult::Project(project)) => {
                _ = cx.update_window(window, |_, window, cx| {
                    _ = this.update(cx, |this, cx| this.show_start(project.id, window, cx));
                });
            }
            Ok(_) => {}
            Err(error) => {
                _ = this.update(cx, |this, cx| {
                    this.note = Some(format!("Could not add the project: {error}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn setup_disclosure(&self, script: (&str, bool), cx: &mut Context<Self>) -> AnyElement {
        let (text, truncated) = script;
        let open = self.start.setup_open;
        let name = if open { "Hide script" } else { "Show script" };
        let ink = cx.theme().foreground;
        v_flex()
            .gap_1()
            .items_start()
            .child(control(
                "setup-disclose",
                Facts {
                    selected: Some(open),
                    ..Facts::button(name)
                },
                Button::new("setup-disclose-button")
                    .small()
                    .ghost()
                    .label(name)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.start.setup_open = !this.start.setup_open;
                        cx.notify();
                    })),
            ))
            .when(open, |rows| {
                rows.child(
                    div()
                        .id("setup-body")
                        .ui_container()
                        .aria_label(text.to_owned())
                        .child(v_flex().gap_0().children(text.lines().map(|line| {
                            let shown = if line.is_empty() { " " } else { line };
                            div()
                                .type_small()
                                .font(gpui_kit::font(cx.theme().mono_font_family.clone()))
                                .text_color(ink)
                                .child(shown.to_owned())
                        }))),
                )
                .when(truncated, |rows| {
                    rows.child(
                        div()
                            .id("setup-truncated")
                            .ui_container()
                            .aria_label("The rest of the script is not shown.")
                            .type_small()
                            .text_color(cx.theme().muted_foreground)
                            .child("The rest of the script is not shown."),
                    )
                })
            })
            .into_any_element()
    }

    fn setup_retry(&self, cx: &mut Context<Self>) -> AnyElement {
        let name = "Retry setup in this worktree";
        control(
            "setup-retry",
            Facts {
                disabled: self.start.starting,
                ..Facts::button(name)
            },
            Button::new("setup-retry-button")
                .small()
                .outline()
                .label(name)
                .disabled(self.start.starting)
                .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
        )
        .into_any_element()
    }

    fn setup_block(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.chosen_folder() != Some(StartFolder::Worktree) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let failed = cx.theme().danger;
        let mut rows = Vec::new();
        if let Some(message) = self.start.setup.as_ref().and_then(SetupSeen::announcement) {
            rows.push(note("setup-script", message, muted));
        }
        if let Some(seen) = self.start.setup.as_ref().and_then(SetupSeen::script) {
            rows.push(self.setup_disclosure(seen, cx));
        }
        if let Some(failure) = &self.start.setup_failure {
            rows.push(note("setup-failure", failure.clone(), failed));
            if !script_changed(failure) {
                rows.push(self.setup_retry(cx));
            }
        }
        (!rows.is_empty()).then(|| {
            v_flex()
                .gap_2()
                .items_start()
                .children(rows)
                .into_any_element()
        })
    }

    fn render_folder(&self, kind: ProjectKind, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        if kind == ProjectKind::Personal {
            return note("folder", "Runs in your Personal folder.", muted);
        }
        let branches = match &self.start.branches {
            Branches::Loading => return note("folder", "Checking the folder…", muted),
            Branches::Failed(message) => {
                return v_flex()
                    .gap_2()
                    .items_start()
                    .child(note(
                        "folder",
                        format!("Couldn’t read the git branches: {message}"),
                        muted,
                    ))
                    .child(control(
                        "folder-retry",
                        Facts {
                            disabled: self.start.starting,
                            ..Facts::button("Retry")
                        },
                        Button::new("folder-retry-button")
                            .small()
                            .outline()
                            .label("Retry")
                            .disabled(self.start.starting)
                            .on_click(dispatch(RetryBranches)),
                    ))
                    .into_any_element();
            }
            Branches::Loaded(branches) => branches,
        };
        if branches.head.is_none() {
            return note(
                "folder",
                "No git commit to start a worktree from, so the thread runs in the folder itself.",
                muted,
            );
        }
        let chosen = self.chosen_folder();
        v_flex()
            .gap_1()
            .items_start()
            .child(div().type_small().text_color(muted).child("Runs in"))
            .child(self.folder_toggles(branches, chosen, cx))
            .child(note(
                "worktree-hint",
                "A worktree is a separate folder for this thread’s changes.",
                muted,
            ))
            .into_any_element()
    }

    fn folder_toggles(
        &self,
        branches: &BranchList,
        chosen: Option<StartFolder>,
        cx: &App,
    ) -> AnyElement {
        let worktree_name: SharedString =
            format!("New worktree from {}", base_label(branches)).into();
        let starting = self.start.starting;
        let toggle = |id: &'static str,
                      button: &'static str,
                      name: SharedString,
                      folder: StartFolder,
                      action: Box<dyn Action>| {
            let on = chosen == Some(folder);
            super::shown::segment(button, on, cx)
                .disabled(starting)
                .tooltip(name.clone())
                .child(control(
                    id,
                    Facts {
                        role: Role::RadioButton,
                        disabled: starting,
                        selected: Some(on),
                        ..Facts::button(name.clone())
                    },
                    name,
                ))
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        };
        super::shown::segments(cx)
            .child(toggle(
                "folder-project",
                "folder-project-button",
                "Project folder".into(),
                StartFolder::Project,
                Box::new(UseProjectFolder),
            ))
            .child(toggle(
                "folder-worktree",
                "folder-worktree-button",
                worktree_name,
                StartFolder::Worktree,
                Box::new(UseWorktreeFolder),
            ))
            .into_any_element()
    }

    pub(super) fn cancel_plugin_start(&mut self) {
        self.start.plugin_start = None;
        self.start.plugin_thread = None;
    }

    pub(super) fn add_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start.plugin_opening || self.start.plugin_start.is_some() {
            return;
        }
        self.start.plugin_opening = true;
        cx.notify();
        let list = Workspace::dispatch(Op::Program(ProgramOp::Discover), cx);
        let window = window.window_handle();
        // Detached, so a thread switch cannot drop a start that is still creating its thread.
        cx.spawn(async move |this, cx| {
            let found = match list.await {
                Ok(OpResult::Programs(list)) => Ok(list.agent().map(|agent| agent.command.clone())),
                Ok(_) => Err("kagan could not list the programs".to_owned()),
                Err(error) => Err(error.to_string()),
            };
            let program = match found {
                Ok(Some(program)) => program,
                other => {
                    let message = match other {
                        Err(message) => message,
                        Ok(_) => NO_AGENT.to_owned(),
                    };
                    _ = this.update(cx, |this, cx| {
                        this.start.plugin_opening = false;
                        this.note = Some(message.into());
                        cx.notify();
                    });
                    return;
                }
            };
            let task = cx.update(|cx| {
                Workspace::dispatch(
                    Op::Thread(ThreadOp::Run(Run {
                        project_id: ProjectId("personal".into()),
                        program,
                        title: Some("New plugin".into()),
                        environment: EnvironmentSpec::Personal,
                        visibility: None,
                        parent_thread_id: None,
                        lifecycle_owner_thread_id: None,
                        metadata: None,
                    })),
                    cx,
                )
            });
            let id = match task.await {
                Ok(OpResult::Thread(thread)) => thread.id,
                other => {
                    _ = cx.update_window(window, |_, window, cx| {
                        _ = this.update(cx, |this, cx| {
                            this.start.plugin_opening = false;
                            this.finish(other, "start the plugin thread", window, cx);
                        });
                    });
                    return;
                }
            };
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.start.plugin_opening = false;
                    this.start.plugin_thread = Some(id.clone());
                    this.select(id.clone(), window, cx);
                    let handle = window.window_handle();
                    this.start.plugin_start = Some(cx.spawn(async move |this, cx| {
                        follow_plugin_prompt(this, handle, id, cx).await;
                    }));
                });
            });
        })
        .detach();
    }

    pub(super) fn plugin_prompt_visible(&self, cx: &App) -> bool {
        self.plugin_prompt_notice.is_some()
            && self.plugin_prompt_notice.as_ref() == self.shown_thread(cx).as_ref()
    }

    pub(super) fn plugin_prompt_notice(cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("plugin-prompt-notice")
            .ui_container()
            .px_4()
            .py_2()
            .child(format!("Copy this prompt: {PLUGIN_PROMPT}"))
            .child(
                Button::new("copy-plugin-prompt")
                    .label("Copy")
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                            PLUGIN_PROMPT.into(),
                        ));
                        this.plugin_prompt_notice = None;
                        cx.notify();
                    })),
            )
    }

    pub(super) fn new_personal_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_start(ProjectId("personal".into()), window, cx);
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
