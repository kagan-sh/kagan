//! One project's automations: the saved text, and the thread each run started.

use crate::theme::TypeScale as _;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, SharedString, Styled as _, Task, Window, div,
};
use kg_core::Workspace;
use kg_domain::{Automation, AutomationId, AutomationOp, Op, OpResult, ProjectId, Timestamp};

use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) struct List {
    project: ProjectId,
    rows: Vec<Automation>,
    name: Entity<InputState>,
    body: Entity<InputState>,
    program: Entity<InputState>,
    minutes: Entity<InputState>,
    at: Entity<InputState>,
    note: Option<SharedString>,
    load: Option<Task<()>>,
    change: Option<Task<()>>,
}

impl List {
    pub(super) fn new(project: ProjectId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let mut list = Self {
            project,
            rows: Vec::new(),
            name: field("Name", window, cx),
            body: field("Text to send", window, cx),
            program: field("Program", window, cx),
            minutes: field("Minutes", window, cx),
            at: field("Once at, ms", window, cx),
            note: None,
            load: None,
            change: None,
        };
        list.reload(cx);
        list
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let task = Workspace::dispatch(
            Op::Automation(AutomationOp::List {
                project_id: Some(project),
            }),
            cx,
        );
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                if let Ok(OpResult::Automations(rows)) = result {
                    this.rows = rows;
                }
                cx.notify();
            });
        }));
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let minutes = self.minutes.read(cx).value().to_string();
        let at = self.at.read(cx).value().to_string();
        let minutes = match parse_minutes(&minutes) {
            Ok(minutes) => minutes,
            Err(error) => {
                self.note = Some(error.into());
                cx.notify();
                return;
            }
        };
        let at = match parse_at(&at) {
            Ok(at) => at,
            Err(error) => {
                self.note = Some(error.into());
                cx.notify();
                return;
            }
        };
        let op = Op::Automation(AutomationOp::Add {
            name: self.name.read(cx).value().to_string(),
            body: self.body.read(cx).value().to_string(),
            project_id: self.project.clone(),
            program: self.program.read(cx).value().to_string(),
            minutes,
            at,
        });
        self.send(op, true, window, cx);
    }

    fn pause(
        &mut self,
        id: AutomationId,
        paused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let op = if paused {
            Op::Automation(AutomationOp::Resume { id })
        } else {
            Op::Automation(AutomationOp::Pause { id })
        };
        self.send(op, false, window, cx);
    }

    fn delete(&mut self, id: AutomationId, window: &mut Window, cx: &mut Context<Self>) {
        self.send(
            Op::Automation(AutomationOp::Delete { id }),
            false,
            window,
            cx,
        );
    }

    fn send(&mut self, op: Op, clear: bool, window: &mut Window, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(op, cx);
        self.change = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(_) => {
                        this.note = None;
                        if clear {
                            for field in [
                                &this.name,
                                &this.body,
                                &this.program,
                                &this.minutes,
                                &this.at,
                            ] {
                                field.update(cx, |state, cx| state.set_value("", window, cx));
                            }
                        }
                        this.reload(cx);
                    }
                    Err(error) => this.note = Some(error.to_string().into()),
                }
                cx.notify();
            });
        }));
    }
}

fn parse_minutes(text: &str) -> Result<Option<u16>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<u16>()
        .ok()
        .filter(|minutes| (1..=1440).contains(minutes))
        .map(Some)
        .ok_or("minutes must be a whole number from 1 to 1440")
}

fn parse_at(text: &str) -> Result<Option<Timestamp>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let at = text
        .parse::<i64>()
        .map(Timestamp)
        .map_err(|_| "a time is milliseconds since the Unix epoch")?;
    if at.0 < 0 {
        return Err("a time cannot be negative");
    }
    Ok(Some(at))
}

fn when(automation: &Automation) -> String {
    match automation.minutes {
        Some(minutes) => format!("Every {minutes} min"),
        None => "Once".into(),
    }
}

fn threads(automation: &Automation) -> String {
    if automation.threads.is_empty() {
        return "No thread yet".into();
    }
    automation
        .threads
        .iter()
        .map(|id| id.0.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn labeled(name: impl Into<SharedString>) -> Facts {
    Facts {
        role: Role::Label,
        ..Facts::button(name)
    }
}

fn row(automation: &Automation, cx: &mut Context<List>) -> gpui_kit::AnyElement {
    let muted = cx.theme().muted_foreground;
    let pause_id = automation.id.clone();
    let delete_id = automation.id.clone();
    let paused = automation.paused;
    let pause = if paused { "Resume" } else { "Pause" };
    let started = threads(automation);
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(
            div()
                .id(ElementId::Name(
                    format!("automation-{}", automation.id).into(),
                ))
                .flex_1()
                .min_w_0()
                .type_small(cx)
                .ui_observe_as(labeled(automation.name.clone()))
                .child(automation.name.clone()),
        )
        .child(
            div()
                .type_small(cx)
                .text_color(muted)
                .child(when(automation)),
        )
        .child(
            div()
                .id(ElementId::Name(
                    format!("automation-threads-{}", automation.id).into(),
                ))
                .type_small(cx)
                .text_color(muted)
                .ui_observe_as(labeled(started.clone()))
                .child(started),
        )
        .child(control(
            format!("automation-pause-{}", automation.id),
            Facts::button(pause),
            Button::new(ElementId::Name(
                format!("button-automation-pause-{}", automation.id).into(),
            ))
            .small()
            .outline()
            .label(pause)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pause(pause_id.clone(), paused, window, cx);
            })),
        ))
        .child(control(
            format!("automation-delete-{}", automation.id),
            Facts::button("Delete"),
            Button::new(ElementId::Name(
                format!("button-automation-delete-{}", automation.id).into(),
            ))
            .small()
            .outline()
            .label("Delete")
            .on_click(cx.listener(move |this, _, window, cx| {
                this.delete(delete_id.clone(), window, cx);
            })),
        ))
        .into_any_element()
}

fn field(
    id: &'static str,
    state: &Entity<InputState>,
    name: &str,
    cx: &mut Context<List>,
) -> impl IntoElement {
    div()
        .id(id)
        .ui_observe_as(Facts::input(name, state, cx))
        .flex_1()
        .min_w_0()
        .child(Input::new(state).small())
}

impl Render for List {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let note = self.note.clone();
        v_flex()
            .w_full()
            .gap_2()
            .children(self.rows.iter().map(|automation| row(automation, cx)))
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(field("automation-name", &self.name, "Name", cx))
                    .child(field("automation-body", &self.body, "Text", cx))
                    .child(field("automation-program", &self.program, "Program", cx))
                    .child(field("automation-minutes", &self.minutes, "Minutes", cx))
                    .child(field("automation-at", &self.at, "Once at", cx))
                    .child(control(
                        "automation-add",
                        Facts::button("Add automation"),
                        Button::new("button-automation-add")
                            .small()
                            .outline()
                            .label("Add")
                            .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                    )),
            )
            .children(note.map(|note| {
                div()
                    .id("automation-note")
                    .type_small(cx)
                    .text_color(cx.theme().danger)
                    .ui_observe_as(labeled(note.clone()))
                    .child(note)
            }))
    }
}
