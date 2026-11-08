use crate::theme::TypeScale as _;
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::list::List;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, RenderOnce, Role, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use kg_domain::ProgramSource;

use super::list::{ProgramRow, status_line};
use super::{CONTEXT, Close, LIST_ROWS, Mode, Picker, Programs};
use super::{CloseCustomCommand, RetryPrograms, ToggleCustomCommand};
use crate::kagan::controls::{control, dispatch, note};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl RenderOnce for ProgramRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let icon = match self.program.source {
            ProgramSource::Known => None,
            ProgramSource::Shell => Some(IconName::SquareTerminal),
            ProgramSource::Recent => Some(IconName::Undo),
        };
        let status = status_line(&self.program);
        // The selected row says so in words too, not only by its fill.
        let name = if self.selected {
            format!("{}, selected", self.program.label)
        } else {
            self.program.label.clone()
        };
        let (list, first) = (self.list, self.first);
        div()
            .on_children_prepainted(move |bounds, window, cx| {
                let Some(row) = bounds.first().filter(|_| first) else {
                    return;
                };
                let height = row.size.height;
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
            })
            .child(
                div()
                    .id(format!("program-{}", self.program.command))
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
                                    .child(div().w_4().children(
                                        icon.map(|icon| Icon::new(icon).small().text_color(muted)),
                                    ))
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
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .type_small(cx)
                                            .text_color(muted)
                                            .text_ellipsis()
                                            .child(self.program.command),
                                    )
                                    .child(
                                        div()
                                            .max_w(gpui_kit::px(100.))
                                            .type_caption(cx)
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .text_color(muted)
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(status),
                                    ),
                            ),
                    ),
            )
    }
}

/// The programs could not be listed: why, and a Retry.
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
    /// Five rows, or fewer when the search leaves fewer. A theme row until one is measured.
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
        let body: AnyElement = if let Programs::Failed(message) = &self.state {
            programs_failed(message, muted, cx)
        } else {
            let height = self.program_list_height(cx);
            List::new(&self.list)
                .h(height)
                .max_h(height)
                .into_any_element()
        };
        // Off while the custom command is open: its field shows the same reason beside it.
        let custom = self.mode == Mode::Custom;
        let reason = self.command(cx).err().filter(|_| !custom);
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
            .when_some(reason, |this, reason| {
                this.child(note("program-reason", reason, muted, cx))
            })
            .child(control(
                "custom-command-toggle",
                Facts::button(toggle),
                Button::new("custom-command-toggle-button")
                    .small()
                    .ghost()
                    .w_full()
                    .accessibility_label(toggle)
                    // The button centres its content, so the label fills the width to sit left.
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div()
                                    .w_4()
                                    .child(Icon::new(IconName::Plus).small().text_color(muted)),
                            )
                            .child(toggle),
                    )
                    .on_click(dispatch(ToggleCustomCommand)),
            ))
            .when(custom, |this| this.child(self.render_custom(cx)))
    }
}
