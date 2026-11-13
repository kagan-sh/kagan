use crate::theme::{SpaceStep, TypeScale as _, space};
use gpui_kit::Focusable as _;
use gpui_kit::base::actions::{Confirm, SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::FocusableExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, RenderOnce, Role, StatefulInteractiveElement as _,
    Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::{Program, ProgramSource};

use super::list::{Version, version_of};
use super::mark::program_mark;
use super::saved;
use super::{COLUMNS, CONTEXT, CUSTOM_CONTEXT, GRID_CONTEXT, Mode, Picker, Programs};
use super::{CloseCustomCommand, RetryPrograms};
use crate::kagan::controls::program_key;
use crate::kagan::controls::{control, dispatch, note};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

// A card fills its column and is this tall: the mark on top, the name, then its caption.
const CARD_HEIGHT: f32 = 96.;
// The grid shows this many rows of cards, then scrolls: three rows hold nine cards, and a tenth
// scrolls into view. Three rows and the Start row fit a 720-point window.
const VISIBLE_ROWS: f32 = 3.;

#[derive(IntoElement)]
struct ProgramCard {
    program: Program,
    selected: bool,
    index: usize,
    picker: WeakEntity<Picker>,
}

impl RenderOnce for ProgramCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let Version {
            shown: version,
            full: full_version,
        } = version_of(&self.program);
        let tooltip = match self.program.source {
            ProgramSource::Saved => Some(self.program.command.clone()),
            _ => full_version,
        };
        let key = program_key(&self.program);
        let label = self.program.label.clone();
        // One caption under the name: an agent's version, or a program's command.
        let caption = match self.program.source {
            ProgramSource::Known => Some((format!("program-version-{key}"), version)),
            ProgramSource::Shell | ProgramSource::Saved => Some((
                format!("program-command-{key}"),
                self.program.command.clone(),
            )),
        };
        // The selected card says so in words too, not only by its fill.
        let name = if self.selected {
            format!("{label}, selected")
        } else {
            label.clone()
        };
        let card_group = format!("program-card-{key}");
        let actions = saved::card_actions(&self.program, &self.picker, &card_group, self.selected);
        let picker = self.picker.clone();
        let index = self.index;
        div()
            .id(format!("program-{key}"))
            .when_some(tooltip, |card, tip| {
                card.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            })
            .ui_observe_as(Facts {
                role: Role::ListBoxOption,
                selected: Some(self.selected),
                ..Facts::button(name.clone())
            })
            .group(card_group)
            .w_full()
            .h(px(CARD_HEIGHT))
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .when(self.selected, |card| card.bg(cx.theme().secondary))
            .on_click(move |_, window, cx| {
                _ = picker.update(cx, |this, cx| this.pick(index, window, cx));
            })
            .child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(program_mark(&self.program, cx))
                    .child(
                        div()
                            .id(format!("program-name-{key}"))
                            .ui_container()
                            .max_w_full()
                            .type_body(cx)
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .aria_label(name.clone())
                            .child(label),
                    )
                    .children(caption.map(|(id, text)| {
                        div()
                            .id(id)
                            .ui_container()
                            .type_caption(cx)
                            .text_color(muted)
                            .max_w_full()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .aria_label(text.clone())
                            .child(text)
                    }))
                    .children(actions),
            )
    }
}

// The Custom command button under the grid. It opens the program line fields below it.
fn custom_toggle(open: bool) -> AnyElement {
    let label = if open {
        "Hide custom command"
    } else {
        "Custom command"
    };
    // A row, so the button keeps its own width under the grid's full-width column.
    h_flex()
        .key_context(CUSTOM_CONTEXT)
        .child(control(
            "custom-command-toggle",
            Facts::button(label),
            Button::new("custom-command-toggle-button")
                .small()
                .outline()
                .icon(gpui_kit::assets::IconName::SquareTerminal)
                .label(label)
                .on_click(dispatch(crate::kagan::ToggleCustomCommand)),
        ))
        .into_any_element()
}

