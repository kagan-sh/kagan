use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div,
};
use kg_domain::OpError;

use super::{StartFolder, setup_announcement, setup_script};
use crate::kagan::Kagan;
use crate::kagan::controls::{control, note};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
impl Kagan {
    pub(super) fn setup_disclosure(
        &self,
        script: (&str, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                                .type_small(cx)
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
                            .type_small(cx)
                            .text_color(cx.theme().muted_foreground)
                            .child("The rest of the script is not shown."),
                    )
                })
            })
            .into_any_element()
    }

    pub(super) fn setup_retry(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn setup_block(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.chosen_folder() != Some(StartFolder::Worktree) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let failed = cx.theme().danger;
        let mut rows = Vec::new();
        if let Some(message) = self.start.setup.as_ref().and_then(setup_announcement) {
            rows.push(note("setup-script", message, muted, cx));
        }
        if let Some(seen) = self.start.setup.as_ref().and_then(setup_script) {
            rows.push(self.setup_disclosure(seen, cx));
        }
        if let Some(failure) = &self.start.setup_failure {
            rows.push(note("setup-failure", failure.to_string(), failed, cx));
            if !matches!(failure, OpError::SetupChanged(_)) {
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
}
