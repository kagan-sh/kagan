use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, IntoElement, ParentElement as _, Role, SharedString,
    Styled as _, Window, div,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};

use super::{BranchPick, Branches, Reuse, StartFolder, base_label};
use crate::kagan::controls::{Selected, control, dispatch, note};
use crate::kagan::{Kagan, RetryBranches, UseProjectFolder, UseWorktreeFolder};
use crate::ui_registry::Facts;
use kg_domain::{BranchList, ProjectKind};
use std::path::{Path, PathBuf};

// The home folder reads as `~`, the way a person says it. Other folders keep their full path.
pub(super) fn shown_path(path: &Path) -> String {
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
    pub(super) fn render_folder(
        &self,
        kind: ProjectKind,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        if let Some(reuse) = &self.start.reuse {
            return Self::reuse_hint(reuse, cx);
        }
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
            self.worktree_hint(branches, cx)
        } else {
            note(
                "project-hint",
                "Changes land in the project folder.",
                muted,
                cx,
            )
        };
        v_flex()
            .gap_2()
            .items_start()
            .child(div().type_caption(cx).text_color(muted).child("Runs in"))
            .child(self.folder_toggles(chosen, window, cx))
            .child(self.branch_control(branches, cx))
            .child(hint)
            .into_any_element()
    }

    // One line. The folder is cut from its start, so its end stays visible, and a tooltip has the rest.
    fn worktree_hint(&self, branches: &BranchList, cx: &App) -> AnyElement {
        let base = match &self.start.pick {
            BranchPick::Current => base_label(branches),
            BranchPick::Named(name) | BranchPick::NewFrom(name) => name.clone(),
        };
        let root = self.start.worktree_root.clone();
        let full = format!("New folder on a branch from {base}, in {root}.");
        let tip = root.clone();
        h_flex()
            .id("worktree-hint")
            .ui_container()
            .w_full()
            .min_w_0()
            .whitespace_nowrap()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .aria_label(full)
            .child(
                div()
                    .flex_shrink_0()
                    .child(format!("New folder on a branch from {base}, in ")),
            )
            .child(
                div()
                    .id("worktree-root")
                    .min_w_0()
                    .text_ellipsis_start()
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(format!("{root}.")),
            )
            .into_any_element()
    }

    // The folder of the ended thread is the only choice, so one line says where the run writes.
    fn reuse_hint(reuse: &Reuse, cx: &App) -> AnyElement {
        let text = format!("Runs in {}. Changes land there.", reuse.name);
        let tip = reuse.shown.clone();
        div()
            .id("reuse-hint")
            .ui_container()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .aria_label(text.clone())
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .child(text)
            .into_any_element()
    }

    // The two segments are one row. The row takes the ring when either segment has keyboard focus.
    pub(super) fn folder_toggles(
        &self,
        chosen: Option<StartFolder>,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let starting = self.start.starting;
        let focus = self.start.segments_focus.clone();
        // Keyboard only, as GPUI's focus_visible: a mouse click moves focus but draws no ring.
        let focused = window.last_input_was_keyboard()
            && focus
                .as_ref()
                .is_some_and(|focus| focus.contains_focused(window, cx));
        let toggle = |id: &'static str,
                      button: &'static str,
                      name: SharedString,
                      folder: StartFolder,
                      action: Box<dyn Action>| {
            let on = chosen == Some(folder);
            let selected = if on { Selected::Yes } else { Selected::No };
            crate::kagan::controls::segment(button, selected, cx)
                .type_body(cx)
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
            .when_some(focus, |row, focus| row.track_focus(&focus).tab_stop(false))
            .when(focused, |row| row.border_color(cx.theme().ring))
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
