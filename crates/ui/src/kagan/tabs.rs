use crate::theme::{SpaceStep, TypeScale as _, radius_md, space};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Selectable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString, Styled as _,
    Subscription, Task, Window, actions, div, px,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, TerminalOp, ThreadId, ThreadOp, WindowOp};

use super::controls::{control, dispatch, tip};
use super::right::{ActivatePanelHeader, Tab};
use super::{Kagan, window};
use crate::terminal::{TerminalView, Typed};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

actions!(
    kagan,
    [
        NewTerminal,
        CloseTerminal,
        NextTerminal,
        PreviousTerminal,
        OpenLocalLink
    ]
);

pub(super) fn bind(cx: &mut App) {
    window::bind::<NextTerminal>(cx, WindowOp::NextTerminal);
    window::bind::<PreviousTerminal>(cx, WindowOp::PreviousTerminal);
    window::bind::<OpenLocalLink>(cx, WindowOp::OpenLocalLink);
}

pub(super) fn perform(op: WindowOp, cx: &mut App) {
    window::with_front(cx, |this, window, cx| match (op, this.tabs.local.clone()) {
        (WindowOp::OpenLocalLink, Some(link)) => cx.open_url(&link),
        (WindowOp::OpenLocalLink, None) => window.play_system_bell(),
        _ => this.step_tab(op == WindowOp::NextTerminal, window, cx),
    });
}

#[derive(Default)]
pub(super) struct Tabs {
    thread: Option<ThreadId>,
    views: Vec<(u32, Entity<TerminalView>, Subscription)>,
    pub(super) browser: bool,
    pub(super) preview: Option<super::preview::Preview>,
    local: Option<String>,
    looking: Option<Task<()>>,
}

impl Tabs {
    pub(super) fn set_font_size(&self, size: u8, cx: &mut App) {
        for (_, view, _) in &self.views {
            view.update(cx, |view, cx| view.set_font_size(size, cx));
        }
    }

    pub(super) fn numbers(&self) -> impl Iterator<Item = u32> + '_ {
        self.views.iter().map(|(number, ..)| *number)
    }

    pub(super) fn view(&self, number: u32) -> Option<&Entity<TerminalView>> {
        self.views
            .iter()
            .find(|(open, ..)| *open == number)
            .map(|(_, view, _)| view)
    }
}

