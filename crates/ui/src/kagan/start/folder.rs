use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Disableable as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, IntoElement, ParentElement as _, Role, SharedString,
    Styled as _, div,
};

use super::{Branches, StartFolder};
use crate::kagan::controls::{Selected, control, dispatch, note};
use crate::kagan::{Kagan, RetryBranches, UseProjectFolder, UseWorktreeFolder};
use crate::ui_registry::Facts;
use kg_domain::ProjectKind;
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
        let worktree = chosen == Some(StartFolder::Worktree);
        v_flex()
            .gap_1()
            .items_start()
            .child(div().type_small(cx).text_color(muted).child("Runs in"))
            .child(self.folder_toggles(chosen, cx))
            .child(self.branch_control(branches, cx))
            .when(worktree, |rows| {
                rows.child(note(
                    "worktree-hint",
                    "A worktree is a separate folder for this thread’s changes.",
                    muted,
                    cx,
                ))
            })
            .into_any_element()
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
