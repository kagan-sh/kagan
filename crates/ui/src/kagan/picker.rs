use crate::theme::TypeScale as _;
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::{Icon, IconName, IndexPath, Selectable, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, Focusable as _, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels, Render,
    RenderOnce, Role, StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity,
    Window, div,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, Program, ProgramList, ProgramOp, ProgramSource, split_program};

use super::shown::{container, control, dispatch, note};
use super::{CloseCustomCommand, RetryPrograms, ToggleCustomCommand};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

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

impl Picker {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let list = cx.new(|cx| ListState::new(Rows::default(), window, cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search programs"));
        let program =
            cx.new(|cx| InputState::new(window, cx).placeholder("Program, such as claude"));
        let arguments = cx.new(|cx| InputState::new(window, cx).placeholder("Arguments, if any"));
        let mut subscriptions = vec![
            cx.observe(&list, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &list,
                window,
                |this, _, event: &ListEvent, window, cx| match event {
                    ListEvent::Confirm(ix) => {
                        this.list
                            .update(cx, |list, _| list.delegate_mut().selected = Some(ix.row));
                        this.confirm(cx);
                    }
                    ListEvent::Cancel => {
                        // Escape makes the list drop its own selection. The row the person saw stays
                        // the one that starts, so the list is put back to the delegate's choice.
                        let was_custom = this.mode == Mode::Custom;
                        this.set_mode(Mode::Catalog, cx);
                        let selected = this.list.read(cx).delegate().selected;
                        this.list.update(cx, |list, cx| {
                            list.set_selected_index(selected.map(IndexPath::new), window, cx);
                        });
                        cx.notify();
                        if !was_custom {
                            cx.emit(Close);
                        }
                    }
                    ListEvent::Select(_) => cx.notify(),
                },
            ),
            cx.subscribe_in(&search, window, |this, _, event, window, cx| match event {
                InputEvent::Change => this.search_changed(window, cx),
                InputEvent::PressEnter { .. } => this.confirm(cx),
                InputEvent::Focus | InputEvent::Blur => {}
            }),
        ];
        subscriptions.extend([&program, &arguments].map(|field| {
            cx.subscribe_in(field, window, |_, _, event, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    cx.emit(Start);
                }
            })
        }));
        Self {
            state: Programs::default(),
            list,
            search,
            mode: Mode::Catalog,
            program,
            arguments,
            _subscriptions: subscriptions,
            load_task: None,
        }
    }

    /// Reads the programs. A read replaces one still running, so only the latest answer shows.
    pub(crate) fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state = Programs::Loading;
        self.list
            .update(cx, |list, _| list.delegate_mut().loading = true);
        let task = Workspace::dispatch(Op::Program(ProgramOp::Discover), cx);
        let window = window.window_handle();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(OpResult::Programs(list)) => this.show(list, window, cx),
                        Ok(_) => this.state = Programs::Failed("the reply had no programs".into()),
                        Err(error) => this.state = Programs::Failed(error.to_string()),
                    }
                    cx.notify();
                });
            });
            let versions = cx.update(|cx| Workspace::dispatch(Op::Program(ProgramOp::List), cx));
            if let Ok(OpResult::Programs(programs)) = versions.await {
                _ = this.update(cx, |this, cx| {
                    this.list.update(cx, |state, cx| {
                        let rows = state.delegate_mut();
                        for program in &mut rows.all {
                            if let Some(found) = programs.programs.iter().find(|found| {
                                found.command == program.command && found.path == program.path
                            }) {
                                program.version.clone_from(&found.version);
                            }
                        }
                        for program in &mut rows.matches {
                            if let Some(found) = rows.all.iter().find(|found| {
                                found.command == program.command && found.path == program.path
                            }) {
                                program.version.clone_from(&found.version);
                            }
                        }
                        cx.notify();
                    });
                });
            }
        }));
    }

    /// Starts the chosen program. A chosen program is the one to start, so the custom command closes.
    fn confirm(&mut self, cx: &mut Context<Self>) {
        self.set_mode(Mode::Catalog, cx);
        if self.command(cx).is_ok() {
            cx.emit(Start);
        }
        cx.notify();
    }

    fn search_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        self.list.update(cx, |state, cx| {
            state.delegate_mut().search(&query);
            let selected = state.delegate().selected.map(IndexPath::new);
            state.set_selected_index(selected, window, cx);
            cx.notify();
        });
    }

    /// Moves the list's selection by `step` rows, within the rows that match the search.
    fn move_selection(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |state, cx| {
            let rows = state.delegate();
            let count = rows.matches.len();
            if count == 0 {
                return;
            }
            let current = rows.selected.unwrap_or(0).cast_signed();
            let next = (current + step)
                .clamp(0, count.cast_signed() - 1)
                .cast_unsigned();
            state.set_selected_index(Some(IndexPath::new(next)), window, cx);
            state.scroll_to_selected_item(window, cx);
        });
    }

    /// Shows the programs under the search that is still in the box, and selects the default when
    /// the search leaves it. The list itself gets the selection, so arrow keys move from it.
    fn show(&mut self, list: ProgramList, window: &mut Window, cx: &mut Context<Self>) {
        self.state = Programs::Loaded;
        self.list.update(cx, |state, cx| {
            let rows = state.delegate_mut();
            rows.loading = false;
            rows.row_height = None;
            rows.all = list.programs;
            rows.filter();
            if let Some(default) = &list.default
                && let Some(row) = rows.matches.iter().position(|p| &p.command == default)
            {
                rows.selected = Some(row);
            }
            let selected = rows.selected.map(IndexPath::new);
            state.set_selected_index(selected, window, cx);
        });
    }

    /// Opens the custom command with `program` and `arguments` in its fields.
    #[cfg(test)]
    pub(crate) fn use_custom(
        &mut self,
        program: &str,
        arguments: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_mode(Mode::Custom, cx);
        self.program
            .update(cx, |state, cx| state.set_value(program, window, cx));
        self.arguments
            .update(cx, |state, cx| state.set_value(arguments, window, cx));
        self.focus_custom(window, cx);
        cx.notify();
    }

    /// Opens or closes the custom command. Opening it puts the cursor in its Program field, and
    /// closing it returns focus to the list.
    pub(crate) fn toggle_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            Mode::Custom => self.close_custom(window, cx),
            Mode::Catalog => {
                self.set_mode(Mode::Custom, cx);
                self.focus_custom(window, cx);
                cx.notify();
            }
        }
    }

    /// Closes the custom command, if it is open, and returns focus to the list.
    fn close_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Catalog {
            return;
        }
        self.set_mode(Mode::Catalog, cx);
        self.focus(window, cx);
        cx.notify();
    }

    // The list draws its selection from the same mode, so both change together.
    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.list.update(cx, |list, cx| {
            list.delegate_mut().mode = mode;
            cx.notify();
        });
    }

    fn focus_custom(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.program.focus_handle(cx), cx);
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.search.focus_handle(cx), cx);
    }

    #[cfg(test)]
    pub(crate) fn focus_list(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.focus(window, cx));
    }

    /// The command a thread starts with: the selected program, or the custom command.
    pub(crate) fn command(&self, cx: &App) -> Result<String, String> {
        if self.mode == Mode::Custom {
            let program = self.program.read(cx).value().trim().to_owned();
            let arguments = self.arguments.read(cx).value().trim().to_owned();
            return custom_command(&program, &arguments).map_err(|(_, message)| message);
        }
        let list = self.list.read(cx).delegate();
        match list.selected() {
            Some(program) if program.installed() => Ok(program.command.clone()),
            Some(program) => Err(format!(
                "{} is not installed. Install it, or choose another program.",
                program.label
            )),
            None => Err("Choose a program to start.".into()),
        }
    }
}

