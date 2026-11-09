use crate::theme::TypeScale as _;
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::list::{List, ListState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, RenderOnce, Role, StatefulInteractiveElement as _, Styled as _, WeakEntity,
    Window, div,
};
use kg_domain::ProgramSource;

use super::list::{ProgramRow, Version, version_of};
use super::{CONTEXT, Close, LIST_ROWS, Mode, Picker, Programs, Rows};
use super::{CloseCustomCommand, RetryPrograms, ToggleCustomCommand};
use crate::kagan::controls::{control, dispatch, note};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl RenderOnce for ProgramRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        // A known agent's command is implied by its name, so only a shell or a recent program shows it.
        let command = (!matches!(self.program.source, ProgramSource::Known))
            .then(|| self.program.command.clone());
        let Version {
            shown: version,
            full: full_version,
        } = version_of(&self.program);
        // The selected row says so in words too, not only by its fill.
        let name = if self.selected {
            format!("{}, selected", self.program.label)
        } else {
            self.program.label.clone()
        };
        let (list, first) = (self.list, self.first);
        div()
            .on_children_prepainted(move |bounds, window, cx| {
                if let Some(row) = bounds.first().filter(|_| first) {
                    record_row_height(&list, row.size.height, window, cx);
                }
            })
            .child(
                div()
                    .id(format!("program-{}", self.program.command))
                    .when_some(full_version, |row, full| {
                        row.tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    })
                    .ui_observe_as(Facts {
                        role: Role::ListBoxOption,
                        selected: Some(self.selected),
                        ..Facts::button(name.clone())
                    })
                    .child(
                        self.base
                            .when(self.selected, |row| row.bg(cx.theme().secondary))
                            .px_2()
                            .h_7()
                            .rounded(cx.theme().radius)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(format!("program-name-{}", self.program.command))
                                            .ui_container()
                                            .min_w_0()
                                            .type_body(cx)
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .aria_label(name)
                                            .child(self.program.label),
                                    )
                                    .child(div().flex_1().min_w_0().children(command.map(
                                        |command| {
                                            div()
                                                .id(format!("program-command-{command}"))
                                                .ui_container()
                                                .type_small(cx)
                                                .text_color(muted)
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .aria_label(command.clone())
                                                .child(command)
                                        },
                                    )))
                                    .child(
                                        div()
                                            .id(format!("program-version-{}", self.program.command))
                                            .flex_shrink_0()
                                            .type_caption(cx)
                                            .text_color(muted)
                                            .whitespace_nowrap()
                                            .ui_container()
                                            .aria_label(version.clone())
                                            .child(version),
                                    ),
                            ),
                    ),
            )
    }
}

// The first row's height is every row's height, so the list's cap is read in rows.
fn record_row_height(
    list: &WeakEntity<ListState<Rows>>,
    height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    // A collapsed pass reports no height. Keeping it would lock the list shut.
    if height.as_f32() <= 1. {
        return;
    }
    let changed = list
        .update(cx, |state, cx| {
            let rows = state.delegate_mut();
            if rows.row_height == Some(height) {
                return false;
            }
            rows.row_height = Some(height);
            if state
                .selected_index()
                .is_some_and(|index| index.row >= usize::from(LIST_ROWS))
            {
                state.scroll_to_selected_item(window, cx);
            }
            cx.notify();
            true
        })
        .unwrap_or(false);
    if changed {
        // The cap is read on the next frame. Painting cannot start one, so the window
        // is refreshed once this paint is done.
        cx.refresh_windows();
    }
}

