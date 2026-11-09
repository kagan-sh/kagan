use gpui_kit::{App, AppContext as _, Context, Window};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentOp, EnvironmentSpec, EnvironmentStatus, Op, OpResult, ProjectBranch, ProjectId,
    ProjectKind, Run, ThreadOp,
};

use super::{
    BranchPick, Branches, ProjectIntent, StartFolder, base_of, pinned_setup, project_kind,
    project_path,
};
use crate::kagan::{
    Kagan, RetryBranches, RetryPrograms, Shown, ToggleCustomCommand, UseProjectFolder,
    UseWorktreeFolder,
};
impl Kagan {
    pub(in crate::kagan) fn load_branches(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let epoch = self.start.setup_epoch;
        let task = Workspace::dispatch(
            Op::Environment(EnvironmentOp::Branches {
                project_id: project.clone(),
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
            let setup = match rev {
                Some(rev) => pinned_setup(project.clone(), rev, cx).await,
                None => None,
            };
            _ = this.update(cx, |this, cx| {
                if matches!(this.screen.shown(), Shown::Start(shown) if *shown == project) {
                    this.start.branches = branches;
                    if this.start.setup_epoch == epoch
                        && matches!(this.start.pick, BranchPick::Current)
                    {
                        this.start.setup = setup;
                    } else {
                        this.reload_setup(cx);
                    }
                    cx.notify();
                }
            });
        }));
    }

    pub(in crate::kagan) fn retry_branches_action(
        &mut self,
        _: &RetryBranches,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Shown::Start(project) = self.screen.shown() else {
            return;
        };
        let project = project.clone();
        self.start.setup_epoch = self.start.setup_epoch.wrapping_add(1);
        self.close_branch_menu(window, cx);
        self.start.branches = Branches::Loading;
        self.start.setup = None;
        self.load_branches(project, cx);
        cx.notify();
    }

    pub(super) fn setup_ready(&self) -> bool {
        if self.start.reuse.is_some() {
            return true;
        }
        match self.chosen_folder() {
            Some(StartFolder::Worktree) => self.start.setup.is_some(),
            Some(StartFolder::Project) | None => true,
        }
    }

