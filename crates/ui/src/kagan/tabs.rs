use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Selectable as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Task, Window, actions, div,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, TerminalOp, Thread, ThreadId, ThreadOp, WindowOp};

use super::shown::{control, dispatch, tip};
use super::{Kagan, Shown};
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
    cx.on_action(|_: &NextTerminal, cx| {
        Workspace::dispatch(Op::Window(WindowOp::NextTerminal), cx).detach();
    });
    cx.on_action(|_: &PreviousTerminal, cx| {
        Workspace::dispatch(Op::Window(WindowOp::PreviousTerminal), cx).detach();
    });
    cx.on_action(|_: &OpenLocalLink, cx| {
        Workspace::dispatch(Op::Window(WindowOp::OpenLocalLink), cx).detach();
    });
}

pub(super) fn perform(op: WindowOp, cx: &mut App) {
    let Some((handle, kagan)) = super::front(cx) else {
        return;
    };
    let _ = cx.update_window(handle, |_, window, cx| {
        kagan.update(cx, |this, cx| match (op, this.tabs.local.clone()) {
            (WindowOp::OpenLocalLink, Some(link)) => cx.open_url(&link),
            (WindowOp::OpenLocalLink, None) => window.play_system_bell(),
            _ => this.step_tab(op == WindowOp::NextTerminal, window, cx),
        });
    });
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum RightPage {
    #[default]
    Panels,
    Browser,
    Terminal(u32),
}

#[derive(Default)]
pub(super) struct Tabs {
    thread: Option<ThreadId>,
    views: Vec<(u32, Entity<TerminalView>)>,
    page: RightPage,
    browser: bool,
    adding: bool,
    local: Option<String>,
    looking: Option<Task<()>>,
}

impl Tabs {
    pub(super) fn right_page(&self) -> RightPage {
        self.page
    }

    pub(super) fn set_font_size(&self, size: u8, cx: &mut App) {
        for (_, view) in &self.views {
            view.update(cx, |view, cx| view.set_font_size(size, cx));
        }
    }

    pub(super) fn view(&self, number: u32) -> Option<Entity<TerminalView>> {
        self.views
            .iter()
            .find(|(open, _)| *open == number)
            .map(|(_, view)| view.clone())
    }

    #[cfg(test)]
    pub(super) fn numbers(&self) -> Vec<u32> {
        self.views.iter().map(|(number, _)| *number).collect()
    }

    #[cfg(test)]
    pub(super) fn shown(&self) -> Option<u32> {
        match self.page {
            RightPage::Terminal(number) => Some(number),
            RightPage::Panels | RightPage::Browser => None,
        }
    }
}

impl Kagan {
    fn tabbed_thread(&self, cx: &App) -> Option<ThreadId> {
        match self.screen.shown() {
            Shown::Thread(view) => Some(view.read(cx).thread().clone()),
            Shown::Nothing | Shown::Start(_) => None,
        }
    }

    pub(super) fn tab_view(&self, agent: &Entity<TerminalView>) -> Entity<TerminalView> {
        match self.tabs.page {
            RightPage::Terminal(number) => self.tabs.view(number).unwrap_or_else(|| agent.clone()),
            RightPage::Panels | RightPage::Browser => agent.clone(),
        }
    }

    pub(super) fn sync_tabs(&mut self, cx: &mut Context<Self>) {
        let thread = self.tabbed_thread(cx);
        if self.tabs.thread != thread {
            self.tabs = Tabs {
                thread: thread.clone(),
                ..Tabs::default()
            };
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
        self.tabs
            .views
            .retain(|(number, _)| numbers.contains(number));
        for number in &numbers {
            if self.tabs.views.iter().all(|(open, _)| open != number) {
                let size = self.terminal_font_size;
                let view = cx.new(|cx| TerminalView::new(id.clone(), Some(*number), size, cx));
                self.tabs.views.push((*number, view));
            }
        }
        if let RightPage::Terminal(shown) = self.tabs.page
            && !numbers.contains(&shown)
        {
            self.tabs.page = RightPage::Panels;
        }
    }

    pub(super) fn show_right(
        &mut self,
        page: RightPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.page = page;
        self.tabs.adding = false;
        if page != RightPage::Panels && !self.layout.right_open {
            self.force_sidebar(false, true, window, cx);
        }
        match page {
            RightPage::Terminal(number) => {
                if let Some(view) = self.tabs.view(number) {
                    window.focus(&view.focus_handle(cx), cx);
                    self.typed_watch =
                        Some(cx.subscribe(&view, |this, _, _: &Typed, cx| this.typed(cx)));
                }
            }
            RightPage::Browser => self.browser.focus(window, cx),
            RightPage::Panels => {}
        }
        cx.notify();
    }

    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let order: Vec<RightPage> = std::iter::once(RightPage::Panels)
            .chain(self.tabs.browser.then_some(RightPage::Browser))
            .chain(
                self.tabs
                    .views
                    .iter()
                    .map(|(number, _)| RightPage::Terminal(*number)),
            )
            .collect();
        if order.len() < 2 || self.shown_thread(cx).is_none() {
            window.play_system_bell();
            return;
        }
        let at = order
            .iter()
            .position(|page| *page == self.tabs.page)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % order.len()
        } else {
            (at + order.len() - 1) % order.len()
        };
        self.show_right(order[next], window, cx);
    }

    pub(super) fn new_terminal(
        &mut self,
        _: &NewTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_right_terminal(window, cx);
    }

    pub(super) fn open_right_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.shown_open_thread(cx) else {
            return;
        };
        let opened = Workspace::dispatch(
            Op::Thread(ThreadOp::Terminal(TerminalOp::New { id: thread.id })),
            cx,
        );
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let opened = opened.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| match opened {
                    Ok(OpResult::Terminals(opened)) => {
                        this.note = None;
                        this.sync_tabs(cx);
                        if let Some(number) = opened.first().map(|extra| extra.number) {
                            this.show_right(RightPage::Terminal(number), window, cx);
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        this.note = Some(format!("Could not open a terminal: {error}").into());
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    pub(super) fn close_terminal(
        &mut self,
        _: &CloseTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_right(self.tabs.page, cx);
    }

    fn close_right(&mut self, page: RightPage, cx: &mut Context<Self>) {
        match page {
            RightPage::Terminal(number) => {
                if let Some(id) = self.shown_thread(cx) {
                    close(id, number, cx);
                }
            }
            RightPage::Browser => {
                self.tabs.browser = false;
                self.tabs.page = RightPage::Panels;
                cx.notify();
            }
            RightPage::Panels => {}
        }
    }

    pub(super) fn toggle_adding(&mut self, cx: &mut Context<Self>) {
        self.tabs.adding = !self.tabs.adding;
        cx.notify();
    }

    pub(super) fn add_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.browser = true;
        self.show_right(RightPage::Browser, window, cx);
    }

    pub(super) fn render_right_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut tabs: Vec<AnyElement> =
            vec![self.right_tab("right-tab-panels", "Panels".into(), RightPage::Panels, cx)];
        if self.tabs.browser {
            tabs.push(self.right_tab(
                "right-tab-browser",
                "Browser".into(),
                RightPage::Browser,
                cx,
            ));
        }
        for (number, _) in &self.tabs.views {
            tabs.push(self.right_tab(
                &format!("right-tab-{number}"),
                format!("Terminal {number}").into(),
                RightPage::Terminal(*number),
                cx,
            ));
        }
        let choices: Vec<AnyElement> = if self.tabs.adding {
            vec![
                control(
                    "right-add-browser",
                    Facts::button("Browser"),
                    Button::new("right-add-browser-button")
                        .small()
                        .outline()
                        .label("Browser")
                        .on_click(cx.listener(|this, _, window, cx| this.add_browser(window, cx))),
                )
                .into_any_element(),
                control(
                    "right-add-terminal",
                    Facts::button("Terminal"),
                    Button::new("right-add-terminal-button")
                        .small()
                        .outline()
                        .label("Terminal")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_right_terminal(window, cx);
                        })),
                )
                .into_any_element(),
            ]
        } else {
            Vec::new()
        };
        let add = control(
            "right-add",
            Facts::button("New tab"),
            Button::new("right-add-button")
                .small()
                .ghost()
                .icon(IconName::Plus)
                .accessibility_label("New tab")
                .on_click(cx.listener(|this, _, _, cx| this.toggle_adding(cx))),
        );
        h_flex()
            .id("right-tabs")
            .role(Role::TabList)
            .ui_container()
            .flex_none()
            .w_full()
            .min_w_0()
            .h_9()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .id("right-tab-list")
                    .min_w_0()
                    .overflow_x_scroll()
                    .gap_1()
                    .children(tabs),
            )
            .child(div().flex_1())
            .children(choices)
            .child(add)
            .into_any_element()
    }

    fn right_tab(
        &self,
        id: &str,
        label: SharedString,
        page: RightPage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.tabs.page == page;
        let button = Button::new(SharedString::from(format!("{id}-button")))
            .small()
            .ghost()
            .selected(selected)
            .label(label.clone())
            .on_click(cx.listener(move |this, _, window, cx| this.show_right(page, window, cx)));
        let close = (page != RightPage::Panels).then(|| {
            let name = format!("Close {label}");
            control(
                format!("{id}-close"),
                Facts::button(name.clone()),
                Button::new(SharedString::from(format!("{id}-close-button")))
                    .xsmall()
                    .ghost()
                    .icon(IconName::Close)
                    .accessibility_label(name)
                    .on_click(cx.listener(move |this, _, _, cx| this.close_right(page, cx))),
            )
        });
        h_flex()
            .flex_none()
            .items_center()
            .rounded(cx.theme().radius)
            .type_small()
            .child(control(
                id.to_owned(),
                Facts {
                    role: Role::Tab,
                    selected: Some(selected),
                    ..Facts::button(label)
                },
                button,
            ))
            .children(close)
            .into_any_element()
    }

    pub(super) fn render_tabs(&self, thread: &Thread, cx: &mut Context<Self>) -> AnyElement {
        let chip = self.local_link_chip(cx);
        let program = kg_domain::split_program(&thread.provider)
            .ok()
            .and_then(|(name, _)| {
                std::path::Path::new(&name)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| thread.provider.clone());
        let border = cx.theme().border;
        h_flex()
            .id("terminal-strip")
            .ui_container()
            .flex_none()
            .w_full()
            .min_w_0()
            .h_9()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .id("terminal-tab-agent")
                    .ui_observe_as(Facts {
                        role: Role::Tab,
                        selected: Some(true),
                        ..Facts::button(program.clone())
                    })
                    .type_small()
                    .child(program),
            )
            .child(div().flex_1())
            .children(chip)
            .into_any_element()
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

    /// "Open localhost:3000" at the end of the tab row, for the newest local link the thread printed.
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

/// The host and port of a link to this machine, such as `localhost:3000`.
fn local_host(link: &str) -> Option<&str> {
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

fn close(id: ThreadId, number: u32, cx: &mut App) {
    Workspace::dispatch(
        Op::Thread(ThreadOp::Terminal(TerminalOp::Close { id, number })),
        cx,
    )
    .detach();
}
