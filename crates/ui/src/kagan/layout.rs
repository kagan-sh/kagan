#[cfg(test)]
pub(crate) mod read;
use crate::theme::TypeScale as _;
use std::future::Future;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, IconName, Sizable as _, TitleBar, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, SharedString, StatefulInteractiveElement as _, Styled,
    Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, PreferenceOp};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    Kagan, ToggleRightSidebar, ToggleSidebar, right,
    screen::{Activation, Side},
};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const WINDOW: &str = "main";

const LEFT_DEFAULT: f32 = 256.;
pub(super) const LEFT_RANGE: (f32, f32) = (200., 420.);

/// The narrowest the center may get. When the open sidebars would squeeze it below this, the
/// right sidebar is hidden first, then the left one. The window cannot be made narrower.
pub(super) const CENTER_MIN: f32 = 480.;
pub(super) const RAIL_WIDTH: f32 = 48.;

pub(super) const RIGHT_OVERLAY: &str = "RightOverlay";
pub(super) const LEFT_OVERLAY: &str = "LeftOverlay";

const LEFT_SCRIM: f32 = 0.5;

fn left_scrim(cx: &App) -> Hsla {
    cx.theme().background.opacity(LEFT_SCRIM)
}

pub(super) fn clamp_width(width: Pixels, range: (f32, f32)) -> Pixels {
    px(f32::from(width).clamp(range.0, range.1))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) left_open: bool,
    pub(super) right_open: bool,
    pub(super) left_width: Pixels,
    pub(super) right_width: Pixels,
    pub(super) review_wrap: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left_open: true,
            right_open: true,
            left_width: px(LEFT_DEFAULT),
            right_width: px(right::WIDTH),
            review_wrap: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Panes {
    pub(super) left: bool,
    pub(super) right: bool,
}

/// The index of the right sidebar in the columns group. The left sidebar, when open, comes first.
pub(super) fn right_index(left_open: bool) -> usize {
    if left_open { 2 } else { 1 }
}

fn key(name: &str) -> String {
    format!("{WINDOW}.{name}")
}

pub(crate) fn read<T: DeserializeOwned>(
    key: &str,
    cx: &mut App,
) -> impl Future<Output = Result<Option<T>, OpError>> + use<T> {
    let name = key.to_owned();
    let task = Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: name.clone() }), cx);
    async move {
        match task.await? {
            OpResult::Setting(serde_json::Value::Null) => Ok(None),
            OpResult::Setting(value) => serde_json::from_value(value).map(Some).map_err(|error| {
                OpError::Failed(format!(
                    "preference {name} is not the value it was saved as: {error}"
                ))
            }),
            _ => Err(OpError::Failed(format!(
                "preference {name} did not read as a setting"
            ))),
        }
    }
}

fn saved_or<T>(read: Result<Option<T>, OpError>, fallback: T) -> T {
    read.ok().flatten().unwrap_or(fallback)
}

pub(super) fn save(key: &str, value: impl Serialize, cx: &mut App) {
    if let Ok(value) = serde_json::to_value(value) {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: key.into(),
                value,
            }),
            cx,
        )
        .detach();
    }
}