    pub(super) fn chosen_folder(&self) -> Option<StartFolder> {
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

    pub(super) fn start_environment(&self, kind: ProjectKind, cx: &App) -> Option<EnvironmentSpec> {
        if let Some(reuse) = &self.start.reuse {
            return Some(EnvironmentSpec::Reuse {
                environment_id: reuse.environment.clone(),
            });
        }
        match kind {
            ProjectKind::Personal => Some(EnvironmentSpec::Personal),
            ProjectKind::Standard => match self.chosen_folder()? {
                StartFolder::Project => self.project_environment(cx),
                StartFolder::Worktree => {
                    if !matches!(&self.start.branches, Branches::Loaded(_)) {
                        return None;
                    }
                    // The commit resolved for the branch on screen, not the branch name.
                    let commit = self.start.setup.as_ref()?.commit.clone();
                    Some(EnvironmentSpec::Worktree {
                        base_branch: Some(commit),
                        branch: None,
                    })
                }
            },
        }
    }

    fn project_environment(&self, cx: &App) -> Option<EnvironmentSpec> {
        let Branches::Loaded(list) = &self.start.branches else {
            return None;
        };
        if self.switches(list) && self.switch_block(cx).is_some() {
            return None;
        }
        match &self.start.pick {
            BranchPick::Current => Some(EnvironmentSpec::ProjectDefault),
            // A named branch stays a checkout even when the loaded list called it current.
            // HEAD may have moved since that read, and core treats a checkout of the live
            // branch as a no-op.
            BranchPick::Named(name) => Some(EnvironmentSpec::ProjectOn {
                branch: ProjectBranch::Checkout { name: name.clone() },
            }),
            BranchPick::NewFrom(base) => Some(EnvironmentSpec::ProjectOn {
                branch: ProjectBranch::New { base: base.clone() },
            }),
        }
    }

    fn switches(&self, list: &kg_domain::BranchList) -> bool {
        match &self.start.pick {
            BranchPick::Current => false,
            BranchPick::Named(name) => list.current.as_deref() != Some(name.as_str()),
            BranchPick::NewFrom(_) => true,
        }
    }

    pub(super) fn switch_block(&self, cx: &App) -> Option<String> {
        if self.chosen_folder() != Some(StartFolder::Project) {
            return None;
        }
        let (moving, blocked) = {
            let Branches::Loaded(list) = &self.start.branches else {
                return None;
            };
            (self.switches(list), list.checkout_block.clone())
        };
        moving
            .then(|| branch_move_block(blocked, self.folder_busy(cx)))
            .flatten()
    }

    pub(super) fn folder_busy(&self, cx: &App) -> bool {
        let Shown::Start(project) = self.screen.shown() else {
            return false;
        };
        let Some(path) = project_path(project, cx) else {
            return false;
        };
        let folder = dunce::canonicalize(&path).unwrap_or(path);
        let workspace = Workspace::global(cx).read(cx);
        workspace.thread_records(cx).iter().any(|thread| {
            thread.project_id == *project
                && thread.archived_at.is_none()
                && thread.environment_id.as_ref().is_some_and(|id| {
                    workspace.environment(id).is_some_and(|environment| {
                        let record = environment.read(cx).record();
                        record.status != EnvironmentStatus::Destroyed
                            && dunce::canonicalize(&record.path)
                                .unwrap_or_else(|_| record.path.clone())
                                == folder
                    })
                })
        })
    }

    fn base_rev(&self) -> Option<String> {
        let Branches::Loaded(list) = &self.start.branches else {
            return None;
        };
        match &self.start.pick {
            BranchPick::Named(name) | BranchPick::NewFrom(name) => Some(name.clone()),
            BranchPick::Current => base_of(list),
        }
    }

    pub(super) fn reload_setup(&mut self, cx: &mut Context<Self>) {
        self.start.setup_epoch = self.start.setup_epoch.wrapping_add(1);
        let epoch = self.start.setup_epoch;
        if self.chosen_folder() != Some(StartFolder::Worktree) {
            self.start.setup = None;
            self.start.setup_task = None;
            return;
        }
        let Shown::Start(project) = self.screen.shown() else {
            return;
        };
        let project = project.clone();
        let rev = self.base_rev();
        self.start.setup = None;
        self.start.setup_task = Some(cx.spawn(async move |this, cx| {
            let setup = match rev {
                Some(rev) => pinned_setup(project.clone(), rev, cx).await,
                None => None,
            };
            _ = this.update(cx, |this, cx| {
                if this.start.setup_epoch == epoch
                    && matches!(this.screen.shown(), Shown::Start(shown) if *shown == project)
                {
                    this.start.setup = setup;
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn apply_pick(&mut self, pick: BranchPick, cx: &mut Context<Self>) {
        if self.start.starting {
            return;
        }
        self.start.pick = pick;
        self.start.panel = super::Panel::Closed;
        self.start.setup_failure = None;
        if self.chosen_folder() == Some(StartFolder::Worktree) {
            self.reload_setup(cx);
        }
        cx.notify();
    }

    pub(in crate::kagan) fn choose_folder(&mut self, folder: StartFolder, cx: &mut Context<Self>) {
        if !matches!(self.screen.shown(), Shown::Start(_)) || self.start.starting {
            return;
        }
        let offered = match (folder, &self.start.branches) {
            (StartFolder::Project, Branches::Loaded(_)) => true,
            (StartFolder::Worktree, Branches::Loaded(branches)) => branches.head.is_some(),
            (_, Branches::Loading | Branches::Failed(_)) => false,
        };
        if offered {
            let current = self.start.folder.unwrap_or(StartFolder::Worktree);
            self.start.folder = Some(folder);
            if current != folder {
                self.start.pick = BranchPick::Current;
                self.start.intent = ProjectIntent::Current;
                self.start.panel = super::Panel::Closed;
                self.start.setup_failure = None;
                if folder == StartFolder::Worktree {
                    self.reload_setup(cx);
                } else {
                    self.start.setup_epoch = self.start.setup_epoch.wrapping_add(1);
                    self.start.setup = None;
                    self.start.setup_task = None;
                }
            }
            cx.notify();
        }
    }

    pub(in crate::kagan) fn close_branch_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.start.panel == super::Panel::Closed {
            return false;
        }
        self.start.panel = super::Panel::Closed;
        self.picker
            .update(cx, |picker, cx| picker.focus(window, cx));
        true
    }

    pub(in crate::kagan) fn use_project_folder(
        &mut self,
        _: &UseProjectFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_folder(StartFolder::Project, cx);
    }

    pub(in crate::kagan) fn use_worktree_folder(
        &mut self,
        _: &UseWorktreeFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_folder(StartFolder::Worktree, cx);
    }

    pub(in crate::kagan) fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let Some(environment) = self.start_environment(kind, cx) else {
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

    pub(in crate::kagan) fn toggle_custom_command(
        &mut self,
        _: &ToggleCustomCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker
            .update(cx, |picker, cx| picker.toggle_custom(window, cx));
    }

    pub(in crate::kagan) fn retry_programs(
        &mut self,
        _: &RetryPrograms,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| picker.load(window, cx));
    }
}

pub(super) fn branch_move_block(git: Option<String>, busy: bool) -> Option<String> {
    git.or_else(|| busy.then(|| kg_domain::FOLDER_IN_USE.to_owned()))
}
