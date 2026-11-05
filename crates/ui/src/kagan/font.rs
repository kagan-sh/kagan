use super::toast::Toast;
use super::{ActualSize, Bigger, Kagan, Shown, Smaller, layout, web_panel};

use gpui_kit::{Context, Subscription, Window};

impl Kagan {
    pub(super) fn bigger(&mut self, _: &Bigger, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_size(self.terminal_font_size.saturating_add(1), cx);
    }

    pub(super) fn smaller(&mut self, _: &Smaller, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_size(self.terminal_font_size.saturating_sub(1), cx);
    }

    pub(super) fn actual_size(&mut self, _: &ActualSize, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_size(12, cx);
    }

    pub(super) fn theme_paint(cx: &mut Context<Self>) -> Subscription {
        cx.observe(&crate::theme::ThemeState::global(cx), |_, _, cx| {
            web_panel::repaint(cx);
            cx.notify();
        })
    }

    pub(super) fn theme_notice(cx: &mut Context<Self>) -> Subscription {
        let theme = crate::theme::ThemeState::global(cx);
        cx.subscribe(&theme, |this, _, notice: &crate::theme::Notice, cx| {
            this.show_toast(Toast::said(notice.0.clone()), cx);
        })
    }
    pub(super) fn apply_terminal_size(&mut self, size: u8, cx: &mut Context<Self>) {
        self.terminal_font_size = size;
        if let Shown::Thread(view) = self.screen.shown() {
            view.update(cx, |view, cx| view.set_font_size(size, cx));
        }
        self.tabs.set_font_size(size, cx);
    }

    pub(super) fn change_terminal_size(&mut self, size: u8, cx: &mut Context<Self>) {
        let size = size.clamp(9, 24);
        self.terminal_font_epoch = self.terminal_font_epoch.wrapping_add(1);
        self.apply_terminal_size(size, cx);
        layout::save("terminal-font-size", size, cx);
    }

    pub(super) fn load_terminal_size(&mut self, cx: &mut Context<Self>) {
        self.terminal_font_epoch = self.terminal_font_epoch.wrapping_add(1);
        let epoch = self.terminal_font_epoch;
        let read = layout::read::<u8>("terminal-font-size", cx);
        cx.spawn(async move |this, cx| {
            let size = read.await.ok().flatten().unwrap_or(12).clamp(9, 24);
            _ = this.update(cx, |this, cx| {
                if this.terminal_font_epoch == epoch {
                    this.apply_terminal_size(size, cx);
                }
            });
        })
        .detach();
    }
}
