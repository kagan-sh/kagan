use std::borrow::Cow;

use gpui_kit::{App, Context, Focusable as _, Window};

use super::selected_tab;
use crate::kagan::Kagan;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::kagan) enum Tab {
    Questions,
    Changes,
    Details,
    Browser,
    Terminal(u32),
    Preview,
    Plugin(String),
}

impl Tab {
    pub(in crate::kagan) fn id(&self) -> Cow<'_, str> {
        match self {
            Self::Questions => "needs-you".into(),
            Self::Changes => "changes".into(),
            Self::Details => "details".into(),
            Self::Browser => "browser".into(),
            Self::Terminal(number) => format!("terminal-{number}").into(),
            Self::Preview => "preview".into(),
            Self::Plugin(id) => id.as_str().into(),
        }
    }

    pub(in crate::kagan) fn available(&self, kagan: &Kagan, cx: &App) -> bool {
        match self {
            Self::Questions => kagan.waiting_count(cx) > 0,
            Self::Changes | Self::Details => kagan.thread_visible(cx),
            Self::Browser => kagan.tabs.browser && kagan.thread_visible(cx),
            Self::Preview => kagan.tabs.preview.is_some() && kagan.thread_visible(cx),
            Self::Terminal(number) => {
                kagan.tabs.view(*number).is_some() && kagan.thread_visible(cx)
            }
            Self::Plugin(id) => kagan.right_slots.iter().any(|slot| slot.id == *id),
        }
    }

    pub(in crate::kagan) fn focus(
        &self,
        kagan: &mut Kagan,
        window: &mut Window,
        cx: &mut Context<Kagan>,
    ) {
        let focus = match self {
            Self::Browser if cfg!(any(target_os = "macos", target_os = "windows")) => {
                return kagan.browser.focus(window, cx);
            }
            Self::Terminal(number) => kagan.tabs.view(*number).map(|view| view.focus_handle(cx)),
            Self::Preview => kagan
                .tabs
                .preview
                .as_ref()
                .map(crate::kagan::preview::Preview::focus_handle),
            Self::Plugin(id) => kagan
                .right_slots
                .iter()
                .find(|slot| slot.id == *id)
                .map(|slot| slot.focus.clone()),
            Self::Questions | Self::Changes | Self::Details | Self::Browser => None,
        };
        window.focus(&focus.unwrap_or_else(|| kagan.right_add_focus.clone()), cx);
    }
}

impl Kagan {
    pub(in crate::kagan) fn focus_right_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = selected_tab(self, cx) else {
            return window.focus(&self.right_add_focus, cx);
        };
        tab.focus(self, window, cx);
    }

    pub(in crate::kagan) fn default_right_tab(&self, cx: &App) -> Option<Tab> {
        if self.waiting_count(cx) > 0 {
            return Some(Tab::Questions);
        }
        if self.thread_visible(cx) {
            return Some(Tab::Changes);
        }
        self.right_slots
            .first()
            .map(|slot| Tab::Plugin(slot.id.clone()))
    }

    pub(in crate::kagan) fn select_right_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.right_tab.as_ref() == Some(&tab) {
            return;
        }
        self.right_tab = Some(tab);
        self.supersede_surface();
        cx.notify();
    }
}
