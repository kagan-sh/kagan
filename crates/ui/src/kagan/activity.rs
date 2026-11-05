//! The activity strip at the bottom of the window: what the workspace is doing, in one line, and
//! a short log when opened (decision 148).

use std::collections::VecDeque;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px,
};
use kg_core::{Updater, Workspace};
use kg_domain::{Event, EventKind, ThreadId, UpdateState};
use kg_plugins::{PluginStrip, StatusItem, StatusTone};

use super::Kagan;
use crate::theme::{Tone, TypeScale as _};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const HEIGHT: f32 = 28.;
const KEEP: usize = 100;
const ROWS: u16 = 7;
const ROW: f32 = 20.;
const DOT: f32 = 6.;
const PLUGIN_WIDTH: f32 = 720.;
const PLUGIN_ITEM_MAX: f32 = 200.;
const PLUGIN_ITEMS: usize = 4;

pub(super) struct Strip {
    focus: FocusHandle,
    open: bool,
    next: u64,
    lines: VecDeque<(u64, SharedString)>,
    update: Option<SharedString>,
    install: Option<SharedString>,
}

impl Strip {
    pub(super) fn new(focus: FocusHandle) -> Self {
        Self {
            focus,
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

    /// Tracks the one plugin install the host allows at a time.
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

    /// Leaves out the shown thread, as the summary's needs count does.
    pub(super) fn saw_needs(&mut self, id: &ThreadId, cx: &App) {
        if self.shown_thread(cx).as_ref() == Some(id) {
            return;
        }
        let title = title_of(id, cx);
        self.activity.needs(&title);
    }

    /// The threads `NextThreadNeedingYou` reaches. The Dock badge is wider, because it counts the
    /// shown thread too.
    fn needs_you(&self, cx: &App) -> usize {
        let shown = self.shown_thread(cx);
        let questions = self.questions.read(cx).questions.clone();
        self.listed(cx)
            .filter(|thread| Some(&thread.id) != shown.as_ref())
            .filter(|thread| {
                thread.unread() || questions.iter().any(|asked| asked.thread_id == thread.id)
            })
            .count()
    }

    fn clicked_strip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.activity.open() {
            window.focus(&self.activity.focus, cx);
        }
        self.activity.toggle();
        cx.notify();
    }

    /// Enter and Space arrive as the click GPUI makes on a focused element.
    fn strip_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" && self.activity.open() {
            self.activity.close();
            cx.notify();
        }
    }

    fn strip_summary(
        &self,
        summary: SharedString,
        plugins: Vec<StatusItem>,
        working: usize,
        needs: usize,
        open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let dot = div()
            .flex_none()
            .size(px(DOT))
            .rounded_full()
            .bg(if working > 0 {
                Tone::Busy.color(cx)
            } else {
                cx.theme().muted_foreground
            });
        let head = h_flex()
            .id("activity-summary")
            .ui_observe_as(Facts {
                role: Role::Button,
                name: "Activity".into(),
                value: Some(summary.clone()),
                disabled: false,
                selected: Some(open),
                focus: None,
                hint: Some(
                    if open {
                        "Hide what the workspace is doing"
                    } else {
                        "Show what the workspace is doing"
                    }
                    .into(),
                ),
            })
            .flex_1()
            .min_w(px(120.))
            .items_center()
            .gap_2()
            .child(dot)
            .child(div().min_w_0().text_ellipsis().child(summary));
        let needs_part = (needs > 0).then(|| {
            div()
                .id("activity-needs")
                .flex_none()
                .type_small(cx)
                .text_color(Tone::Needs.color(cx))
                .on_click(cx.listener(|_, _, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(super::navigation::NextThreadNeedingYou), cx);
                }))
                .ui_observe_as(Facts {
                    role: Role::Button,
                    name: "Needs you".into(),
                    value: Some(needs.to_string().into()),
                    disabled: false,
                    selected: None,
                    focus: None,
                    hint: Some("Go to the next thread that needs you".into()),
                })
                .child(format!("{needs} needs you"))
        });
        let available = {
            let updater = Updater::global(cx);
            let updater = updater.read(cx);
            match updater.state() {
                UpdateState::Available { version, .. } => {
                    super::update::hidden(cx).as_deref() != Some(version.as_str())
                }
                _ => false,
            }
        };
        let update_part = available.then(|| {
            div()
                .id("activity-update")
                .ui_container()
                .flex_none()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child("Update available")
        });
        let install_part = self.activity.install.clone().map(|install| {
            div()
                .id("activity-install")
                .ui_container()
                .flex_none()
                .max_w(px(240.))
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(install)
        });
        h_flex()
            .h(px(HEIGHT))
            .flex_none()
            .items_center()
            .gap_3()
            .px_3()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child(head)
            .children(needs_part)
            .children(update_part)
            .children(install_part)
            .children(plugins.into_iter().map(|item| plugin_item(item, cx)))
    }

    /// Takes height only: the sidebars' width preferences do not account for it.
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
                focus: Some(self.activity.focus.clone()),
                hint: None,
            })
            .flex_none()
            .w_full()
            .h(height)
            .overflow_hidden()
            .border_t_1()
            .border_color(cx.theme().border)
            .key_context("ActivityStrip")
            .focus_visible(|strip| strip.border_1().border_color(cx.theme().ring))
            .on_click(cx.listener(|this, _, window, cx| this.clicked_strip(window, cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.strip_key(event, cx);
            }))
            .child(summary_row)
            .when(open, |strip| strip.child(log))
    }
}

