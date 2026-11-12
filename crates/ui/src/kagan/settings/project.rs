//! The page for one project: its environment variables, its default program, and the spot where
//! its agent rules, which are the same editor the project menu opens. It lists the way Settings does.

use crate::theme::{SpaceStep, TypeScale as _, radius_md, space};

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Render, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, WeakEntity, Window, actions, div, px,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, Program, ProgramOp, ProjectId, ProjectOp, ProjectSection};

use super::automations::List;

use super::group_block;
use super::row::{Group, Row, group, row};
use crate::kagan::Kagan;
use crate::kagan::controls::{choice_item, choice_trigger, program_key};
use crate::kagan::rules::RulesEditor;
use crate::kagan::screen::{Center, FocusReturn};
use crate::kagan::variables::Variables;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

actions!(project_settings, [Close]);

const CONTEXT: &str = "ProjectSettings";

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", Close, Some(CONTEXT))]
}

pub(in crate::kagan) struct Page {
    project: ProjectId,
    name: SharedString,
    program: Option<String>,
    programs: Vec<Program>,
    // The program list has come back, so an unknown stored program can be named as missing.
    programs_loaded: bool,
    focus: FocusHandle,
    variables: Entity<Variables>,
    rules: Entity<RulesEditor>,
    automations: Entity<List>,
    scroll: ScrollHandle,
    kagan: WeakEntity<Kagan>,
    note: Option<SharedString>,
    // Each task is kept while it runs; dropping one cancels it.
    load: Option<Task<()>>,
    change: Option<Task<()>>,
}

impl Page {
    pub(in crate::kagan) fn new(
        project: ProjectId,
        kagan: WeakEntity<Kagan>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scope = project.clone();
        let variables = cx.new(|cx| Variables::new(Some(scope.clone()), window, cx));
        let rules = cx.new(|cx| RulesEditor::new(Some(scope), window, cx));
        let automations = cx.new(|cx| List::new(project.clone(), window, cx));
        let mut page = Self {
            project,
            name: SharedString::default(),
            program: None,
            programs: Vec::new(),
            programs_loaded: false,
            focus: cx.focus_handle(),
            variables,
            rules,
            automations,
            scroll: ScrollHandle::new(),
            kagan,
            note: None,
            load: None,
            change: None,
        };
        page.reload(cx);
        page
    }

    pub(in crate::kagan) fn project(&self) -> &ProjectId {
        &self.project
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let project = Workspace::dispatch(
            Op::Project(ProjectOp::Get {
                id: self.project.clone(),
            }),
            cx,
        );
        let programs = Workspace::dispatch(
            Op::Program(ProgramOp::Discover {
                project_id: Some(self.project.clone()),
            }),
            cx,
        );
        self.load = Some(cx.spawn(async move |this, cx| {
            let project = project.await;
            let programs = programs.await;
            _ = this.update(cx, |this, cx| {
                if let Ok(OpResult::Project(project)) = project {
                    this.name = project.name.into();
                    this.program = project.program;
                }
                if let Ok(OpResult::Programs(list)) = programs {
                    this.programs = list
                        .programs
                        .into_iter()
                        .filter(Program::installed)
                        .collect();
                    this.programs_loaded = true;
                }
                cx.notify();
            });
        }));
    }

    fn pick(&mut self, program: Option<String>, cx: &mut Context<Self>) {
        let op = Op::Project(ProjectOp::SetProgram {
            id: self.project.clone(),
            program: program.clone(),
        });
        let task = Workspace::dispatch(op, cx);
        self.change = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_) => {
                        this.program = program;
                        this.note = None;
                    }
                    Err(error) => this.note = Some(error.to_string().into()),
                }
                cx.notify();
            });
        }));
    }

    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(kagan) = self.kagan.upgrade() {
            kagan.update(cx, |kagan, cx| kagan.leave_project_page(window, cx));
        }
    }

    fn program_label(&self) -> String {
        match &self.program {
            Some(stored) => self
                .programs
                .iter()
                .find(|program| &program.command == stored)
                .map_or_else(|| stored.clone(), |program| program.label.clone()),
            None => "First installed".to_owned(),
        }
    }

    fn program_row(&self, cx: &mut Context<Self>) -> Row {
        let label = self.program_label();
        let page = cx.entity().downgrade();
        let current = self.program.clone();
        let (picked, programs) = (current.clone(), self.programs.clone());
        let choice = choice_trigger("project-program-choice", label, move |menu, _, _| {
            program_menu(menu, &page, picked.as_ref(), &programs)
        });
        let mut column = v_flex().w_full().items_end().gap_2().child(choice);
        if let Some(stored) = current.filter(|stored| {
            self.programs_loaded && !self.programs.iter().any(|p| &p.command == stored)
        }) {
            let text =
                format!("{stored} is not installed, so the first installed program is used.");
            column = column.child(
                div()
                    .id("project-program-missing")
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .ui_observe_as(Facts {
                        role: Role::Label,
                        ..Facts::button(text.clone())
                    })
                    .child(text),
            );
        }
        row(
            "project-program",
            "Program",
            "New threads in this project start with this program. Only installed programs are listed.",
            column.into_any_element(),
        )
    }

    // The scroll position is clamped to the page's end when it is drawn, so a large offset lands at
    // the bottom, where Automations is, without waiting for the page's height.
    pub(crate) fn reveal(&self, section: ProjectSection) {
        match section {
            ProjectSection::Automations => {
                self.scroll
                    .set_offset(gpui_kit::point(px(0.), px(-1_000_000.)));
            }
        }
    }

    fn groups(&self, cx: &mut Context<Self>) -> Vec<Group> {
        let variables = row(
            "project-variables",
            "Variables",
            "Only for this project. A value here wins over the same name set in General.",
            self.variables.clone().into_any_element(),
        );
        let mut rules = row(
            "project-rules",
            "Rules",
            "Text each agent is told when it starts in this project. It comes after the global rules.",
            self.rules.clone().into_any_element(),
        );
        rules.wide = true;
        let mut automations = row(
            "project-automations",
            "Automations",
            "A named text, a program, and a time. While kagan is open, a due one starts a thread and sends the text.",
            self.automations.clone().into_any_element(),
        );
        automations.wide = true;
        vec![
            Group {
                headed: true,
                ..group(
                    "project-variables",
                    "Environment variables",
                    vec![variables],
                )
            },
            Group {
                headed: true,
                ..group(
                    "project-program",
                    "Default program",
                    vec![self.program_row(cx)],
                )
            },
            Group {
                headed: true,
                ..group("project-rules", "Agent rules", vec![rules])
            },
            Group {
                headed: true,
                ..group("project-automations", "Automations", vec![automations])
            },
        ]
    }
}