// A row the width of the list. Its label keeps the program names' inset, and the chevron turns
// while the fields are open.
fn custom_toggle(toggle: &'static str, custom: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    control(
        "custom-command-toggle",
        Facts::button(toggle),
        Button::new("custom-command-toggle-button")
            .small()
            .ghost()
            .w_full()
            .h_7()
            .accessibility_label(toggle)
            // The button centres its content, so the row fills the width.
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .id("custom-command-label")
                            .ui_container()
                            .type_body(cx)
                            .aria_label(toggle)
                            .child(toggle),
                    )
                    .child(
                        div().pr_2().child(
                            Icon::new(if custom {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .small()
                            .text_color(muted),
                        ),
                    ),
            )
            .on_click(dispatch(ToggleCustomCommand)),
    )
    .into_any_element()
}

fn programs_failed(message: &str, muted: Hsla, cx: &App) -> AnyElement {
    // The message sits above the button: a long path in it must not push the Retry out of view.
    v_flex()
        .gap_2()
        .items_start()
        .child(
            div()
                .w_full()
                .type_body(cx)
                .text_color(muted)
                .child(format!("Couldn’t look for programs: {message}")),
        )
        .child(control(
            "programs-retry",
            Facts::button("Retry"),
            Button::new("programs-retry-button")
                .small()
                .outline()
                .label("Retry")
                .on_click(dispatch(RetryPrograms)),
        ))
        .into_any_element()
}

impl Picker {
    fn program_list_height(&self, cx: &App) -> gpui_kit::Pixels {
        let rows = self.list.read(cx).delegate();
        let count = if rows.loading {
            usize::from(LIST_ROWS)
        } else {
            rows.matches.len().clamp(1, usize::from(LIST_ROWS))
        };
        let row = rows
            .row_height
            .filter(|height| height.as_f32() > 1.)
            .unwrap_or_else(|| gpui_kit::px(crate::theme::row_height(cx)));
        row * f32::from(u16::try_from(count).unwrap_or(LIST_ROWS))
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let body: AnyElement = match &self.state {
            Programs::Failed(message) => programs_failed(message, muted, cx),
            Programs::Loading => div()
                .id("program-loading")
                .ui_container()
                .flex()
                .items_center()
                .h(gpui_kit::px(crate::theme::row_height(cx)))
                .px_2()
                .type_body(cx)
                .text_color(muted)
                .aria_label("Looking for programs…")
                .child("Looking for programs…")
                .into_any_element(),
            Programs::Loaded => {
                let height = self.program_list_height(cx);
                List::new(&self.list)
                    .h(height)
                    .max_h(height)
                    .into_any_element()
            }
        };
        let hidden = self
            .list
            .read(cx)
            .delegate()
            .matches
            .len()
            .saturating_sub(usize::from(LIST_ROWS));
        // Off while the custom command is open: its field shows the same reason beside it.
        // Off with no match too: the list's own row says what to do.
        let custom = self.mode == Mode::Custom;
        let matched = !self.list.read(cx).delegate().matches.is_empty();
        let reason = self.command(cx).err().filter(|_| !custom && matched);
        let toggle = match self.mode {
            Mode::Custom => "Hide custom command",
            Mode::Catalog => "Custom command…",
        };
        v_flex()
            .key_context(CONTEXT)
            .gap_2()
            .on_action(cx.listener(|this, _: &CloseCustomCommand, window, cx| {
                if this.mode == Mode::Custom {
                    this.close_custom(window, cx);
                } else {
                    cx.emit(Close);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectUp, window, cx| {
                this.move_selection(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectDown, window, cx| {
                this.move_selection(1, window, cx);
            }))
            .child(control(
                "program-search",
                Facts::input("Search programs", &self.search, cx),
                Input::new(&self.search)
                    .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                    .small(),
            ))
            .child(div().id("program-list").ui_container().child(body))
            .when(hidden > 0, |this| {
                this.child(note(
                    "program-more",
                    format!("{hidden} more. Type to search, or scroll."),
                    muted,
                    cx,
                ))
            })
            .when_some(reason, |this, reason| {
                this.child(note("program-reason", reason, muted, cx))
            })
            .child(custom_toggle(toggle, custom, cx))
            .when(custom, |this| this.child(self.render_custom(cx)))
    }
}
