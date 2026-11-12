use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, AppContext as _, Context, Focusable as _, Window};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProgramList, ProgramOp};

use super::list::custom_command;
use super::{Cards, Mode, Picker, Programs, Start};

impl Picker {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search programs"));
        let program =
            cx.new(|cx| InputState::new(window, cx).placeholder("Program, such as claude"));
        let arguments = cx.new(|cx| InputState::new(window, cx).placeholder("Arguments, if any"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name, to save it"));
        let mut subscriptions =
            vec![
                cx.subscribe_in(&search, window, |this, _, event, _, cx| match event {
                    InputEvent::Change => this.search_changed(cx),
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
        // A refusal says what to change, so it goes once the person starts changing a field.
        subscriptions.extend([&program, &arguments, &name].map(|field| {
            cx.subscribe_in(field, window, |this, _, event, _, cx| {
                if let InputEvent::Change = event {
                    this.notice = None;
                    cx.notify();
                }
            })
        }));
        subscriptions.push(
            cx.subscribe_in(&name, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.save_program(window, cx);
                }
            }),
        );
        Self {
            state: Programs::default(),
            cards: Cards::new(),
            grid: cx.focus_handle().tab_stop(true),
            scroll: gpui_kit::ScrollHandle::new(),
            search,
            mode: Mode::Catalog,
            program,
            arguments,
            name,
            editing: None,
            notice: None,
            _subscriptions: subscriptions,
            load_task: None,
            project: None,
            saving: false,
        }
    }

    pub(crate) fn project(&self) -> Option<kg_domain::ProjectId> {
        self.project.clone()
    }

    // Opening the picker: a search from an earlier visit is cleared, and the grid shows a loading
    // line until the programs arrive.
    pub(crate) fn load(
        &mut self,
        project: Option<kg_domain::ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.state = Programs::Loading;
        self.cards.loading = true;
        self.reload(project, window, cx);
    }

    // A change to the saved programs: the grid stays on screen, and the card the person had
    // selected stays selected if it is still there.
    pub(super) fn reload(
        &mut self,
        project: Option<kg_domain::ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project.clone_from(&project);
        self.set_mode(Mode::Catalog, cx);
        self.notice = None;
        let task = Workspace::dispatch(
            Op::Program(ProgramOp::Discover {
                project_id: project.clone(),
            }),
            cx,
        );
        let window = window.window_handle();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, _, cx| {
                _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(OpResult::Programs(list)) => this.show(list, cx),
                        Ok(_) => this.state = Programs::Failed("the reply had no programs".into()),
                        Err(error) => this.state = Programs::Failed(error.to_string()),
                    }
                    cx.notify();
                });
            });
            let versions = cx.update(|cx| {
                Workspace::dispatch(
                    Op::Program(ProgramOp::List {
                        project_id: project,
                    }),
                    cx,
                )
            });
            if let Ok(OpResult::Programs(programs)) = versions.await {
                _ = this.update(cx, |this, cx| {
                    let cards = &mut this.cards;
                    for program in &mut cards.all {
                        if let Some(found) = programs.programs.iter().find(|found| {
                            found.command == program.command && found.path == program.path
                        }) {
                            program.version.clone_from(&found.version);
                        }
                    }
                    for program in &mut cards.matches {
                        if let Some(found) = cards.all.iter().find(|found| {
                            found.command == program.command && found.path == program.path
                        }) {
                            program.version.clone_from(&found.version);
                        }
                    }
                    cx.notify();
                });
            }
        }));
    }

    // Enter, a card's click and the search's Enter start the selected program. The custom card
    // opens its fields instead, which the key and the click do themselves.
    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        self.set_mode(Mode::Catalog, cx);
        if self.command(cx).is_ok() {
            cx.emit(Start);
        }
        cx.notify();
    }

    // A card's click: the card is selected and the grid takes the focus. The custom card opens
    // its fields, and a program card starts.
    pub(super) fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.grid.clone(), cx);
        self.cards.cursor = index;
        self.confirm(cx);
    }

    fn search_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        self.cards.search(&query);
        cx.notify();
    }

    // Moves the selection across the agent cards. Up and Down pass the number of cards in a row;
    // a move past either end stops at the first or last card.
    pub(super) fn move_by(&mut self, step: isize, cx: &mut Context<Self>) {
        let last = self.cards.matches.len().saturating_sub(1).cast_signed();
        let next = (self.cards.cursor.cast_signed() + step).clamp(0, last);
        self.cards.cursor = next.cast_unsigned();
        self.scroll.scroll_to_item(self.cards.cursor);
        cx.notify();
    }

    fn show(&mut self, list: ProgramList, cx: &mut Context<Self>) {
        let reloading = matches!(self.state, Programs::Loaded);
        let kept = reloading
            .then(|| self.cards.selected().map(|p| (p.command.clone(), p.source)))
            .flatten();
        self.state = Programs::Loaded;
        self.cards.loading = false;
        self.cards.all = list.programs;
        self.cards.filter();
        let place = kept.and_then(|(command, source)| {
            self.cards
                .matches
                .iter()
                .position(|p| p.command == command && p.source == source)
        });
        if let Some(row) = place {
            self.cards.cursor = row;
            self.scroll.scroll_to_item(row);
        } else if let Some(default) = &list.default
            && let Some(row) = self
                .cards
                .matches
                .iter()
                .position(|p| &p.command == default)
        {
            self.cards.cursor = row;
        }
        cx.notify();
    }

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

    pub(super) fn close_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Catalog {
            return;
        }
        self.set_mode(Mode::Catalog, cx);
        self.focus(window, cx);
        cx.notify();
    }

    // Leaving the custom command ends a rename too.
    pub(super) fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        if mode == Mode::Catalog {
            self.editing = None;
        }
        cx.notify();
    }

    fn focus_custom(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.program.focus_handle(cx), cx);
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.search.focus_handle(cx), cx);
    }

    #[cfg(test)]
    pub(crate) fn focus_list(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.grid, cx);
    }

    pub(crate) fn command(&self, cx: &App) -> Result<String, String> {
        if self.mode == Mode::Custom {
            let program = self.program.read(cx).value().trim().to_owned();
            let arguments = self.arguments.read(cx).value().trim().to_owned();
            return custom_command(&program, &arguments).map_err(|(_, message)| message);
        }
        match self.cards.selected() {
            Some(program) if program.installed() => Ok(program.command.clone()),
            Some(program) => Err(format!(
                "{} is not installed. Install it, or choose another program.",
                program.label
            )),
            None => Err("Choose a program to start.".into()),
        }
    }
}