impl Kagan {
    pub(super) fn load_layout(cx: &mut Context<Self>) {
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
    pub(super) fn sync_panels_keeping_widths(
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

    pub(super) fn room_panes(&self, left_open: bool, right_open: bool) -> Panes {
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

    pub(super) fn desired_panes(&self, cx: &App) -> Panes {
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

    pub(super) fn force_sidebar(
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
    pub(super) fn save_open_change(&self, before: Layout, cx: &mut App) {
        if self.layout.left_open != before.left_open {
            save(&key("left-sidebar"), self.layout.left_open, cx);
        }
        if self.layout.right_open != before.right_open {
            save(&key("right-sidebar"), self.layout.right_open, cx);
        }
    }

    pub(super) fn fit_to_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn hold_resizes(&mut self, cx: &mut Context<Self>) {
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

    fn chosen_width(&self, width: Pixels) -> bool {
        width + px(0.5) >= self.layout.left_width || self.side_fits(self.layout.left_width)
    }

    /// Whether the window has hidden a sidebar the layout shows. Widths are not saved while it
    /// has: the sidebar beside a hidden one takes its room, and that is not a width the person
    /// chose.
    pub(super) fn hidden_by_width(&self) -> bool {
        self.room_panes(self.layout.left_open, self.layout.right_open)
            != Panes {
                left: self.layout.left_open,
                right: self.layout.right_open,
            }
    }

    pub(super) fn sync_panels(&mut self, cx: &mut Context<Self>) {
        let want = self.desired_panes(cx);
        self.sync_to(want, cx);
    }

    pub(super) fn sync_to(&mut self, want: Panes, cx: &mut Context<Self>) {
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
        let layout = self.layout;
        self.columns.update(cx, |state, cx| {
            if held.right && !want.right && state.sizes().len() > right_index(held.left) {
                state.remove_panel(right_index(held.left), cx);
            }
            if held.left && !want.left && !state.sizes().is_empty() {
                state.remove_panel(0, cx);
            }
            if want.left && !held.left {
                state.insert_panel(Some(layout.left_width), Some(0), cx);
            }
            if want.right && !held.right {
                state.insert_panel(Some(layout.right_width), None, cx);
            }
        });
        cx.notify();
    }

    pub(super) fn restore_widths(
        &mut self,
        kept: Panes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Restoring makes resize events that are not the person's choice, so they are ignored.
        self.hold_resizes(cx);
        let (index, layout) = (right_index(self.panes.left), self.layout);
        self.columns.update(cx, |state, cx| {
            if kept.left {
                state.resize_panel(0, layout.left_width, window, cx);
            }
            if kept.right {
                state.resize_panel(index, layout.right_width, window, cx);
            }
        });
    }

    /// A drag ended: keep the widths the sidebars have now. Resizes in Settings are not kept.
    pub(super) fn resized(&mut self, cx: &mut Context<Self>) {
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
        if self.panes.right
            && let Some(width) = sizes.last()
        {
            next.right_width = *width;
        }
        if next.left_width != self.layout.left_width {
            save(&key("left-width"), next.left_width, cx);
        }
        if next.right_width != self.layout.right_width {
            save(&key("right-width"), next.right_width, cx);
        }
        self.layout = next;
        cx.notify();
    }

    /// The sidebar buttons and shortcuts do nothing in Settings, where the sidebars are fixed.
    /// Showing a sidebar that has no room draws it over the center, as the right panels do.
    pub(super) fn toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_settings() {
            return;
        }
        if self.panes.left {
            save(&key("left-sidebar"), false, cx);
        } else if self.room_panes(true, false).left {
            save(&key("left-sidebar"), true, cx);
        } else if self.screen.drawn(Side::Left) {
            self.dismiss_overlay(Side::Left, window, cx);
        } else {
            self.open_overlay(Side::Left, Activation::Focus, window, cx);
        }
    }

    /// Shows the left sidebar beside the center when there is room, and over it when there is not.
    pub(super) fn show_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.room_panes(true, self.layout.right_open).left {
            self.force_sidebar(true, true, window, cx);
        } else {
            self.open_overlay(Side::Left, Activation::Focus, window, cx);
        }
    }

    pub(super) fn left_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.screen.drawn(Side::Left) || self.panes.left || self.in_settings() {
            return None;
        }
        let dismiss =
            cx.listener(|this, _, window, cx| this.dismiss_overlay(Side::Left, window, cx));
        let focus = self.left_overlay_focus.clone();
        let layer = div()
            .id("left-overlay")
            .ui_container()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("left-overlay-scrim")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(RAIL_WIDTH))
                    .right_0()
                    .bg(left_scrim(cx))
                    .occlude()
                    .on_click(dismiss)
                    .ui_observe_as(Facts::button("Dismiss sidebar")),
            )
            .child(
                div()
                    .id("left-overlay-panel")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(RAIL_WIDTH))
                    .w(self.layout.left_width)
                    .bg(cx.theme().sidebar)
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .occlude()
                    .key_context(LEFT_OVERLAY)
                    .track_focus(&focus)
                    .ui_observe_as(Facts {
                        hint: None,
                        role: Role::Unknown,
                        name: "Sidebar".into(),
                        value: None,
                        disabled: false,
                        selected: None,
                        focus: Some(focus.clone()),
                    })
                    .child(self.left_side(window, cx)),
            )
            .into_any_element();
        match self.screen.painted(Side::Left) {
            Some(Activation::Focus) => window.focus(&focus, cx),
            Some(Activation::SidebarRow) => {
                if let Some(id) = self.sidebar_target(cx) {
                    self.sidebar
                        .update(cx, |sidebar, cx| sidebar.reveal_row(&id, window, cx));
                } else {
                    window.focus(&focus, cx);
                }
            }
            Some(Activation::Quiet) | None => {}
        }
        Some(layer)
    }