impl Kagan {
    pub(super) fn sync_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let thread = self.screen.covered_thread(cx);
        if self.tabs.thread != thread {
            self.tabs = Tabs {
                thread: thread.clone(),
                ..Tabs::default()
            };
            self.browser.leave_thread(window, cx);
            if self
                .right
                .tab
                .as_ref()
                .is_some_and(|tab| matches!(tab, Tab::Browser | Tab::Terminal(_) | Tab::Preview))
            {
                self.right.tab = None;
            }
            // A thread's page outlives the switch; the tab comes back with the thread.
            if self
                .tabs
                .thread
                .as_ref()
                .is_some_and(|id| self.browser.owner() == Some(id))
            {
                self.tabs.browser = true;
            }
            self.load_local_link(cx);
        }
        let Some(id) = thread else {
            return;
        };
        let numbers: Vec<u32> = Workspace::global(cx)
            .read(cx)
            .extras(&id)
            .into_iter()
            .map(|extra| extra.number)
            .collect();
        let held = self.tabs.views.iter().any(|(number, view, _)| {
            !numbers.contains(number) && view.focus_handle(cx).contains_focused(window, cx)
        });
        self.tabs
            .views
            .retain(|(number, ..)| numbers.contains(number));
        for number in &numbers {
            if self.tabs.view(*number).is_none() {
                let size = self.terminal_font_size;
                let view = cx.new(|cx| TerminalView::new(id.clone(), Some(*number), size, cx));
                let typed = cx.subscribe(&view, |this, _, _: &Typed, cx| this.typed(cx));
                self.tabs.views.push((*number, view, typed));
            }
        }
        if self.right.tab.as_ref().is_some_and(
            |shown| matches!(shown, Tab::Terminal(number) if !numbers.contains(number)),
        ) {
            self.right.tab = self.default_right_tab(cx);
        }
        if held {
            self.focus_right_tab(window, cx);
        }
    }

    pub(super) fn show_right_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.select_right_tab(tab, cx);
        if !self.right_drawn() {
            self.show_right(window, cx);
        }
        self.focus_right_tab(window, cx);
    }

    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let order: Vec<Option<u32>> = std::iter::once(None)
            .chain(self.tabs.numbers().map(Some))
            .collect();
        if order.len() < 2 || self.shown_thread(cx).is_none() {
            window.play_system_bell();
            return;
        }
        let shown = self.focused_terminal(window, cx);
        let at = order.iter().position(|at| *at == shown).unwrap_or(0);
        let next = if forward {
            (at + 1) % order.len()
        } else {
            (at + order.len() - 1) % order.len()
        };
        match order[next] {
            Some(number) => self.show_right_tab(Tab::Terminal(number), window, cx),
            None => self.focus_work(window, cx),
        }
    }

    pub(super) fn new_terminal(&mut self, _: &NewTerminal, _: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.shown_open_thread(cx) else {
            return;
        };
        self.run_then(
            Op::Thread(ThreadOp::Terminal(TerminalOp::New { id: thread.id })),
            cx,
            |this, result, window, cx| match result {
                Ok(OpResult::Terminals(opened)) => {
                    this.note = None;
                    this.sync_tabs(window, cx);
                    if let Some(extra) = opened.first() {
                        this.show_right_tab(Tab::Terminal(extra.number), window, cx);
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    this.note = Some(format!("Could not open a terminal: {error}").into());
                    cx.notify();
                }
            },
        );
    }

    pub(super) fn close_terminal(
        &mut self,
        _: &CloseTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(number) = self.focused_terminal(window, cx) {
            self.close_right_terminal(number, window, cx);
        }
    }

    pub(super) fn right_holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.right.add_focus.is_focused(window)
            || self.browser.holds_focus(window, cx)
            || self
                .tabs
                .preview
                .as_ref()
                .is_some_and(|preview| preview.holds_focus(window, cx))
            || self.focused_terminal(window, cx).is_some()
            || self
                .right
                .slots
                .iter()
                .any(|slot| slot.focus.contains_focused(window, cx))
    }

    fn focused_terminal(&self, window: &Window, cx: &App) -> Option<u32> {
        self.tabs
            .views
            .iter()
            .find(|(_, view, _)| view.focus_handle(cx).contains_focused(window, cx))
            .map(|(number, ..)| *number)
    }

    fn close_right_terminal(&mut self, number: u32, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.shown_thread(cx) else {
            return;
        };
        self.run_then(
            Op::Thread(ThreadOp::Terminal(TerminalOp::Close { id, number })),
            cx,
            |this, result, window, cx| {
                if result.is_ok() {
                    this.sync_tabs(window, cx);
                    this.focus_right_tab(window, cx);
                }
            },
        );
    }

    pub(super) fn can_open_browser(&self, cx: &App) -> bool {
        self.thread_visible(cx) && !self.tabs.browser
    }

    pub(super) fn can_open_terminal(&self, cx: &App) -> bool {
        self.shown_open_thread(cx).is_some()
    }

    pub(super) fn open_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.browser = true;
        self.show_right_tab(Tab::Browser, window, cx);
    }

    fn close_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.browser = false;
        self.browser.close(window, cx);
        if self.right.tab == Some(Tab::Browser) {
            self.right.tab = self.default_right_tab(cx);
        }
        self.focus_right_tab(window, cx);
        cx.notify();
    }

    pub(super) fn right_tabs(
        &self,
        selected: Option<&Tab>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let browser = self.tabs.browser.then(|| {
            let close: CloseTab = (
                "right-tab-close-browser".into(),
                Box::new(Kagan::close_browser),
            );
            tab(
                "right-tab-browser",
                "Browser".into(),
                selected == Some(&Tab::Browser),
                |this, window, cx| this.show_right_tab(Tab::Browser, window, cx),
                Some(close),
                None,
                cx,
            )
        });
        let terminals: Vec<AnyElement> = self
            .tabs
            .numbers()
            .map(|number| {
                let id = Tab::Terminal(number).id().into_owned();
                let close: CloseTab = (
                    format!("right-tab-close-{id}").into(),
                    Box::new(move |this, window, cx| this.close_right_terminal(number, window, cx)),
                );
                let selected = selected == Some(&Tab::Terminal(number));
                tab(
                    format!("right-tab-{id}"),
                    format!("Terminal {number}").into(),
                    selected,
                    move |this, window, cx| this.show_right_tab(Tab::Terminal(number), window, cx),
                    Some(close),
                    None,
                    cx,
                )
            })
            .collect();
        let preview = self.tabs.preview.is_some().then(|| {
            let close: CloseTab = (
                "right-tab-close-preview".into(),
                Box::new(Kagan::close_preview),
            );
            tab(
                "right-tab-preview",
                "Preview".into(),
                selected == Some(&Tab::Preview),
                |this, window, cx| this.show_right_tab(Tab::Preview, window, cx),
                Some(close),
                None,
                cx,
            )
        });
        browser
            .into_iter()
            .chain(terminals)
            .chain(preview)
            .collect()
    }

    pub(super) fn render_strip(&self, cx: &App) -> Option<AnyElement> {
        let chip = self.local_link_chip(cx)?;
        Some(
            h_flex()
                .id("terminal-strip")
                .ui_container()
                .flex_none()
                .w_full()
                .min_w_0()
                .h_9()
                .px(px(space(SpaceStep::Sm, cx)))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(div().flex_1())
                .child(chip)
                .into_any_element(),
        )
    }
}

