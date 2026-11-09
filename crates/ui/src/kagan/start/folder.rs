use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Disableable as _, Sizable as _, v_flex};
use gpui_kit::{
    Action, AnyElement, App, Context, IntoElement, ParentElement as _, Role, SharedString,
    Styled as _, div,
};

use super::{BranchPick, Branches, StartFolder, base_label};
use crate::kagan::controls::{Selected, control, dispatch, note};
use crate::kagan::{Kagan, RetryBranches, UseProjectFolder, UseWorktreeFolder};
use crate::ui_registry::Facts;
use kg_core::Workspace;
use kg_domain::{BranchList, ProjectKind};
use std::path::{Path, PathBuf};

// The home folder reads as `~`, the way a person says it. Other folders keep their full path.
fn shown_path(path: &Path) -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    match home.and_then(|home| {
        path.strip_prefix(home)
            .ok()
            .map(|rest| Path::new("~").join(rest))
    }) {
        Some(short) => short.display().to_string(),
        None => path.display().to_string(),
    }
}
impl Kagan {
    pub(super) fn render_folder(&self, kind: ProjectKind, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        if kind == ProjectKind::Personal {
            return note("folder", "Runs in your Personal folder.", muted, cx);
        }
        let branches = match &self.start.branches {
            Branches::Loading => return note("folder", "Checking the folder…", muted, cx),
            Branches::Failed(message) => {
                return v_flex()
                    .gap_2()
                    .items_start()
                    .child(note(
                        "folder",
                        format!("Couldn’t read the git branches: {message}"),
                        muted,
                        cx,
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
                cx,
            );
        }
        let chosen = self.chosen_folder();
        let hint = if chosen == Some(StartFolder::Worktree) {
            ("worktree-hint", self.worktree_hint(branches, cx))
        } else {
            (
                "project-hint",
                "Runs in your project folder. Changes land in that folder.".to_owned(),
            )
        };
        v_flex()
            .gap_2()
            .items_start()
            .child(div().type_caption(cx).text_color(muted).child("Runs in"))
            .child(self.folder_toggles(chosen, cx))
            .child(self.branch_control(branches, cx))
            .child(note(hint.0, hint.1, muted, cx))
            .into_any_element()
    }

    fn worktree_hint(&self, branches: &BranchList, cx: &App) -> String {
        let base = match &self.start.pick {
            BranchPick::Current => base_label(branches),
            BranchPick::Named(name) | BranchPick::NewFrom(name) => name.clone(),
        };
        let root = Workspace::global(cx).read(cx).data_dir().join("worktrees");
        format!(
            "A new folder on its own branch from {base}, under {}. Your project folder stays as it is.",
            shown_path(&root)
        )
    }

    pub(super) fn folder_toggles(&self, chosen: Option<StartFolder>, cx: &App) -> AnyElement {
        let starting = self.start.starting;
        let toggle = |id: &'static str,
                      button: &'static str,
                      name: SharedString,
                      folder: StartFolder,
                      action: Box<dyn Action>| {
            let on = chosen == Some(folder);
            let selected = if on { Selected::Yes } else { Selected::No };
            crate::kagan::controls::segment(button, selected, cx)
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
        crate::kagan::controls::segments(cx)
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
                "New worktree".into(),
                StartFolder::Worktree,
                Box::new(UseWorktreeFolder),
            ))
            .into_any_element()
    }
}
