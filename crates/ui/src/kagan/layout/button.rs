use crate::theme::{SpaceStep, TypeScale as _, space};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{IconName, h_flex};
use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString, Styled, div,
    px,
};

use crate::kagan::{Kagan, ToggleRightSidebar};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::bar_button;

impl Kagan {
    fn changed_files(&self, cx: &App) -> usize {
        if !self.thread_visible(cx) || self.changes.thread != self.shown_thread(cx) {
            return 0;
        }
        self.changes.files.len()
    }

    pub(super) fn right_button(&self, drawn: bool, waiting: usize, cx: &App) -> impl IntoElement {
        let changed = if drawn { 0 } else { self.changed_files(cx) };
        let icon = if drawn {
            IconName::PanelRight
        } else {
            IconName::PanelRightOpen
        };
        let value: SharedString = if drawn {
            "shown".into()
        } else if waiting > 0 {
            format!("hidden, {waiting} waiting").into()
        } else if changed > 0 {
            format!("hidden, {changed} changed").into()
        } else {
            "hidden".into()
        };
        let name: SharedString = if drawn {
            "Hide panels".into()
        } else {
            "Show panels".into()
        };
        let nothing = !drawn && !self.has_right_content(cx);
        let button = bar_button(
            "right-sidebar-toggle",
            name.clone(),
            icon,
            self.in_settings() || nothing,
            ToggleRightSidebar,
            cx,
        )
        .tab_stop(false);
        let button = if nothing {
            button.tooltip("Open a thread to see its changes, or wait for a question.")
        } else if changed > 0 {
            button.tooltip("This thread has changed files. Show panels to see them.")
        } else {
            button
        };
        let facts = Facts {
            hint: None,
            role: Role::Button,
            name,
            value: Some(value),
            disabled: self.in_settings() || nothing,
            selected: None,
            focus: Some(self.right.toggle_focus.clone()),
        };
        let row = h_flex()
            .id("right-sidebar-toggle")
            .gap(px(space(SpaceStep::Xs, cx)))
            .items_center()
            .ui_observe_as(facts);
        let row = if drawn || waiting == 0 {
            row
        } else {
            row.child(
                div()
                    .id("right-sidebar-waiting")
                    .ui_container()
                    .type_small(cx)
                    .text_color(crate::theme::Tone::Needs.color(cx))
                    .child(waiting.to_string()),
            )
        };
        let row = if changed == 0 {
            row
        } else {
            row.child(
                div()
                    .id("right-sidebar-changes")
                    .ui_container()
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(changed.to_string()),
            )
        };
        row.child(button)
    }
}