impl Kagan {
    pub(super) fn load_local_link(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.tabs.thread.clone() else {
            return;
        };
        let read = Workspace::dispatch(Op::Thread(ThreadOp::Links { id: id.clone() }), cx);
        self.tabs.looking = Some(cx.spawn(async move |this, cx| {
            let Ok(OpResult::Links(links)) = read.await else {
                return;
            };
            _ = this.update(cx, |this, cx| {
                let local = links
                    .into_iter()
                    .rev()
                    .find(|link| local_host(link).is_some());
                if this.tabs.thread.as_ref() == Some(&id) && this.tabs.local != local {
                    this.tabs.local = local;
                    cx.notify();
                }
            });
        }));
    }

    fn local_link_chip(&self, cx: &App) -> Option<AnyElement> {
        let host = local_host(self.tabs.local.as_deref()?)?;
        let label = format!("Open {host}");
        Some(
            control(
                "open-local-link",
                Facts::button(label.clone()),
                Button::new("open-local-link-button")
                    .small()
                    .outline()
                    .label(label.clone())
                    .tooltip(tip(&label, &OpenLocalLink, cx))
                    .on_click(dispatch(OpenLocalLink)),
            )
            .into_any_element(),
        )
    }
}

pub(super) fn local_host(link: &str) -> Option<&str> {
    let (_, rest) = link.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let name = match host.strip_prefix('[') {
        Some(inner) => &host[..inner.find(']')? + 2],
        None => host.split(':').next()?,
    };
    matches!(
        name,
        "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]" | "[::]"
    )
    .then_some(host)
}

pub(super) type CloseTab = (
    SharedString,
    Box<dyn Fn(&mut Kagan, &mut Window, &mut Context<Kagan>)>,
);

pub(super) fn tab(
    id: impl Into<SharedString>,
    label: SharedString,
    selected: bool,
    on_select: impl Fn(&mut Kagan, &mut Window, &mut Context<Kagan>) + Clone + 'static,
    close: Option<CloseTab>,
    focus: Option<FocusHandle>,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let id: SharedString = id.into();
    let select = on_select.clone();
    let button = Button::new(SharedString::from(format!("{id}-button")))
        .small()
        .ghost()
        .selected(selected)
        .label(label.clone())
        .when(focus.is_some(), |button| button.tab_stop(false))
        .on_click(cx.listener(move |this, _, window, cx| on_select(this, window, cx)));
    let close = close.map(|(close_id, close)| {
        let name = format!("Close {label}");
        control(
            close_id.clone(),
            Facts::button(name.clone()),
            Button::new(SharedString::from(format!("{close_id}-button")))
                .xsmall()
                .ghost()
                .icon(IconName::Close)
                .accessibility_label(name)
                .on_click(cx.listener(move |this, _, window, cx| close(this, window, cx))),
        )
    });
    let tab = h_flex()
        .flex_none()
        .items_center()
        .rounded(px(radius_md(cx)))
        .type_small(cx)
        .child(control(
            id,
            Facts {
                role: Role::Tab,
                selected: Some(selected),
                focus: focus.clone(),
                ..Facts::button(label)
            },
            button,
        ))
        .children(close);
    match focus {
        None => tab.into_any_element(),
        Some(focus) => tab
            .track_focus(&focus)
            .tab_stop(true)
            .key_context("PanelHeader")
            .focus(|tab| tab.border_1().border_color(cx.theme().ring))
            .on_action(
                cx.listener(move |this, _: &ActivatePanelHeader, window, cx| {
                    select(this, window, cx);
                }),
            )
            .into_any_element(),
    }
}
