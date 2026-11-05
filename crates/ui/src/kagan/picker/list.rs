use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::list::{ListDelegate, ListItem, ListState};
use gpui_kit::component::{IndexPath, Selectable, v_flex};
use gpui_kit::{
    App, Context, Entity, Focusable as _, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div,
};
use kg_domain::{Program, split_program};

use super::{Mode, Rows};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) enum Field {
    Program,
    Arguments,
}

/// `program` and `arguments` as one command line: the program quoted when it has a space.
pub(super) fn custom_command(program: &str, arguments: &str) -> Result<String, (Field, String)> {
    if program.is_empty() {
        return Err((Field::Program, "Enter a program to start.".into()));
    }
    let quoted = (program.starts_with('"') && program.ends_with('"'))
        || (program.starts_with('\'') && program.ends_with('\''));
    if program.contains(char::is_whitespace) && !quoted {
        return Err((
            Field::Program,
            "Put arguments in Arguments. Quote a program path containing spaces.".into(),
        ));
    }
    let program = if quoted {
        program.to_owned()
    } else if program.contains(['\'', '"']) {
        if !program.contains('\'') {
            format!("'{program}'")
        } else if !program.contains('"') {
            format!("\"{program}\"")
        } else {
            return Err((
                Field::Program,
                "Rename the program: it has both quote kinds.".into(),
            ));
        }
    } else {
        program.to_owned()
    };
    let command = if arguments.is_empty() {
        program
    } else {
        format!("{program} {arguments}")
    };
    split_program(&command).map_err(|_| {
        (
            Field::Arguments,
            "The arguments have an unfinished quote.".into(),
        )
    })?;
    Ok(command)
}

impl ListDelegate for Rows {
    type Item = ProgramRow;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.matches.len()
    }

    fn loading(&self, _: &App) -> bool {
        self.loading
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let program = self.matches.get(ix.row)?.clone();
        Some(ProgramRow {
            base: ListItem::new(format!("program-item-{}", program.command)),
            selected: self.mode == Mode::Catalog && self.selected == Some(ix.row),
            mode: self.mode,
            first: ix.row == 0,
            list: cx.entity().downgrade(),
            program,
        })
    }

    /// Escape makes the list clear its selection. A list with programs keeps one: the program
    /// that starts stays the one the person saw.
    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if ix.is_some() || self.matches.is_empty() {
            self.selected = ix.map(|ix| ix.row);
        }
        cx.notify();
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .p_2()
            .type_body(cx)
            .text_color(cx.theme().muted_foreground)
            .child(format!("No program matches “{}”.", self.query))
    }
}

#[derive(IntoElement)]
pub(crate) struct ProgramRow {
    pub(super) base: ListItem,
    pub(super) program: Program,
    pub(super) selected: bool,
    pub(super) mode: Mode,
    /// The first row sets the list's height. Rows can differ, so the others do not.
    pub(super) first: bool,
    pub(super) list: WeakEntity<ListState<Rows>>,
}

impl Selectable for ProgramRow {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected && self.mode == Mode::Catalog;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

/// A custom command field: its name and input, and what the input says under it. Clicking
/// anywhere in the field focuses the input, and kg ui finds the field by its id.
pub(super) fn custom_field(
    id: &'static str,
    title: &'static str,
    input: &Entity<InputState>,
    note: Option<(&'static str, String)>,
    muted: Hsla,
    cx: &App,
) -> impl IntoElement {
    let focus = input.focus_handle(cx);
    div()
        .id(id)
        .w_full()
        .ui_observe_as(Facts::input(title, input, cx))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&focus, cx);
        })
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(div().type_small(cx).text_color(muted).child(title))
                .child(Input::new(input).text_size(gpui_kit::px(crate::theme::type_body(cx))))
                .children(note.map(|(id, text)| {
                    div()
                        .id(id)
                        .ui_container()
                        .type_small(cx)
                        .text_color(muted)
                        .aria_label(text.clone())
                        .child(text)
                })),
        )
}

/// What a row says after the name: the first line of the version, or that the program is missing.
pub(super) fn status_line(program: &Program) -> String {
    if program.installed() {
        // Only the first line of a version: a row must stay one line tall.
        program
            .version
            .as_deref()
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    } else {
        "Not installed".to_owned()
    }
}