    pub(super) fn toggle_right(
        &mut self,
        _: &ToggleRightSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_settings() || (!self.screen.drawn(Side::Right) && !self.has_right_content(cx)) {
            return;
        }
        if self.panes.right {
            if self.right_holds_focus(window, cx) {
                self.focus_work(window, cx);
            }
            self.right_fill = right::Fill::Column;
            self.dismiss_overlay(Side::Right, window, cx);
            save(&key("right-sidebar"), false, cx);
            return;
        }
        if !self.screen.drawn(Side::Right) && self.waiting_count(cx) > 0 {
            self.select_right_tab("needs-you", cx);
        }
        // Save the wide layout when the column fits, even while the overlay is up.
        if self.screen.review().is_none()
            && self.has_right_content(cx)
            && self.room_panes(self.layout.left_open, true).right
        {
            save(&key("right-sidebar"), true, cx);
            return;
        }
        if self.screen.drawn(Side::Right) {
            self.dismiss_overlay(Side::Right, window, cx);
            return;
        }
        self.open_overlay(Side::Right, Activation::Focus, window, cx);
    }

    /// The right sidebar is on screen, as a column or as an overlay.
    pub(super) fn right_drawn(&self) -> bool {
        self.panes.right || (self.screen.drawn(Side::Right) && !self.in_settings())
    }

    pub(super) fn right_mounted(&self) -> bool {
        self.layout.right_open || self.screen.drawn(Side::Right)
    }

