use gpui_kit::{AppContext as _, Context, Window};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentOp, EnvironmentSpec, Op, OpResult, ProjectId, ProjectKind, Run, ThreadOp,
};

use super::{Branches, StartFolder, base_of, project_kind, project_path, read_setup};
use crate::kagan::{
    Kagan, RetryBranches, RetryPrograms, Shown, ToggleCustomCommand, UseProjectFolder,
    UseWorktreeFolder,
};
impl Kagan {
    pub(in crate::kagan) fn load_branches(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let path = project_path(&project, cx);
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

    pub(in crate::kagan) fn retry_branches_action(
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

    pub(super) fn setup_ready(&self) -> bool {
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

    pub(super) fn start_environment(&self, kind: ProjectKind) -> Option<EnvironmentSpec> {
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
            self.start.folder = Some(folder);
            cx.notify();
        }
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
