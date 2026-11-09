use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProjectId, ProjectKind, ProjectOp};
use std::path::PathBuf;

use super::{LeaveStart, START_CONTEXT, project_kind};
use crate::kagan::Kagan;
use crate::kagan::controls::{control, note};
use crate::kagan::picker::Close;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
impl Kagan {
    pub(in crate::kagan) fn render_start(
        &self,
        project: &ProjectId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .sidebar
            .read(cx)
            .project(project)
            .map_or_else(String::new, |project| project.name.clone());
        let kind = project_kind(project, cx);
        let ready = kind.is_some_and(|kind| self.start_environment(kind, cx).is_some())
            && self.setup_ready()
            && !self.start.starting
            && self.picker.read(cx).command(cx).is_ok();
        // The column is as tall as the window, less its top offset, so the fields scroll inside it
        // and the actions stay its last child, under the fields.
        v_flex()
            .id("start-form")
            .size_full()
            .items_center()
            .key_context(START_CONTEXT)
            .on_action(cx.listener(|this, _: &LeaveStart, window, cx| {
                this.leave_start(window, cx);
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
                                                    .aria_label(format!("New thread in {name}"))
                                                    .type_title(cx)
                                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                                    .child(format!("New thread in {name}")),
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
                                    .children(kind.map(|kind| self.render_folder(kind, cx)))
                                    .children(self.setup_block(cx)),
                            ),
                    )
                    .child(self.start_actions(kind, ready, cx)),
            )
            .into_any_element()
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
                .on_click(cx.listener(|this, _, _, cx| {
                    this.picker.update(cx, |_, cx| cx.emit(Close));
                })),
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
                        .child(div().opacity(0.7).type_caption(cx).child("Enter")),
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