/// The custom command's field a problem is about, so its message sits under that field.
enum Field {
    Program,
    Arguments,
}

/// `program` and `arguments` as one command line: the program quoted when it has a space.
fn custom_command(program: &str, arguments: &str) -> Result<String, (Field, String)> {
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
            .type_body()
            .text_color(cx.theme().muted_foreground)
            .child(format!("No program matches “{}”.", self.query))
    }
}

#[derive(IntoElement)]
pub(super) struct ProgramRow {
    base: ListItem,
    program: Program,
    selected: bool,
    mode: Mode,
    /// The first row sets the list's height. Rows can differ, so the others do not.
    first: bool,
    list: WeakEntity<ListState<Rows>>,
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
fn custom_field(
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
                .child(div().type_small().text_color(muted).child(title))
                .child(Input::new(input).text_size(gpui_kit::px(crate::theme::TYPE_BODY)))
                .children(note.map(|(id, text)| {
                    div()
                        .id(id)
                        .ui_container()
                        .type_small()
                        .text_color(muted)
                        .aria_label(text.clone())
                        .child(text)
                })),
        )
}

/// What a row says after the name: the first line of the version, or that the program is missing.
fn status_line(program: &Program) -> String {
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
                                            .type_body()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .aria_label(name)
                                            .child(self.program.label),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .type_small()
                                            .text_color(muted)
                                            .text_ellipsis()
                                            .child(self.program.command),
                                    )
                                    .child(
                                        div()
                                            .max_w(gpui_kit::px(100.))
                                            .type_caption()
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
fn programs_failed(message: &str, muted: Hsla) -> AnyElement {
    // The message sits above the button: a long path in it must not push the Retry out of view.
    v_flex()
        .gap_2()
        .items_start()
        .child(
            div()
                .w_full()
                .type_body()
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

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let body: AnyElement = if let Programs::Failed(message) = &self.state {
            programs_failed(message, muted)
        } else {
            // Capped at LIST_ROWS rows once the first row is measured. Until then, all rows show.
            let cap = self
                .list
                .read(cx)
                .delegate()
                .row_height
                .map(|row| row * f32::from(LIST_ROWS));
            List::new(&self.list)
                .when_some(cap, gpui_kit::Styled::max_h)
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
                    .text_size(gpui_kit::px(crate::theme::TYPE_BODY))
                    .small(),
            ))
            .child(container("program-list", body))
            .when_some(reason, |this, reason| {
                this.child(note("program-reason", reason, muted))
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