// The row a stored line marks. A line can be saved under several names, so only the first row that
// runs it is marked, the same row the trigger's label names.
pub(crate) fn marked_program(programs: &[Program], current: Option<&String>) -> Option<usize> {
    current.and_then(|stored| {
        programs
            .iter()
            .position(|program| &program.command == stored)
    })
}

fn program_menu(
    menu: PopupMenu,
    page: &WeakEntity<Page>,
    current: Option<&String>,
    programs: &[Program],
) -> PopupMenu {
    let marked = marked_program(programs, current);
    let menu = programs
        .iter()
        .enumerate()
        .fold(menu.scrollable(true), |menu, (index, program)| {
            let item = choice_item(
                format!("project-program-menu-{}", program_key(program)),
                program.label.clone(),
                marked == Some(index),
            );
            let pick = program.command.clone();
            let page = page.clone();
            menu.item(item.on_click(move |_, _, cx| {
                _ = page.update(cx, |page, cx| page.pick(Some(pick.clone()), cx));
            }))
        });
    let page = page.clone();
    menu.item(
        choice_item(
            "project-program-menu-first".to_owned(),
            "First installed".to_owned(),
            current.is_none(),
        )
        .on_click(move |_, _, cx| {
            _ = page.update(cx, |page, cx| page.pick(None, cx));
        }),
    )
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let groups = self.groups(cx);
        let radius = px(radius_md(cx));
        v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Close, window, cx| this.done(window, cx)))
            .size_full()
            .p(px(space(SpaceStep::Xl, cx)))
            .gap(px(space(SpaceStep::Xl, cx)))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(
                        v_flex()
                            .child(div().type_title(cx).child("Project settings"))
                            .child(
                                div()
                                    .type_small(cx)
                                    .text_color(muted)
                                    .child(self.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("project-settings-close")
                            .px(px(space(SpaceStep::Sm, cx)))
                            .py(px(space(SpaceStep::Xs, cx)))
                            .rounded(radius)
                            .type_body(cx)
                            .on_click(cx.listener(|this, _, window, cx| this.done(window, cx)))
                            .child("Close")
                            .ui_observe_as(Facts::button("Close")),
                    ),
            )
            .children(self.note.clone().map(|note| {
                div()
                    .type_small(cx)
                    .text_color(cx.theme().danger)
                    .child(note)
            }))
            .child(
                div()
                    .id("project-settings-scroll")
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        v_flex()
                            .w_full()
                            .gap(px(space(SpaceStep::Xl, cx)))
                            .children(groups.into_iter().map(|group| group_block(group, cx))),
                    ),
            )
    }
}

impl Kagan {
    pub(in crate::kagan) fn open_project_page(
        &mut self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Page> {
        if !matches!(self.screen.current(), Center::Project(_)) {
            self.screen.remember_center(window, cx);
        }
        let kagan = cx.entity().downgrade();
        let page = cx.new(|cx| Page::new(project, kagan, window, cx));
        let focus = page.read(cx).focus.clone();
        self.open_center(Center::Project(page.clone()), window, cx);
        window.focus(&focus, cx);
        cx.notify();
        page
    }

    pub(in crate::kagan) fn leave_project_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let work = self.screen.shown().clone();
        self.open_center(Center::Work(work), window, cx);
        self.return_focus(FocusReturn::Center, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "project_test.rs"]
mod tests;
