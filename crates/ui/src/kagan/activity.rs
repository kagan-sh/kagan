use std::collections::VecDeque;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, Styled as _, Subscription, Window, px,
};
use kg_core::{Updater, Workspace};
use kg_domain::{Event, EventKind, ThreadId, UpdateState};
use kg_plugins::{PluginStrip, StatusItem};

use super::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

mod log;
mod plugins;
mod summary;

use log::strip_log;

const HEIGHT: f32 = 28.;
const KEEP: usize = 100;
const ROWS: u16 = 7;
const ROW: f32 = 20.;
const DOT: f32 = 6.;
const PLUGIN_WIDTH: f32 = 720.;
const PLUGIN_ITEM_MAX: f32 = 200.;
const PLUGIN_ITEMS: usize = 4;

pub(super) const CONTEXT: &str = "ActivityStrip";

pub(super) struct Strip {
    toggle_focus: FocusHandle,
    open: bool,
    next: u64,
    lines: VecDeque<(u64, SharedString)>,
    update: Option<SharedString>,
    install: Option<SharedString>,
}

impl Strip {
    pub(super) fn new(toggle_focus: FocusHandle) -> Self {
        Self {
            toggle_focus,
            open: false,
            next: 0,
            lines: VecDeque::new(),
            update: None,
            install: None,
        }
    }

    fn open(&self) -> bool {
        self.open
    }

    fn say(&mut self, line: String) {
        if self.lines.len() == KEEP {
            self.lines.pop_front();
        }
        self.lines.push_back((self.next, line.into()));
        self.next += 1;
    }

    fn toggle(&mut self) {
        self.open = !self.open;
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn turn(&mut self, event: &Event, title: &str) {
        match event.kind {
            EventKind::TurnStarted => self.say(format!("{title}: turn started")),
            EventKind::TurnEnded { .. } => self.say(format!("{title}: turn ended")),
            _ => {}
        }
    }

    fn needs(&mut self, title: &str) {
        self.say(format!("{title}: needs you"));
    }

    pub(super) fn plugin_command(&mut self, plugin: &str) {
        self.say(format!("{plugin}: ran a command"));
    }

    pub(super) fn update_state(&mut self, state: &UpdateState) {
        let line = match state {
            UpdateState::Available { version, .. } => format!("kagan {version} is available"),
            UpdateState::Downloading { version } => format!("Downloading kagan {version}…"),
            UpdateState::Ready { version, .. } => format!("kagan {version} is ready to install"),
            _ => return,
        };
        if self.update.as_deref() == Some(line.as_str()) {
            return;
        }
        self.update = Some(line.clone().into());
        self.say(line);
    }

    pub(super) fn installing(&mut self, running: Option<&str>) {
        let line = running.map(|name| format!("Installing {name}"));
        if self.install.as_deref() == line.as_deref() {
            return;
        }
        if let Some(line) = line {
            self.install = Some(line.clone().into());
            self.say(line);
        } else {
            self.install = None;
        }
    }

    fn tail(&self) -> Vec<(u64, SharedString)> {
        self.lines
            .iter()
            .rev()
            .take(usize::from(ROWS))
            .rev()
            .cloned()
            .collect()
    }

    #[cfg(test)]
    fn kept(&self) -> usize {
        self.lines.len()
    }
}

fn title_of(id: &ThreadId, cx: &App) -> String {
    Workspace::global(cx)
        .read(cx)
        .thread(id)
        .map(|thread| thread.read(cx).record().title.clone())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Thread".into())
}

pub(super) fn watch(cx: &mut Context<Kagan>) -> Subscription {
    cx.observe(&Updater::global(cx), |this, updater, cx| {
        let state = updater.read(cx).state().clone();
        this.activity.update_state(&state);
        cx.notify();
    })
}

impl Kagan {
    pub(super) fn saw_turn(&mut self, event: &Event, cx: &App) {
        let title = title_of(&event.thread_id, cx);
        self.activity.turn(event, &title);
    }

    pub(super) fn saw_needs(&mut self, id: &ThreadId, cx: &App) {
        if self.shown_thread(cx).as_ref() == Some(id) {
            return;
        }
        let title = title_of(id, cx);
        self.activity.needs(&title);
    }

    // The threads `NextThreadNeedingYou` reaches. The Dock badge is wider, because it counts the
    // shown thread too.
    fn needs_you(&self, cx: &App) -> usize {
        let shown = self.shown_thread(cx);
        self.listed(cx)
            .filter(|thread| Some(&thread.id) != shown.as_ref())
            .filter(|thread| self.questions.read(cx).needs_you(thread))
            .count()
    }

    #[cfg(test)]
    pub(crate) fn activity_toggle_focus(&self) -> FocusHandle {
        self.activity.toggle_focus.clone()
    }

    pub(super) fn toggle_log(&mut self, cx: &mut Context<Self>) {
        self.activity.toggle();
        cx.notify();
    }

    pub(super) fn close_log(&mut self, cx: &mut Context<Self>) {
        if self.activity.open() {
            self.activity.close();
            cx.notify();
        }
    }

    pub(super) fn activity_strip(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let working = Workspace::global(cx).read(cx).busy_threads(cx);
        let needs = self.needs_you(cx);
        let open = self.activity.open();
        let summary: SharedString = match working {
            0 => "Quiet".into(),
            1 => "1 agent working".into(),
            count => format!("{count} agents working").into(),
        };
        let prefs = self.prefs.read(cx);
        let plugins: Vec<StatusItem> = if prefs.loaded
            && prefs.plugin_status
            && f32::from(window.viewport_size().width) >= PLUGIN_WIDTH
        {
            PluginStrip::global(cx)
                .map(|strip| strip.read(cx).items().take(PLUGIN_ITEMS).cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let log = strip_log(self.activity.tail(), cx);
        let summary_row = self.strip_summary(summary, plugins, working, needs, open, cx);
        let height = px(if open {
            HEIGHT + f32::from(ROWS) * ROW
        } else {
            HEIGHT
        });
        v_flex()
            .id("activity-strip")
            .ui_observe_as(Facts {
                role: Role::Unknown,
                name: "Activity strip".into(),
                value: None,
                disabled: false,
                selected: None,
                focus: None,
                hint: None,
            })
            .flex_none()
            .w_full()
            .h(height)
            .overflow_hidden()
            .bg(cx.theme().background)
            .border_t_1()
            .border_color(cx.theme().border)
            .key_context(CONTEXT)
            .child(summary_row)
            .when(open, |strip| strip.child(log))
    }
}

#[cfg(test)]
mod tests;
