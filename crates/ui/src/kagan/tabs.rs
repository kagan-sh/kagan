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

/// The extra terminals of the thread on screen, which tab shows (`None` is the program's terminal),
/// and the newest local link its terminals printed.
#[derive(Default)]
pub(super) struct Tabs {
    thread: Option<ThreadId>,
    views: Vec<(u32, Entity<TerminalView>)>,
    shown: Option<u32>,
    local: Option<String>,
    looking: Option<Task<()>>,
}

impl Tabs {
    pub(super) fn set_font_size(&self, size: u8, cx: &mut App) {
        for (_, view) in &self.views {
            view.update(cx, |view, cx| view.set_font_size(size, cx));
        }
    }

    #[cfg(test)]
    pub(super) fn numbers(&self) -> Vec<u32> {
        self.views.iter().map(|(number, _)| *number).collect()
    }

    #[cfg(test)]
    pub(super) fn shown(&self) -> Option<u32> {
        self.shown
    }
}

impl Kagan {
    fn tabbed_thread(&self, cx: &App) -> Option<ThreadId> {
        match self.screen.shown() {
            Shown::Thread(view) => Some(view.read(cx).thread().clone()),
            Shown::Nothing | Shown::Start(_) => None,
        }
    }

    /// The view the center draws for a thread: the shown extra terminal, or the program's.
    pub(super) fn tab_view(&self, agent: &Entity<TerminalView>) -> Entity<TerminalView> {
        self.tabs
            .shown
            .and_then(|shown| self.tabs.views.iter().find(|(number, _)| *number == shown))
            .map_or_else(|| agent.clone(), |(_, view)| view.clone())
    }

    /// Follows the host's list of extra terminals. A closed tab gives its place to the one before it.
    pub(super) fn sync_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        if let Some(shown) = self.tabs.shown
            && !numbers.contains(&shown)
        {
            let before = numbers
                .iter()
                .rev()
                .find(|number| **number < shown)
                .copied();
            self.show_tab(before, window, cx);
        }
    }

    pub(super) fn show_tab(
        &mut self,
        number: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.shown = number;
        if let Shown::Thread(agent) = self.screen.shown().clone() {
            let view = self.tab_view(&agent);
            window.focus(&view.focus_handle(cx), cx);
            self.typed_watch = Some(cx.subscribe(&view, |this, _, _: &Typed, cx| this.typed(cx)));
        }
        cx.notify();
    }

    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let order: Vec<Option<u32>> = std::iter::once(None)
            .chain(self.tabs.views.iter().map(|(number, _)| Some(*number)))
            .collect();
        if order.len() < 2 || self.shown_thread(cx).is_none() {
            window.play_system_bell();
            return;
        }
        let at = order
            .iter()
            .position(|number| *number == self.tabs.shown)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % order.len()
        } else {
            (at + order.len() - 1) % order.len()
        };
        self.show_tab(order[next], window, cx);
    }

    pub(super) fn new_terminal(
        &mut self,
        _: &NewTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                        this.sync_tabs(window, cx);
                        let number = opened.first().map(|extra| extra.number);
                        this.show_tab(number, window, cx);
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
        if let (Some(id), Some(number)) = (self.shown_thread(cx), self.tabs.shown) {
            close(id, number, cx);
        }
    }

    /// The tab row over the terminal, drawn only while the thread has an extra terminal.
    pub(super) fn render_tabs(
        &self,
        thread: &Thread,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chip = self.local_link_chip(cx);
        if self.tabs.views.is_empty() && chip.is_none() {
            return None;
        }
        let program = kg_domain::split_program(&thread.provider)
            .ok()
            .and_then(|(name, _)| {
                std::path::Path::new(&name)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| thread.provider.clone());
        let agent = tab(
            "terminal-tab-agent",
            program.into(),
            self.tabs.shown.is_none(),
            None,
            cx,
        );
        let border = cx.theme().border;
        let extras: Vec<AnyElement> = self
            .tabs
            .views
            .iter()
            .map(|(number, _)| {
                let selected = self.tabs.shown == Some(*number);
                tab(
                    format!("terminal-tab-{number}"),
                    format!("Terminal {number}").into(),
                    selected,
                    Some((thread.id.clone(), *number)),
                    cx,
                )
            })
            .collect();
        let tabs = (!extras.is_empty()).then(|| {
            h_flex()
                .id("terminal-tabs")
                .role(Role::TabList)
                .ui_container()
                .min_w_0()
                .overflow_x_scroll()
                .gap_1()
                .child(agent)
                .children(extras)
        });
        Some(
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
                .children(tabs)
                .child(div().flex_1())
                .children(chip)
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

fn tab(
    id: impl Into<SharedString>,
    label: SharedString,
    selected: bool,
    extra: Option<(ThreadId, u32)>,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let id: SharedString = id.into();
    let number = extra.as_ref().map(|(_, number)| *number);
    let button = Button::new(SharedString::from(format!("{id}-button")))
        .small()
        .ghost()
        .selected(selected)
        .label(label.clone())
        .on_click(cx.listener(move |this, _, window, cx| this.show_tab(number, window, cx)));
    let close = extra.map(|(thread, number)| {
        let name = format!("Close {label}");
        control(
            format!("terminal-close-{number}"),
            Facts::button(name.clone()),
            Button::new(SharedString::from(format!(
                "terminal-close-{number}-button"
            )))
            .xsmall()
            .ghost()
            .icon(IconName::Close)
            .accessibility_label(name)
            .on_click(move |_, _, cx| close(thread.clone(), number, cx)),
        )
    });
    h_flex()
        .flex_none()
        .items_center()
        .rounded(cx.theme().radius)
        .type_small()
        .child(control(
            id,
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

/// The + in the thread's title row, which opens another terminal in its folder.
pub(super) fn new_terminal_button(cx: &App) -> impl IntoElement {
    control(
        "new-terminal",
        Facts::button("New terminal"),
        Button::new("new-terminal-button")
            .small()
            .ghost()
            .icon(IconName::Plus)
            .accessibility_label("New terminal")
            .tooltip(tip("New terminal", &NewTerminal, cx))
            .on_click(dispatch(NewTerminal)),
    )
}
