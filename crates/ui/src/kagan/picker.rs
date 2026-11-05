use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::list::ListState;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Pixels,
    Styled as _, Subscription, Task,
};
use kg_domain::{Program, split_program};

use super::{CloseCustomCommand, RetryPrograms, ToggleCustomCommand};

/// The key context of the picker.
pub(crate) const CONTEXT: &str = "ProgramPicker";

/// Asks the window to start the chosen program.
pub(crate) struct Start;
pub(crate) struct Close;

/// What the picker loads: the programs, or why they could not be found.
#[derive(Default)]
enum Programs {
    #[default]
    Loading,
    Loaded,
    Failed(String),
}

/// The most rows the list shows at once. Its height is that many measured rows.
const LIST_ROWS: u16 = 5;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Catalog,
    Custom,
}

/// The picker's list: every program, the ones the search leaves, the search, and the selected row.
#[derive(Default)]
pub(super) struct Rows {
    all: Vec<Program>,
    matches: Vec<Program>,
    selected: Option<usize>,
    query: String,
    loading: bool,
    mode: Mode,
    /// One row's height as it was last painted, so the list can cap itself at `LIST_ROWS` rows.
    row_height: Option<Pixels>,
}

impl Rows {
    /// Keeps the programs that match `query`, and selects the first of them.
    fn search(&mut self, query: &str) {
        // A change to the same text keeps the selection the person has made.
        if query == self.query {
            return;
        }
        query.clone_into(&mut self.query);
        self.filter();
    }

    /// The matches for `query`, with the first one selected when there is one.
    fn filter(&mut self) {
        let needle = self.query.to_lowercase();
        self.matches = self
            .all
            .iter()
            .filter(|program| {
                program.label.to_lowercase().contains(&needle)
                    || program.command.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        self.selected = (!self.matches.is_empty()).then_some(0);
    }

    fn selected(&self) -> Option<&Program> {
        self.selected.and_then(|row| self.matches.get(row))
    }
}

pub(crate) struct Picker {
    state: Programs,
    list: Entity<ListState<Rows>>,
    /// The search above the list. The picker filters the list itself, so arrow keys and Enter
    /// reach the list from here.
    search: Entity<InputState>,
    /// The custom command is open, and its fields are what starts. Choosing a row closes it.
    mode: Mode,
    program: Entity<InputState>,
    arguments: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    load_task: Option<Task<()>>,
}

impl EventEmitter<Start> for Picker {}
impl EventEmitter<Close> for Picker {}

mod list;

use list::{Field, custom_command, custom_field};
mod render;
mod state;
impl Picker {
    fn render_custom(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (program_note, arguments_note) = match custom_command(
            self.program.read(cx).value().trim(),
            self.arguments.read(cx).value().trim(),
        ) {
            Ok(command) => {
                let (name, args) = split_program(&command).unwrap_or_default();
                let preview = match args.len() {
                    0 => format!("Runs {name}"),
                    count => format!(
                        "Runs {name} with {count} {}",
                        if count == 1 { "argument" } else { "arguments" }
                    ),
                };
                (None, Some(("custom-preview", preview)))
            }
            Err((Field::Program, message)) => (Some(("program-feedback", message)), None),
            Err((Field::Arguments, message)) => (None, Some(("arguments-feedback", message))),
        };
        v_flex()
            .gap_2()
            .child(custom_field(
                "custom-program",
                "Program",
                &self.program,
                program_note,
                muted,
                cx,
            ))
            .child(custom_field(
                "custom-arguments",
                "Arguments",
                &self.arguments,
                arguments_note,
                muted,
                cx,
            ))
            .into_any_element()
    }
}
