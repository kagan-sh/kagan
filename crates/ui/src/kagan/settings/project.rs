//! The page for one project: its environment variables, its default program, and the spot where
//! its agent rules, which are the same editor the project menu opens. It lists the way Settings does.

use crate::theme::{SpaceStep, TypeScale as _, space};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    WeakEntity, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, Program, ProgramOp, ProjectId, ProjectOp};

use super::group_block;
use super::row::{Group, Row, group, row};
use crate::kagan::Kagan;
use crate::kagan::controls::control;
use crate::kagan::rules::RulesEditor;
use crate::kagan::screen::{Center, FocusReturn};
use crate::kagan::variables::Variables;
use crate::ui_registry::Facts;

pub(in crate::kagan) struct Page {
    project: ProjectId,
    name: SharedString,
    program: Option<String>,
    programs: Vec<Program>,
    variables: Entity<Variables>,
    rules: Entity<RulesEditor>,
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
        variables.update(cx, Variables::reload);
        let mut page = Self {
            project,
            name: SharedString::default(),
            program: None,
            programs: Vec::new(),
            variables,
            rules,
            kagan,
            note: None,
            load: None,
            change: None,
        };
        page.reload(cx);
        page
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

    fn program_row(&self, cx: &mut Context<Self>) -> Row {
        let current = self.program.clone();
        let mut buttons = h_flex().gap_2().flex_wrap().items_center();
        for program in &self.programs {
            let picked = program.command.clone();
            let selected = current.as_deref() == Some(program.command.as_str());
            let button = Button::new(ElementId::Name(
                format!("button-project-program-{}", program.command).into(),
            ))
            .small()
            .label(program.label.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.pick(Some(picked.clone()), cx);
            }));
            let button = if selected {
                button.primary()
            } else {
                button.outline()
            };
            buttons = buttons.child(control(
                format!("project-program-{}", program.command),
                Facts::button(program.label.clone()),
                button,
            ));
        }
        let first = Button::new("button-project-program-first")
            .small()
            .outline()
            .label("First installed")
            .on_click(cx.listener(|this, _, _, cx| this.pick(None, cx)));
        buttons = buttons.child(control(
            "project-program-first",
            Facts::button("First installed"),
            first,
        ));
        row(
            "project-program",
            "Program",
            "New threads in this project start with this program. Only installed programs are listed.",
            buttons.into_any_element(),
        )
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
        ]
    }
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let groups = self.groups(cx);
        let done = Button::new("button-project-settings-done")
            .small()
            .outline()
            .label("Done")
            .on_click(cx.listener(|this, _, window, cx| this.done(window, cx)));
        v_flex()
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
                    .child(control(
                        "project-settings-done",
                        Facts::button("Done"),
                        done,
                    )),
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
    pub(crate) fn open_project_page(
        &mut self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen.current(), Center::Project(_)) {
            self.screen.remember_center(window, cx);
        }
        let kagan = cx.entity().downgrade();
        let page = cx.new(|cx| Page::new(project, kagan, window, cx));
        self.open_center(Center::Project(page), window, cx);
        cx.notify();
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
