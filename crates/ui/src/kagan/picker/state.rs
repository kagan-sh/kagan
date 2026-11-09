use gpui_kit::component::IndexPath;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::list::{ListEvent, ListState};
use gpui_kit::{App, AppContext as _, Context, Focusable as _, Window};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProgramList, ProgramOp};

use super::list::custom_command;
use super::{Mode, Picker, Programs, Rows, Start};

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
                        // CloseCustomCommand is the same key and emits Close, so this must not.
                        this.set_mode(Mode::Catalog, cx);
                        let selected = this.list.read(cx).delegate().selected;
                        this.list.update(cx, |list, cx| {
                            list.set_selected_index(selected.map(IndexPath::new), window, cx);
                        });
                        cx.notify();
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

    pub(super) fn move_selection(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn show(&mut self, list: ProgramList, window: &mut Window, cx: &mut Context<Self>) {
        self.state = Programs::Loaded;
        self.list.update(cx, |state, cx| {
            let rows = state.delegate_mut();
            rows.loading = false;
            rows.all = list.programs;
            rows.filter();
            if let Some(default) = &list.default
                && let Some(row) = rows.matches.iter().position(|p| &p.command == default)
            {
                rows.selected = Some(row);
            }
            let selected = rows.selected.map(IndexPath::new);
            state.set_selected_index(selected, window, cx);
            state.scroll_to_selected_item(window, cx);
        });
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
