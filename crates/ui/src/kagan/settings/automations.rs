//! One project's automations: the saved text, and the thread each run started.

use crate::theme::TypeScale as _;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::{
    AppContext as _, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, SharedString, Styled as _, Subscription, Task, Window, div,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    Automation, AutomationId, AutomationOp, Op, OpResult, ProjectId, ProjectOp, Timestamp,
};

use crate::kagan::controls::control;
use crate::kagan::window::open_thread;
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
    fallback: Option<String>,
    note: Option<SharedString>,
    load: Option<Task<()>>,
    change: Option<Task<()>>,
    _watch: Subscription,
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
            at: field("Once in (minutes)", window, cx),
            fallback: None,
            note: None,
            load: None,
            change: None,
            _watch: cx.subscribe(&Workspace::global(cx), |this, _, event, cx| {
                if matches!(event, WorkspaceEvent::Changed) {
                    this.reload(cx);
                }
            }),
        };
        list.reload(cx);
        list
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let task = Workspace::dispatch(
            Op::Automation(AutomationOp::List {
                project_id: Some(project.clone()),
            }),
            cx,
        );
        let project_task = Workspace::dispatch(Op::Project(ProjectOp::Get { id: project }), cx);
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let project = project_task.await;
            _ = this.update(cx, |this, cx| {
                if let Ok(OpResult::Automations(rows)) = result {
                    this.rows = rows;
                }
                if let Ok(OpResult::Project(project)) = project {
                    this.fallback = project.program;
                }
                cx.notify();
            });
        }));
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let every = self.minutes.read(cx).value().to_string();
        let once = self.at.read(cx).value().to_string();
        let minutes = match parse_minutes(&every) {
            Ok(minutes) => minutes,
            Err(error) => {
                self.note = Some(error.into());
                cx.notify();
                return;
            }
        };
        let at = match parse_once(&once) {
            Ok(at) => at,
            Err(error) => {
                self.note = Some(error.into());
                cx.notify();
                return;
            }
        };
        let typed = self.program.read(cx).value().trim().to_owned();
        let program = if typed.is_empty() {
            self.fallback.clone().unwrap_or_default()
        } else {
            typed
        };
        let op = Op::Automation(AutomationOp::Add {
            name: self.name.read(cx).value().to_string(),
            body: self.body.read(cx).value().to_string(),
            project_id: self.project.clone(),
            program,
            minutes,
            at,
        });
        self.send(op, true, window, cx);
    }

    fn toggle(
        &mut self,
        id: AutomationId,
        is_paused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let op = if is_paused {
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

fn parse_once(text: &str) -> Result<Option<Timestamp>, &'static str> {
    let Some(minutes) = parse_minutes(text)? else {
        return Ok(None);
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        });
    Ok(Some(Timestamp(now.saturating_add(
        i64::from(minutes).saturating_mul(60_000),
    ))))
}

fn when(automation: &Automation) -> String {
    match automation.minutes {
        Some(minutes) => format!("Every {minutes} min"),
        None => "Once".into(),
    }
}

fn runs(automation: &Automation) -> String {
    match automation.threads.len() {
        0 => "No run yet".into(),
        1 => "1 run".into(),
        count => format!("{count} runs"),
    }
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
    let started = runs(automation);
    let last = automation.threads.last().cloned();
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
        .children(last.map(|last| {
            control(
                format!("automation-open-{}", automation.id),
                Facts::button("Open last run"),
                Button::new(ElementId::Name(
                    format!("button-automation-open-{}", automation.id).into(),
                ))
                .small()
                .outline()
                .label("Open last run")
                .on_click(move |_, _, cx| {
                    let last = last.clone();
                    cx.defer(move |cx| open_thread(&last, cx));
                }),
            )
        }))
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
                this.toggle(pause_id.clone(), paused, window, cx);
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
                    .child(field("automation-at", &self.at, "Once in", cx))
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
