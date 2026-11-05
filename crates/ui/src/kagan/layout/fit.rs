use gpui_kit::{App, Context, Pixels, Window, px};

use crate::kagan::{Kagan, right};

use super::{
    CENTER_MIN, LEFT_RANGE, Layout, Panes, RAIL_WIDTH, clamp_width, key, read, right_index, save,
    saved_or,
};

impl Kagan {
    pub(in crate::kagan) fn load_layout(cx: &mut Context<Self>) {
        let left_open = read::<bool>(&key("left-sidebar"), cx);
        let right_open = read::<bool>(&key("right-sidebar"), cx);
        let left_width = read::<Pixels>(&key("left-width"), cx);
        let right_width = read::<Pixels>(&key("right-width"), cx);
        let review_wrap = read::<bool>(&key("review-wrap"), cx);
        cx.spawn(async move |this, cx| {
            let defaults = Layout::default();
            let next = Layout {
                left_open: saved_or(left_open.await, defaults.left_open),
                right_open: saved_or(right_open.await, defaults.right_open),
                left_width: clamp_width(
                    saved_or(left_width.await, defaults.left_width),
                    LEFT_RANGE,
                ),
                right_width: clamp_width(
                    saved_or(right_width.await, defaults.right_width),
                    right::WIDTH_RANGE,
                ),
                review_wrap: saved_or(review_wrap.await, false),
            };
            _ = this.update_in(cx, |this, window, cx| this.apply_layout(next, window, cx));
        })
        .detach();
    }

    fn apply_layout(&mut self, next: Layout, window: &mut Window, cx: &mut Context<Self>) {
        let was = self.layout;
        if next == was {
            return;
        }
        self.layout = next;
        self.supersede_surface();
        let held = self.panes;
        self.columns.update(cx, |state, cx| {
            if held.left && next.left_width != was.left_width {
                state.resize_panel(0, next.left_width, window, cx);
            }
            if held.right && next.right_width != was.right_width {
                state.resize_panel(right_index(held.left), next.right_width, window, cx);
            }
        });
        self.sync_panels_keeping_widths(window, cx);
        cx.notify();
    }

    // A panel the sync just added is already sized by the columns group. Resizing it now reads stale
    // bounds, so restore widths only for panels that stayed.
    pub(in crate::kagan) fn sync_panels_keeping_widths(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.panes;
        self.sync_panels(cx);
        let kept = Panes {
            left: before.left && self.panes.left,
            right: before.right && self.panes.right,
        };
        self.restore_widths(kept, window, cx);
    }

    pub(in crate::kagan) fn room_panes(&self, left_open: bool, right_open: bool) -> Panes {
        let room = f32::from(self.viewport) - CENTER_MIN - RAIL_WIDTH;
        let left = f32::from(self.layout.left_width);
        let right = f32::from(self.layout.right_width);
        let mut panes = Panes {
            left: left_open,
            right: right_open,
        };
        if panes.right && (right > room || (panes.left && left + right > room)) {
            panes.right = false;
        }
        if panes.left && left > room {
            panes.left = false;
        }
        panes
    }

    pub(in crate::kagan) fn desired_panes(&self, cx: &App) -> Panes {
        if self.in_settings() {
            return Panes {
                left: false,
                right: false,
            };
        }
        self.room_panes(
            self.layout.left_open,
            self.layout.right_open && self.has_right_content(cx) && self.screen.review().is_none(),
        )
    }

    pub(in crate::kagan) fn force_sidebar(
        &mut self,
        left: bool,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (name, mut next) = (
            if left {
                "left-sidebar"
            } else {
                "right-sidebar"
            },
            self.layout,
        );
        let shown = if left {
            &mut next.left_open
        } else {
            &mut next.right_open
        };
        if *shown == open {
            return;
        }
        *shown = open;
        if !self.surface_hold {
            save(&key(name), open, cx);
        }
        self.apply_layout(next, window, cx);
        self.sync_panels(cx);
    }

    // Call only after the painted frame has kept these open flags.
    pub(in crate::kagan) fn save_open_change(&self, before: Layout, cx: &mut App) {
        if self.layout.left_open != before.left_open {
            save(&key("left-sidebar"), self.layout.left_open, cx);
        }
        if self.layout.right_open != before.right_open {
            save(&key("right-sidebar"), self.layout.right_open, cx);
        }
    }

    pub(in crate::kagan) fn fit_to_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let changed = self.viewport != window.viewport_size().width;
        self.viewport = window.viewport_size().width;
        if self.desired_panes(cx) != self.panes {
            cx.defer_in(window, |this, window, cx| {
                this.sync_panels(cx);
                cx.defer_in(window, |this, window, cx| {
                    this.restore_widths(this.panes, window, cx);
                });
            });
            return;
        }
        if changed {
            let panes = self.panes;
            cx.defer_in(window, move |this, window, cx| {
                this.restore_widths(panes, window, cx);
            });
            return;
        }
        if !self.settling && self.needs_preferred(cx) {
            cx.defer_in(window, |this, window, cx| {
                this.restore_preferred(window, cx);
            });
        }
    }

    fn needs_preferred(&self, cx: &App) -> bool {
        if !self.panes.left {
            return false;
        }
        let want = self.layout.left_width;
        let Some(have) = self.columns.read(cx).sizes().first().copied() else {
            return false;
        };
        have + px(0.5) < want && self.side_fits(want)
    }

    pub(in crate::kagan) fn hold_resizes(&mut self, cx: &mut Context<Self>) {
        self.settling = true;
        cx.spawn(async |this, cx| {
            _ = this.update(cx, |this, _| this.settling = false);
        })
        .detach();
    }

    fn restore_preferred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settling || !self.needs_preferred(cx) {
            return;
        }
        let want = self.layout.left_width;
        self.hold_resizes(cx);
        self.columns
            .update(cx, |state, cx| state.resize_panel(0, want, window, cx));
    }

    fn side_fits(&self, width: Pixels) -> bool {
        let right = if self.panes.right {
            f32::from(self.layout.right_width)
        } else {
            0.
        };
        f32::from(self.viewport) - CENTER_MIN - RAIL_WIDTH - right + 0.5 >= f32::from(width)
    }

    pub(super) fn chosen_width(&self, width: Pixels) -> bool {
        width + px(0.5) >= self.layout.left_width || self.side_fits(self.layout.left_width)
    }

    /// Whether the window has hidden a sidebar the layout shows. Widths are not saved while it
    /// has: the sidebar beside a hidden one takes its room, and that is not a width the person
    /// chose.
    pub(in crate::kagan) fn hidden_by_width(&self) -> bool {
        self.room_panes(self.layout.left_open, self.layout.right_open)
            != Panes {
                left: self.layout.left_open,
                right: self.layout.right_open,
            }
    }
}
