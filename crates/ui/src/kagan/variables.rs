//! The environment variables a person sets, for every program or for one project. The list shows
//! names and that each one is set. A value goes into the field once and is never read back.

use crate::theme::TypeScale as _;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Task, Window, div, rems,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{EnvOp, Op, OpResult, ProjectId};

use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(crate) struct Variables {
    scope: Option<ProjectId>,
    names: Vec<String>,
    name: gpui_kit::Entity<InputState>,
    value: gpui_kit::Entity<InputState>,
    note: Option<SharedString>,
    // Each task is kept while it runs; dropping one cancels it.
    load: Option<Task<()>>,
    change: Option<Task<()>>,
    _stored: Subscription,
}

impl Variables {
    pub(crate) fn new(
        scope: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("NAME"));
        let value = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Value (hidden)")
        });
        let stored = cx.subscribe_in(
            &Workspace::global(cx),
            window,
            |this, _, event: &WorkspaceEvent, _, cx| {
                if matches!(event, WorkspaceEvent::Changed) {
                    this.reload(cx);
                }
            },
        );
        let mut variables = Self {
            scope,
            names: Vec::new(),
            name,
            value,
            note: None,
            load: None,
            change: None,
            _stored: stored,
        };
        variables.reload(cx);
        variables
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(
            Op::Env(EnvOp::List {
                project_id: self.scope.clone(),
            }),
            cx,
        );
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                if let Ok(OpResult::Variables(mut names)) = result {
                    names.sort();
                    this.names = names;
                }
                cx.notify();
            });
        }));
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_owned();
        let value = self.value.read(cx).value().to_string();
        let op = Op::Env(EnvOp::Set {
            name,
            project_id: self.scope.clone(),
            value: Some(value),
            thread_id: None,
        });
        self.send(op, window, cx);
    }

    fn remove(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let op = Op::Env(EnvOp::Remove {
            name,
            project_id: self.scope.clone(),
            thread_id: None,
        });
        self.send(op, window, cx);
    }

    fn send(&mut self, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(op, cx);
        self.change = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(_) => {
                        this.note = None;
                        this.name
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.value
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.reload(cx);
                    }
                    Err(error) => this.note = Some(error.to_string().into()),
                }
                cx.notify();
            });
        }));
    }
}

impl Render for Variables {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = self.names.iter().map(|name| {
            let remove_name = name.clone();
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(div().flex_1().min_w_0().type_small(cx).child(name.clone()))
                .child(div().type_small(cx).text_color(muted).child("Set"))
                .child(control(
                    format!("env-remove-{name}"),
                    Facts::button("Remove"),
                    Button::new(ElementId::Name(format!("button-env-remove-{name}").into()))
                        .small()
                        .outline()
                        .label("Remove")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.remove(remove_name.clone(), window, cx);
                        })),
                ))
        });
        v_flex()
            .w_full()
            .gap_2()
            .children(rows)
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .id("env-name")
                            .ui_observe_as(Facts::input("Variable name", &self.name, cx))
                            .w(rems(11.))
                            .child(Input::new(&self.name).small()),
                    )
                    .child(
                        div()
                            .id("env-value")
                            .ui_observe_as(Facts::input("Variable value", &self.value, cx))
                            .w(rems(13.75))
                            .child(Input::new(&self.value).small()),
                    )
                    .child(control(
                        "env-save",
                        Facts::button("Set variable"),
                        Button::new("button-env-save")
                            .small()
                            .outline()
                            .label("Set")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    )),
            )
            .children(self.note.clone().map(|note| {
                div()
                    .type_small(cx)
                    .text_color(cx.theme().danger)
                    .child(note)
            }))
    }
}