    pub(super) fn right_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.screen.drawn(Side::Right) || self.panes.right || self.in_settings() {
            return None;
        }
        let dismiss =
            cx.listener(|this, _, window, cx| this.dismiss_overlay(Side::Right, window, cx));
        let layer = div()
            .id("right-overlay")
            .ui_container()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("right-overlay-scrim")
                    .ui_container()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_click(dismiss),
            )
            .child(
                div()
                    .id("right-overlay-panel")
                    .ui_container()
                    .key_context(RIGHT_OVERLAY)
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(right::WIDTH))
                    .bg(cx.theme().sidebar)
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .occlude()
                    .child(right::render(self, window, cx)),
            )
            .into_any_element();
        if matches!(self.screen.painted(Side::Right), Some(Activation::Focus)) {
            let key = self.overlay_focus(cx);
            let handle = super::tab_stop(window, cx, ElementId::Name(key.into()));
            window.focus(&handle, cx);
        }
        Some(layer)
    }

    pub(super) fn title_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let drawn = self.right_drawn();
        let left_shown = self.panes.left || self.screen.drawn(Side::Left);
        let thread = self
            .thread_visible(cx)
            .then(|| self.shown_thread(cx))
            .flatten()
            .and_then(|id| self.record(&id, cx).cloned());
        let project = thread
            .as_ref()
            .and_then(|thread| self.sidebar.read(cx).project(&thread.project_id))
            .map(|project| project.name.clone());
        let title = thread.as_ref().map_or_else(
            || super::TITLE.to_owned(),
            |thread| {
                project.as_ref().map_or_else(
                    || thread.title.clone(),
                    |project| format!("{} — {project}", thread.title),
                )
            },
        );
        if self.window_title != title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        let label = thread.as_ref().map(|thread| {
            h_flex()
                .id("thread-heading")
                .ui_container()
                .flex_1()
                .min_w_0()
                .justify_center()
                .gap_1p5()
                .child(self.title_line(thread, cx))
                .when_some(project, |heading, project| {
                    heading
                        .child(
                            div()
                                .flex_none()
                                .type_body()
                                .text_color(cx.theme().muted_foreground)
                                .child("·"),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_initial()
                                .text_ellipsis()
                                .type_body()
                                .text_color(cx.theme().muted_foreground)
                                .child(project),
                        )
                })
        });
        let controls = thread
            .as_ref()
            .map(|thread| self.thread_controls(thread, cx));
        // Controls occlude the drag area, or Windows takes a press on them as a caption drag.
        TitleBar::new().pl_0().child(
            h_flex()
                .size_full()
                .items_center()
                .pr_2()
                .when(cfg!(target_os = "macos"), Styled::pl_20)
                .when(!cfg!(target_os = "macos"), Styled::pl_3)
                .child(toggle(
                    "left-sidebar-toggle",
                    if left_shown {
                        "Hide sidebar"
                    } else {
                        "Show sidebar"
                    },
                    left_shown,
                    self.in_settings(),
                    (IconName::PanelLeft, IconName::PanelLeftOpen),
                    ToggleSidebar,
                    cx,
                ))
                .child(h_flex().flex_1().min_w_0().children(label))
                .child(
                    h_flex()
                        .gap_2()
                        .occlude()
                        .flex_none()
                        .children(controls)
                        .child(self.right_button(drawn, self.waiting_count(cx), cx)),
                ),
        )
    }

    /// Changed files of the shown thread, as of the last read. Zero while the loaded changes
    /// belong to another thread.
    fn changed_files(&self, cx: &App) -> usize {
        if !self.thread_visible(cx) || self.changes.thread != self.shown_thread(cx) {
            return 0;
        }
        self.changes.files.len()
    }

    fn right_button(&self, drawn: bool, waiting: usize, cx: &App) -> impl IntoElement {
        let changed = if drawn { 0 } else { self.changed_files(cx) };
        let icon = if drawn {
            IconName::PanelRight
        } else {
            IconName::PanelRightOpen
        };
        let value: SharedString = if drawn {
            "shown".into()
        } else if waiting > 0 {
            format!("hidden, {waiting} waiting").into()
        } else if changed > 0 {
            format!("hidden, {changed} changed").into()
        } else {
            "hidden".into()
        };
        let name: SharedString = if drawn {
            "Hide panels".into()
        } else {
            "Show panels".into()
        };
        let nothing = !drawn && !self.has_right_content(cx);
        let button = bar_button(
            "right-sidebar-toggle",
            name.clone(),
            icon,
            self.in_settings() || nothing,
            ToggleRightSidebar,
            cx,
        )
        .tab_stop(false);
        let button = if nothing {
            button.tooltip("Open a thread to see its changes, or wait for a question.")
        } else if changed > 0 {
            button.tooltip("This thread has changed files. Show panels to see them.")
        } else {
            button
        };
        let facts = Facts {
            hint: None,
            role: Role::Button,
            name,
            value: Some(value),
            disabled: self.in_settings() || nothing,
            selected: None,
            focus: Some(self.right_toggle_focus.clone()),
        };
        let row = h_flex()
            .id("right-sidebar-toggle")
            .gap_1()
            .items_center()
            .ui_observe_as(facts);
        let row = if drawn || waiting == 0 {
            row
        } else {
            row.child(
                div()
                    .id("right-sidebar-waiting")
                    .ui_container()
                    .type_small()
                    .text_color(crate::theme::Tone::Needs.color(cx))
                    .child(waiting.to_string()),
            )
        };
        let row = if changed == 0 {
            row
        } else {
            row.child(
                div()
                    .id("right-sidebar-changes")
                    .ui_container()
                    .type_small()
                    .text_color(cx.theme().muted_foreground)
                    .child(changed.to_string()),
            )
        };
        row.child(button)
    }
}

fn toggle(
    id: &'static str,
    name: &'static str,
    open: bool,
    disabled: bool,
    icons: (IconName, IconName),
    action: impl Action + Clone,
    cx: &App,
) -> impl IntoElement {
    let icon = if open { icons.0 } else { icons.1 };
    let button = bar_button(id, name.into(), icon, disabled, action, cx);
    let facts = Facts {
        role: Role::Button,
        name: name.into(),
        value: Some(if open { "shown" } else { "hidden" }.into()),
        disabled,
        selected: None,
        hint: None,
        focus: None,
    };
    div().id(id).occlude().ui_observe_as(facts).child(button)
}

fn bar_button(
    id: &str,
    name: SharedString,
    icon: IconName,
    disabled: bool,
    action: impl Action + Clone,
    cx: &App,
) -> Button {
    let tip = super::controls::tip(&name, &action, cx);
    Button::new(ElementId::Name(SharedString::from(format!("{id}-button"))))
        .small()
        .ghost()
        .icon(icon)
        .accessibility_label(name)
        .tooltip(tip)
        .disabled(disabled)
        .on_click(move |_, window, cx| window.dispatch_action(Box::new(action.clone()), cx))
}
