use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, FocusHandle, IntoElement, ParentElement as _,
    ScrollHandle, SharedString, Styled as _, Subscription, Task,
};
use kg_domain::{Program, split_program};

use super::{CloseCustomCommand, RetryPrograms};

pub(crate) const CONTEXT: &str = "ProgramPicker";

pub(crate) fn bindings() -> [gpui_kit::KeyBinding; 5] {
    use gpui_kit::KeyBinding;
    use gpui_kit::base::actions::{Confirm, SelectDown, SelectLeft, SelectRight, SelectUp};
    [
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
    ]
}

pub(crate) struct Start;
pub(crate) struct Close;

#[derive(Default)]
enum Programs {
    #[default]
    Loading,
    Loaded,
    Failed(String),
}

// The cards in a row, when the grid is at its widest. A narrower grid wraps to fewer.
const COLUMNS: u16 = 3;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Catalog,
    Custom,
}

// The program cards: the programs that match the search, then the custom command card.
pub(super) struct Cards {
    all: Vec<Program>,
    matches: Vec<Program>,
    // An index into `matches`, or `matches.len()` for the custom command card.
    cursor: usize,
    query: String,
    loading: bool,
    // The cards in a row, as the last paint laid them out. Up and Down move by this many.
    columns: usize,
}

impl Cards {
    fn new() -> Self {
        Self {
            all: Vec::new(),
            matches: Vec::new(),
            cursor: 0,
            query: String::new(),
            loading: false,
            columns: usize::from(COLUMNS),
        }
    }

    fn search(&mut self, query: &str) {
        // A change to the same text keeps the selection the person has made.
        if query == self.query {
            return;
        }
        query.clone_into(&mut self.query);
        self.filter();
    }

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
        self.cursor = 0;
    }

    fn selected(&self) -> Option<&Program> {
        self.matches.get(self.cursor)
    }

    fn custom_selected(&self) -> bool {
        self.cursor == self.matches.len()
    }
}

pub(crate) struct Picker {
    state: Programs,
    cards: Cards,
    // Keys go to the grid when a card has the focus; the search field keeps its own arrows.
    grid: FocusHandle,
    scroll: ScrollHandle,
    search: Entity<InputState>,
    mode: Mode,
    program: Entity<InputState>,
    arguments: Entity<InputState>,
    // The name a custom command is saved under. While `editing`, it is the only field: a rename.
    name: Entity<InputState>,
    editing: Option<String>,
    notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
    load_task: Option<Task<()>>,
    // The project the program list was read for: its default, and the default of a retry.
    project: Option<kg_domain::ProjectId>,
    // A save or rename is in flight. A second submit meanwhile is ignored, so Enter twice cannot
    // rename the same program twice.
    saving: bool,
}

impl EventEmitter<Start> for Picker {}
impl EventEmitter<Close> for Picker {}

mod list;
mod mark;

use list::{Field, custom_command, custom_field};
mod render;
pub(crate) mod saved;
mod state;
impl Picker {
    fn render_custom(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        if self.editing.is_some() {
            return v_flex()
                .gap_2()
                .child(custom_field(
                    "custom-name",
                    "Name",
                    &self.name,
                    None,
                    muted,
                    cx,
                ))
                .child(saved::save_control(true))
                .into_any_element();
        }
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
            .child(custom_field(
                "custom-name",
                "Name",
                &self.name,
                None,
                muted,
                cx,
            ))
            .child(saved::save_control(false))
            .into_any_element()
    }
}
