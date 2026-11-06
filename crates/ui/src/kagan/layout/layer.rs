use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};

use crate::kagan::screen::{Activation, Side};
use crate::kagan::{Kagan, ToggleRightSidebar, ToggleSidebar, right};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::{LEFT_OVERLAY, RAIL_WIDTH, key, left_scrim, save};

impl Kagan {
    /// The sidebar buttons and shortcuts do nothing in Settings, where the sidebars are fixed.
    /// Showing a sidebar that has no room draws it over the center, as the right panels do.
    pub(in crate::kagan) fn toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_settings() {
            return;
        }
        if self.panes.left {
            save(&key("left-sidebar"), false, cx);
        } else if self.room_panes(true, false).left {
            save(&key("left-sidebar"), true, cx);
        } else if self.screen.drawn(Side::Left) {
            self.dismiss_overlay(Side::Left, window, cx);
        } else {
            self.open_overlay(Side::Left, Activation::Focus, window, cx);
        }
    }

    /// Shows the left sidebar beside the center when there is room, and over it when there is not.
    pub(in crate::kagan) fn show_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.room_panes(true, self.layout.right_open).left {
            self.force_sidebar(true, true, window, cx);
        } else {
            self.open_overlay(Side::Left, Activation::Focus, window, cx);
        }
    }

    pub(in crate::kagan) fn left_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.screen.drawn(Side::Left) || self.panes.left || self.in_settings() {
            return None;
        }
        let dismiss =
            cx.listener(|this, _, window, cx| this.dismiss_overlay(Side::Left, window, cx));
        let focus = self.left_overlay_focus.clone();
        let layer = div()
            .id("left-overlay")
            .ui_container()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("left-overlay-scrim")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(RAIL_WIDTH))
                    .right_0()
                    .bg(left_scrim(cx))
                    .occlude()
                    .on_click(dismiss)
                    .ui_observe_as(Facts::button("Dismiss sidebar")),
            )
            .child(
                div()
                    .id("left-overlay-panel")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(RAIL_WIDTH))
                    .w(self.layout.left_width)
                    .bg(cx.theme().sidebar)
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .occlude()
                    .key_context(LEFT_OVERLAY)
                    .track_focus(&focus)
                    .ui_observe_as(Facts {
                        hint: None,
                        role: Role::Unknown,
                        name: "Sidebar".into(),
                        value: None,
                        disabled: false,
                        selected: None,
                        focus: Some(focus.clone()),
                    })
                    .child(self.left_side(window, cx)),
            )
            .into_any_element();
        match self.screen.painted(Side::Left) {
            Some(Activation::Focus) => window.focus(&focus, cx),
            Some(Activation::SidebarRow) => {
                if let Some(id) = self.sidebar_target(cx) {
                    self.sidebar
                        .update(cx, |sidebar, cx| sidebar.reveal_row(&id, window, cx));
                } else {
                    window.focus(&focus, cx);
                }
            }
            Some(Activation::Quiet) | None => {}
        }
        Some(layer)
    }

    pub(in crate::kagan) fn toggle_right(
        &mut self,
        _: &ToggleRightSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_settings() || (!self.screen.drawn(Side::Right) && !self.has_right_content(cx)) {
            return;
        }
        if self.panes.right {
            if self.right_holds_focus(window, cx) {
                self.focus_work(window, cx);
            }
            self.right_fill = right::Fill::Column;
            self.dismiss_overlay(Side::Right, window, cx);
            save(&key("right-sidebar"), false, cx);
            return;
        }
        if !self.screen.drawn(Side::Right) && self.waiting_count(cx) > 0 {
            self.select_right_tab(crate::kagan::right::Tab::Questions, cx);
        }
        // Save the wide layout when the column fits, even while the overlay is up.
        if self.screen.review().is_none()
            && self.has_right_content(cx)
            && self.room_panes(self.layout.left_open, true).right
        {
            save(&key("right-sidebar"), true, cx);
            return;
        }
        if self.screen.drawn(Side::Right) {
            self.dismiss_overlay(Side::Right, window, cx);
            return;
        }
        self.open_overlay(Side::Right, Activation::Focus, window, cx);
    }

    /// The center column is drawn. Full screen on the right replaces it.
    pub(in crate::kagan) fn center_drawn(&self) -> bool {
        !(self.right_fill == right::Fill::Window && self.panes.right)
    }

    /// The right sidebar is on screen, as a column or as an overlay.
    pub(in crate::kagan) fn right_drawn(&self) -> bool {
        self.panes.right || (self.screen.drawn(Side::Right) && !self.in_settings())
    }

    pub(in crate::kagan) fn right_mounted(&self) -> bool {
        self.layout.right_open || self.screen.drawn(Side::Right)
    }
}
