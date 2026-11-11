use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::list::ListState;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Pixels,
    SharedString, Styled as _, Subscription, Task, WeakEntity,
};
use kg_domain::{Program, split_program};

use super::{CloseCustomCommand, RetryPrograms, ToggleCustomCommand};

pub(crate) const CONTEXT: &str = "ProgramPicker";

pub(crate) struct Start;
pub(crate) struct Close;

#[derive(Default)]
enum Programs {
    #[default]
    Loading,
    Loaded,
    Failed(String),
}

const LIST_ROWS: u16 = 8;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Catalog,
    Custom,
}

#[derive(Default)]
pub(super) struct Rows {
    all: Vec<Program>,
    matches: Vec<Program>,
    selected: Option<usize>,
    query: String,
    loading: bool,
    mode: Mode,
    row_height: Option<Pixels>,
    // The picker that owns these rows, for a saved program's pin, rename and delete buttons.
    picker: Option<WeakEntity<Picker>>,
}

impl Rows {
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
        self.selected = (!self.matches.is_empty()).then_some(0);
    }

    fn selected(&self) -> Option<&Program> {
        self.selected.and_then(|row| self.matches.get(row))
    }
}

pub(crate) struct Picker {
    state: Programs,
    list: Entity<ListState<Rows>>,
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
}

impl EventEmitter<Start> for Picker {}
impl EventEmitter<Close> for Picker {}

mod list;

use list::{Field, custom_command, custom_field};
mod render;
mod saved;
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
                .child(saved::save_control(true, cx))
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
            .child(saved::save_control(false, cx))
            .into_any_element()
    }
}
