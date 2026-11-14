use gpui_kit::{Context, Window};

use crate::kagan::Kagan;
use crate::kagan::screen::Side;

use super::{Panes, key, right_index, save};

impl Kagan {
    pub(in crate::kagan) fn sync_panels(&mut self, cx: &mut Context<Self>) {
        let want = self.desired_panes(cx);
        self.sync_to(want, cx);
    }

    pub(in crate::kagan) fn sync_to(&mut self, want: Panes, cx: &mut Context<Self>) {
        if (want.right && self.screen.promote(Side::Right))
            || (want.left && self.screen.promote(Side::Left))
        {
            cx.notify();
        }
        let held = self.panes;
        if held == want {
            return;
        }
        self.panes = want;
        self.hide_browser(cx);
        let (left_width, right_width) = (self.layout.left_width, self.right_fit(want.left));
        self.columns.update(cx, |state, cx| {
            if held.right && !want.right && state.sizes().len() > right_index(held.left) {
                state.remove_panel(right_index(held.left), cx);
            }
            if held.left && !want.left && !state.sizes().is_empty() {
                state.remove_panel(0, cx);
            }
            if want.left && !held.left {
                state.insert_panel(Some(left_width), Some(0), cx);
            }
            if want.right && !held.right {
                state.insert_panel(Some(right_width), None, cx);
            }
        });
        cx.notify();
    }

    pub(in crate::kagan) fn restore_widths(
        &mut self,
        kept: Panes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Restoring makes resize events that are not the person's choice, so they are ignored.
        self.hold_resizes(cx);
        let index = right_index(self.panes.left);
        let (left_width, right_width) = (self.layout.left_width, self.right_fit(self.panes.left));
        self.columns.update(cx, |state, cx| {
            if kept.left {
                state.resize_panel(0, left_width, window, cx);
            }
            if kept.right {
                state.resize_panel(index, right_width, window, cx);
            }
        });
    }

    pub(in crate::kagan) fn resized(&mut self, cx: &mut Context<Self>) {
        if self.in_settings() || self.settling || self.hidden_by_width() {
            return;
        }
        let sizes = self.columns.read(cx).sizes().clone();
        let mut next = self.layout;
        if self.panes.left
            && let Some(width) = sizes.first().copied()
            && self.chosen_width(width)
        {
            next.left_width = width;
        }
        // A right width equal to its fit was set by the room, not chosen by the person.
        let fit = f32::from(self.right_fit(self.panes.left));
        if self.panes.right
            && let Some(width) = sizes.last()
            && (f32::from(*width) - fit).abs() > 0.5
        {
            next.right_width = *width;
        }
        if next.left_width != self.layout.left_width {
            save(&key(kg_domain::prefs::LEFT_WIDTH), next.left_width, cx);
        }
        if next.right_width != self.layout.right_width {
            save(&key(kg_domain::prefs::RIGHT_WIDTH), next.right_width, cx);
        }
        self.layout = next;
        cx.notify();
    }
}