fn plugin_item(item: StatusItem, cx: &App) -> impl IntoElement + use<> {
    let color = match item.tone {
        StatusTone::Quiet => cx.theme().muted_foreground,
        StatusTone::Busy => Tone::Busy.color(cx),
        StatusTone::Needs => Tone::Needs.color(cx),
        StatusTone::Failed => Tone::Failed.color(cx),
    };
    h_flex()
        .id(ElementId::Name(
            format!("activity-plugin-{}", item.id.0).into(),
        ))
        .ui_observe_as(Facts {
            role: Role::Status,
            name: item.title.clone().into(),
            value: Some(item.text.clone().into()),
            focus: None,
            hint: None,
            disabled: false,
            selected: None,
        })
        .flex_shrink_1()
        .max_w(px(PLUGIN_ITEM_MAX))
        .min_w_0()
        .items_center()
        .gap_1()
        .child(
            div()
                .min_w_0()
                .max_w(px(PLUGIN_ITEM_MAX / 2.))
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(item.title),
        )
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(color)
                .child(item.text),
        )
}

fn strip_log(rows: Vec<(u64, SharedString)>, cx: &App) -> impl IntoElement + use<> {
    let rows: Vec<AnyElement> = if rows.is_empty() {
        vec![
            div()
                .id("activity-empty")
                .aria_label("Nothing has happened yet.")
                .ui_container()
                .flex_none()
                .h(px(ROW))
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child("Nothing has happened yet.")
                .into_any_element(),
        ]
    } else {
        rows.into_iter()
            .map(|(seq, line)| {
                h_flex()
                    .id(ElementId::Name(format!("activity-row-{seq}").into()))
                    .aria_label(line.clone())
                    .ui_container()
                    .flex_none()
                    .h(px(ROW))
                    .min_w_0()
                    .items_center()
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(div().min_w_0().text_ellipsis().child(line))
                    .into_any_element()
            })
            .collect()
    };
    v_flex()
        .id("activity-log")
        .ui_container()
        .flex_1()
        .min_h_0()
        .px_3()
        .children(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(cx: &mut gpui_kit::TestAppContext) -> Strip {
        Strip::new(cx.update(|cx| cx.focus_handle()))
    }

    #[gpui_kit::test]
    fn the_log_keeps_the_last_hundred_lines(cx: &mut gpui_kit::TestAppContext) {
        let mut strip = strip(cx);
        for line in 0..120 {
            strip.say(line.to_string());
        }
        assert_eq!(strip.kept(), KEEP);
        assert_eq!(
            strip.lines.front().map(|(_, line)| line.as_ref()),
            Some("20")
        );
    }

    #[gpui_kit::test]
    fn the_log_shows_the_last_seven_oldest_first(cx: &mut gpui_kit::TestAppContext) {
        let mut strip = strip(cx);
        for line in 1..=10 {
            strip.say(line.to_string());
        }
        let lines: Vec<String> = strip
            .tail()
            .into_iter()
            .map(|(_, line)| line.to_string())
            .collect();
        assert_eq!(lines, ["4", "5", "6", "7", "8", "9", "10"]);
    }

    #[gpui_kit::test]
    fn a_repeated_update_or_install_line_is_written_once(cx: &mut gpui_kit::TestAppContext) {
        let mut strip = strip(cx);
        let ready = UpdateState::Ready {
            version: "2.0.0".into(),
            notes: String::new(),
            at_quit: false,
        };
        strip.update_state(&ready);
        strip.update_state(&ready);
        strip.installing(Some("demo"));
        strip.installing(Some("demo"));
        strip.installing(None);
        assert_eq!(strip.kept(), 2);
    }
}
