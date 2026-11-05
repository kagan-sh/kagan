use crate::theme::TypeScale as _;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{IconName, TitleBar, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};

use crate::kagan::screen::{Activation, Side};
use crate::kagan::{Kagan, ToggleSidebar, right};
use crate::ui_observe::UiObserveExt as _;

use super::{RIGHT_OVERLAY, toggle};
use crate::kagan::TITLE;

impl Kagan {
    pub(in crate::kagan) fn right_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.screen.drawn(Side::Right) || self.panes.right || self.in_settings() {
            return None;
        }
        let dismiss =
            cx.listener(|this, _, window, cx| this.dismiss_overlay(Side::Right, window, cx));
        let layer = div()
            .id("right-overlay")
            .ui_container()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("right-overlay-scrim")
                    .ui_container()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_click(dismiss),
            )
            .child(
                div()
                    .id("right-overlay-panel")
                    .ui_container()
                    .key_context(RIGHT_OVERLAY)
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(right::WIDTH))
                    .bg(cx.theme().sidebar)
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .occlude()
                    .child(right::render(self, window, cx)),
            )
            .into_any_element();
        if matches!(self.screen.painted(Side::Right), Some(Activation::Focus)) {
            let key = self.overlay_focus(cx);
            let handle = crate::kagan::tab_stop(window, cx, ElementId::Name(key.into()));
            window.focus(&handle, cx);
        }
        Some(layer)
    }

    pub(in crate::kagan) fn title_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let drawn = self.right_drawn();
        let left_shown = self.panes.left || self.screen.drawn(Side::Left);
        let thread = self
            .thread_visible(cx)
            .then(|| self.shown_thread(cx))
            .flatten()
            .and_then(|id| self.record(&id, cx).cloned());
        let project = thread
            .as_ref()
            .and_then(|thread| self.sidebar.read(cx).project(&thread.project_id))
            .map(|project| project.name.clone());
        let title = thread.as_ref().map_or_else(
            || TITLE.to_owned(),
            |thread| {
                project.as_ref().map_or_else(
                    || thread.title.clone(),
                    |project| format!("{} — {project}", thread.title),
                )
            },
        );
        if self.window_title != title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        let label = thread.as_ref().map(|thread| {
            h_flex()
                .id("thread-heading")
                .ui_container()
                .flex_1()
                .min_w_0()
                .justify_center()
                .gap_1p5()
                .child(self.title_line(thread, cx))
                .when_some(project, |heading, project| {
                    heading
                        .child(
                            div()
                                .flex_none()
                                .type_body()
                                .text_color(cx.theme().muted_foreground)
                                .child("·"),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_initial()
                                .text_ellipsis()
                                .type_body()
                                .text_color(cx.theme().muted_foreground)
                                .child(project),
                        )
                })
        });
        let controls = thread
            .as_ref()
            .map(|thread| self.thread_controls(thread, cx));
        // Controls occlude the drag area, or Windows takes a press on them as a caption drag.
        TitleBar::new().pl_0().child(
            h_flex()
                .size_full()
                .items_center()
                .pr_2()
                .when(cfg!(target_os = "macos"), Styled::pl_20)
                .when(!cfg!(target_os = "macos"), Styled::pl_3)
                .child(toggle(
                    "left-sidebar-toggle",
                    if left_shown {
                        "Hide sidebar"
                    } else {
                        "Show sidebar"
                    },
                    left_shown,
                    self.in_settings(),
                    (IconName::PanelLeft, IconName::PanelLeftOpen),
                    ToggleSidebar,
                    cx,
                ))
                .child(h_flex().flex_1().min_w_0().children(label))
                .child(
                    h_flex()
                        .gap_2()
                        .occlude()
                        .flex_none()
                        .children(controls)
                        .child(self.right_button(drawn, self.waiting_count(cx), cx)),
                ),
        )
    }
}
