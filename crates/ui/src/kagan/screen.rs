#[cfg(test)]
pub(crate) mod tests;
use gpui_kit::{App, Context, WeakFocusHandle, Window};

use super::{Kagan, Shown, archived, changes::Changes, layout::Panes, review, skills, surface};
use kg_domain::ThreadId;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy)]
pub(super) enum Activation {
    Focus,
    SidebarRow,
    Quiet,
}

struct Overlay {
    side: Side,
    activation: Option<Activation>,
}

pub(super) enum Center {
    Work(Shown),
    Review(Box<review::Review>),
    Page(Box<surface::Slot>),
    Settings,
    Archived(archived::List),
    Skills(gpui_kit::Entity<skills::Page>),
}

struct Pending {
    previous: Option<Center>,
    focus: Option<WeakFocusHandle>,
}

#[derive(Clone, Copy)]
pub(super) enum FocusReturn {
    Center,
    LeftOverlay,
    RightOverlay,
}

pub(super) struct Screen {
    draw: Option<Overlay>,
    current: Center,
    work: Shown,
    pending: Option<Pending>,
    center_focus: Option<WeakFocusHandle>,
    overlay_focus: Option<WeakFocusHandle>,
}

impl Screen {
    pub(super) fn new() -> Self {
        Self {
            draw: None,
            current: Center::Work(Shown::Nothing),
            work: Shown::Nothing,
            pending: None,
            center_focus: None,
            overlay_focus: None,
        }
    }

    pub(super) fn drawn(&self, side: Side) -> bool {
        self.draw
            .as_ref()
            .is_some_and(|overlay| overlay.side == side)
    }

    pub(super) fn promote(&mut self, side: Side) -> bool {
        if !self.drawn(side) {
            return false;
        }
        self.draw = None;
        self.overlay_focus = None;
        true
    }

    pub(super) fn painted(&mut self, side: Side) -> Option<Activation> {
        self.draw
            .as_mut()
            .filter(|overlay| overlay.side == side)
            .and_then(|overlay| overlay.activation.take())
    }

    pub(super) fn current(&self) -> &Center {
        &self.current
    }

    pub(super) fn shown(&self) -> &Shown {
        &self.work
    }

    pub(super) fn covered_thread(&self, cx: &App) -> Option<ThreadId> {
        match self.shown() {
            Shown::Thread(view) => Some(view.read(cx).thread().clone()),
            Shown::Nothing | Shown::Start(_) => None,
        }
    }

    pub(super) fn open(&mut self, center: Center) {
        if let Center::Work(shown) = &center {
            self.work = shown.clone();
        }
        let old = std::mem::replace(&mut self.current, center);
        if let Some(pending) = &mut self.pending
            && pending.previous.is_none()
        {
            pending.previous = Some(old);
        }
    }

    pub(super) fn close(&mut self) {
        self.open(Center::Work(self.work.clone()));
    }

    pub(super) fn review(&self) -> Option<&review::Review> {
        match &self.current {
            Center::Review(review) => Some(review),
            _ => None,
        }
    }

    pub(super) fn review_mut(&mut self) -> Option<&mut review::Review> {
        match &mut self.current {
            Center::Review(review) => Some(review),
            _ => None,
        }
    }

    pub(super) fn page(&self) -> Option<&surface::Slot> {
        match &self.current {
            Center::Page(page) => Some(page),
            _ => None,
        }
    }

    pub(super) fn committed_page(&self) -> Option<&surface::Slot> {
        match self
            .pending
            .as_ref()
            .and_then(|pending| pending.previous.as_ref())
            .unwrap_or(&self.current)
        {
            Center::Page(page) => Some(page),
            _ => None,
        }
    }

    pub(super) fn page_mut(&mut self) -> Option<&mut surface::Slot> {
        match &mut self.current {
            Center::Page(page) => Some(page),
            _ => None,
        }
    }

    pub(super) fn remember_center(&mut self, window: &Window, cx: &App) {
        self.center_focus = window.focused(cx).map(|focus| focus.downgrade());
    }

    pub(super) fn list_archived(&mut self, threads: Vec<kg_domain::Thread>) {
        if let Center::Archived(list) = &mut self.current {
            *list = archived::List::Listed(threads);
        }
    }

    pub(super) fn begin(&mut self) {
        self.pending = Some(Pending {
            previous: None,
            focus: self.center_focus.clone(),
        });
    }

    pub(super) fn commit(&mut self) {
        self.pending = None;
    }

    pub(super) fn rollback(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.center_focus = pending.focus;
            if let Some(previous) = pending.previous {
                self.current = previous;
            }
        }
    }
}

impl Kagan {
    pub(super) fn return_focus(
        &mut self,
        target_kind: FocusReturn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match target_kind {
            FocusReturn::Center => self.screen.center_focus.take(),
            FocusReturn::LeftOverlay | FocusReturn::RightOverlay => {
                self.screen.overlay_focus.take()
            }
        };
        let kagan = cx.entity();
        let current = window.focused(cx).map(|focus| focus.downgrade());
        window.defer(cx, move |window, cx| {
            kagan.update(cx, |this, cx| {
                if window.focused(cx) != current.and_then(|focus| focus.upgrade()) {
                    return;
                }
                if let Some(target) = target.and_then(|target| target.upgrade())
                    && (matches!(target_kind, FocusReturn::Center) || target != this.focus)
                    && this.focus.contains(&target, window)
                {
                    window.focus(&target, cx);
                } else {
                    match target_kind {
                        FocusReturn::LeftOverlay | FocusReturn::RightOverlay
                            if this.screen.review().is_some() =>
                        {
                            this.focus_review(window, cx);
                        }
                        FocusReturn::Center | FocusReturn::LeftOverlay => {
                            this.focus_work(window, cx);
                        }
                        FocusReturn::RightOverlay => window.focus(&this.right.toggle_focus, cx),
                    }
                }
            });
        });
    }

    pub(super) fn dismiss_overlay(
        &mut self,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.screen.drawn(side) {
            self.screen.draw = None;
            self.return_focus(
                match side {
                    Side::Left => FocusReturn::LeftOverlay,
                    Side::Right => FocusReturn::RightOverlay,
                },
                window,
                cx,
            );
            cx.notify();
        }
    }

    pub(super) fn open_overlay(
        &mut self,
        side: Side,
        activation: Activation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let other = match side {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        };
        self.dismiss_overlay(other, window, cx);
        if !self.screen.drawn(side) {
            self.screen.overlay_focus = window.focused(cx).map(|focus| focus.downgrade());
        }
        self.screen.draw = Some(Overlay {
            side,
            activation: Some(activation),
        });
        if side == Side::Right {
            self.load_changes(cx);
        }
        cx.notify();
    }

    pub(super) fn open_center(
        &mut self,
        center: Center,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.supersede_surface();
        if self.in_settings() && !matches!(center, Center::Settings) {
            self.hold_resizes(cx);
            self.screen.close();
            self.sync_to(
                Panes {
                    left: self.layout.left_open,
                    right: self.layout.right_open,
                },
                cx,
            );
            self.restore_widths(self.panes, window, cx);
        }
        if matches!(center, Center::Review(_) | Center::Settings) {
            self.dismiss_overlay(Side::Right, window, cx);
        }
        self.screen.open(center);
        self.read_shown(cx);
    }

    pub(super) fn show_work(&mut self, shown: Shown, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_overlay(Side::Left, window, cx);
        self.open_center(Center::Work(shown), window, cx);
        self.dismiss_thread_menus(cx);
        self.close_rename();
        self.note = None;
        self.changes = Changes::new();
    }
}