// The first row's cards share a top edge, so the cards in that row are the columns.
fn columns_of(bounds: &[Bounds<Pixels>]) -> usize {
    let Some(first) = bounds.first() else {
        return usize::from(COLUMNS);
    };
    bounds
        .iter()
        .take_while(|bounds| bounds.origin.y == first.origin.y)
        .count()
        .max(1)
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
    fn grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let gap = space(SpaceStep::Sm, cx);
        let height = px(VISIBLE_ROWS * CARD_HEIGHT + (VISIBLE_ROWS - 1.) * gap);
        let picker: WeakEntity<Picker> = cx.weak_entity();
        let cards = &self.cards;
        let measured = picker.clone();
        let mut grid = div()
            .on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, _, cx| {
                let columns = columns_of(&bounds);
                _ = measured.update(cx, |this, _| this.cards.columns = columns);
            })
            .id("program-list")
            .key_context(GRID_CONTEXT)
            .ui_observe_as(Facts {
                role: Role::ListBox,
                focus: Some(self.grid.clone()),
                ..Facts::button("Programs")
            })
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .w_full()
            .grid()
            .grid_cols(COLUMNS)
            .gap(px(gap))
            .max_h(height);
        for (index, program) in cards.matches.iter().enumerate() {
            grid = grid.child(ProgramCard {
                program: program.clone(),
                selected: cards.cursor == index && self.mode == Mode::Catalog,
                index,
                picker: picker.clone(),
            });
        }
        grid.into_any_element()
    }
}

impl Render for Picker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let body: AnyElement = match &self.state {
            Programs::Failed(message) => programs_failed(message, muted, cx),
            Programs::Loading => div()
                .id("program-loading")
                .ui_container()
                .flex()
                .items_center()
                .h(px(crate::theme::row_height(cx)))
                .px_2()
                .type_body(cx)
                .text_color(muted)
                .aria_label("Looking for programs…")
                .child("Looking for programs…")
                .into_any_element(),
            Programs::Loaded => self.grid(cx),
        };
        // Off while the custom command is open: its field shows the same reason beside it.
        // Off with no match too: the grid's own card says what to do.
        let custom = self.mode == Mode::Custom;
        let matched = !self.cards.matches.is_empty();
        let reason = self.command(cx).err().filter(|_| !custom && matched);
        v_flex()
            .key_context(CONTEXT)
            .gap_2()
            .on_action(cx.listener(|this, _: &CloseCustomCommand, window, cx| {
                if this.mode == Mode::Custom {
                    this.close_custom(window, cx);
                } else {
                    cx.emit(super::Close);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| {
                this.move_by(-(this.cards.columns.cast_signed()), cx);
            }))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| {
                this.move_by(this.cards.columns.cast_signed(), cx);
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                // Only the grid has this context, so Enter here starts the selected card.
                if this.mode == Mode::Custom {
                    return;
                }
                this.confirm(cx);
            }))
            .child(control(
                "program-search",
                Facts::input("Search programs", &self.search, cx),
                Input::new(&self.search)
                    // One ring: the field's own border in the focus token, inside its bounds, so no
                    // parent clips it. The outer ring doubled it.
                    .focus_ring(false)
                    .border_color(super::list::focus_border(
                        self.search.focus_handle(cx).is_focused(window),
                        cx,
                    ))
                    .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                    .small(),
            ))
            .when(!matched && matches!(self.state, Programs::Loaded), |this| {
                this.child(
                    div()
                        .id("program-empty")
                        .ui_container()
                        .type_body(cx)
                        .text_color(muted)
                        .aria_label(format!(
                            "No program matches “{}”. Clear the search to see them all.",
                            self.cards.query
                        ))
                        .child(format!(
                            "No program matches “{}”. Clear the search to see them all.",
                            self.cards.query
                        )),
                )
            })
            .child(body)
            .child(custom_toggle(custom))
            .when_some(reason, |this, reason| {
                this.child(note("program-reason", reason, muted, cx))
            })
            .when_some(self.notice.clone(), |this, text| {
                this.child(saved::notice(text, cx))
            })
            .when(custom, |this| this.child(self.render_custom(window, cx)))
    }
}
