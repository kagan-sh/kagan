use crate::theme::TypeScale as _;
use gpui_kit::Role;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    rems,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProjectId, ProjectKind, ProjectOp};
use std::path::PathBuf;

use super::{CONTROLS_CONTEXT, LeaveStart, START_CONTEXT, StartThread, project_kind};
use crate::kagan::controls::{Selected, bound_strokes, control, keys_text, note};
use crate::kagan::{Kagan, UseChatLink, UseTerminalLink};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
impl Kagan {
    pub(in crate::kagan) fn render_start(
        &self,
        project: &ProjectId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .sidebar
            .read(cx)
            .project(project)
            .map_or_else(String::new, |project| project.name.clone());
        let kind = project_kind(project, cx);
        let heading = self.start_heading(&name);
        let ready = self.start_ready(kind, cx);
        // The column is as tall as the window, less its top offset, so the fields scroll inside it
        // and the actions stay its last child, under the fields.
        v_flex()
            .id("start-form")
            .size_full()
            .items_center()
            .key_context(START_CONTEXT)
            .when_some(self.start.form_focus.clone(), |form, focus| {
                form.track_focus(&focus).tab_stop(false)
            })
            .on_action(cx.listener(|this, _: &LeaveStart, window, cx| {
                this.escape_start(window, cx);
            }))
            .on_action(cx.listener(|this, _: &StartThread, window, cx| {
                this.start_thread(window, cx);
            }))
            .child(
                v_flex()
                    .w_full()
                    .max_w(rems(40.))
                    .h_full()
                    .min_h_0()
                    .pt_16()
                    .px_4()
                    .pb_4()
                    .key_context(CONTROLS_CONTEXT)
                    .child(
                        v_flex()
                            .id("start-fields")
                            .ui_container()
                            .w_full()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                v_flex()
                                    .flex_shrink_0()
                                    .gap_4()
                                    .pb_4()
                                    .child(
                                        v_flex()
                                            .gap_4()
                                            .child(
                                                div()
                                                    .id("start-prompt")
                                                    .ui_container()
                                                    .aria_label(heading.clone())
                                                    .type_title(cx)
                                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                                    .child(heading),
                                            )
                                            .child(
                                                v_flex()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .id("program-caption")
                                                            .ui_container()
                                                            .type_caption(cx)
                                                            .text_color(cx.theme().muted_foreground)
                                                            .aria_label("Program")
                                                            .child("Program"),
                                                    )
                                                    .child(self.picker.clone()),
                                            ),
                                    )
                                    .children(kind.map(|kind| self.render_folder(kind, window, cx)))
                                    .children(self.run_as(window, cx))
                                    .children(self.start.prompt.is_some().then(|| {
                                        div()
                                            .id("start-fork-note")
                                            .type_caption(cx)
                                            .text_color(cx.theme().muted_foreground)
                                            .ui_observe_as(Facts {
                                                role: Role::Status,
                                                ..Facts::button("Fork prompt")
                                            })
                                            .child("The skill's fork prompt is typed into the program after Start.")
                                    }))
                                    .children(self.setup_block(cx)),
                            ),
                    )
                    .child(self.start_actions(kind, ready, cx)),
            )
            .into_any_element()
    }

    // Run as: Terminal or Chat, for an agent that answers ACP. Anything else only has a terminal.
    fn run_as(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let program = self.picker.read(cx).command(cx).ok()?;
        kg_domain::acp_command(&program)?;
        let chat = self.start.link(&program) == kg_domain::Link::Acp;
        let starting = self.start.starting;
        let focus = self.start.segments_focus.clone();
        let focused = window.last_input_was_keyboard()
            && focus
                .as_ref()
                .is_some_and(|focus| focus.contains_focused(window, cx));
        let toggle =
            |id: &'static str, name: SharedString, selected: bool, action: Box<dyn Action>| {
                let mark = if selected {
                    Selected::Yes
                } else {
                    Selected::No
                };
                crate::kagan::controls::segment(format!("{id}-segment"), mark, cx)
                    .type_body(cx)
                    .disabled(starting)
                    .child(control(
                        id,
                        Facts {
                            role: Role::RadioButton,
                            disabled: starting,
                            selected: Some(selected),
                            ..Facts::button(name.clone())
                        },
                        name,
                    ))
                    .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
            };
        let row = crate::kagan::controls::segments(cx)
            .when_some(focus, |row, focus| row.track_focus(&focus).tab_stop(false))
            .when(focused, |row| row.border_color(cx.theme().ring))
            .child(toggle(
                "run-as-terminal",
                "Terminal".into(),
                !chat,
                Box::new(UseTerminalLink),
            ))
            .child(toggle(
                "run-as-chat",
                "Chat".into(),
                chat,
                Box::new(UseChatLink),
            ));
        Some(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .id("run-as-caption")
                        .ui_container()
                        .type_caption(cx)
                        .text_color(cx.theme().muted_foreground)
                        .aria_label("Run as")
                        .child("Run as"),
                )
                .child(h_flex().gap_2().items_center().child(row))
                .children(chat.then(|| {
                    div()
                        .id("run-as-chat-note")
                        .type_caption(cx)
                        .text_color(cx.theme().muted_foreground)
                        .child("This agent answers in the thread’s transcript. You type prompts here; there is no terminal.")
                }))
                .into_any_element(),
        )
    }

    // Cancel leaves the screen the way Escape does. Start is the default commit, so it keeps its key.
    fn start_actions(
        &self,
        kind: Option<ProjectKind>,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let retry = self.setup_retry_pending();
        let keys = bound_strokes(&StartThread, cx);
        let label = if retry { "Retry setup" } else { "Start" };
        // Start waits for the branch's setup script, so say so while it is missing.
        let reason = if retry {
            Some("Fix .kagan/setup, then retry.")
        } else {
            (kind.is_some() && !self.setup_ready())
                .then_some("Start is off until kagan reads this branch’s setup script.")
        };
        let cancel = control(
            "start-cancel",
            Facts::button("Cancel"),
            Button::new("start-cancel-button")
                .small()
                .outline()
                .child(div().type_body(cx).child("Cancel"))
                .on_click(cx.listener(|this, _, window, cx| this.leave_start(window, cx))),
        );
        let start = control(
            "start",
            Facts {
                disabled: !ready,
                ..Facts::button(label)
            },
            Button::new("start-button-control")
                .primary()
                .small()
                .loading(self.start.starting)
                .disabled(!ready)
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().type_body(cx).child(label))
                        .child(
                            div()
                                .id("start-key")
                                .ui_container()
                                .aria_label(keys_text(&keys))
                                .flex()
                                .gap_1()
                                .children(keys.into_iter().map(Kbd::new)),
                        ),
                )
                .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
        );
        v_flex()
            .flex_shrink_0()
            .gap_4()
            .children(reason.map(|text| note("start-reason", text, muted, cx)))
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(cancel)
                    .child(start),
            )
            .into_any_element()
    }

    pub(crate) fn add_picked_project(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let added = Workspace::dispatch(Op::Project(ProjectOp::Add { path }), cx);
        let window = window.window_handle();
        cx.spawn(async move |this, cx| match added.await {
            Ok(OpResult::Project(project)) => {
                _ = cx.update_window(window, |_, window, cx| {
                    _ = this.update(cx, |this, cx| this.show_start(project.id, window, cx));
                });
            }
            Ok(_) => {}
            Err(error) => {
                _ = this.update(cx, |this, cx| {
                    this.note = Some(format!("Could not add the project: {error}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
