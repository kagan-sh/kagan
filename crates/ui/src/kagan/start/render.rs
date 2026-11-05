use crate::theme::TypeScale as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProjectId, ProjectOp};
use std::path::PathBuf;

use super::project_kind;
use crate::kagan::Kagan;
use crate::kagan::controls::control;
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
        let ready = kind.is_some_and(|kind| self.start_environment(kind).is_some())
            && self.setup_ready()
            && !self.start.starting
            && self.picker.read(cx).command(cx).is_ok();
        let start = control(
            "start",
            Facts {
                disabled: !ready,
                ..Facts::button("Start")
            },
            Button::new("start-button-control")
                .primary()
                .label("Start")
                .loading(self.start.starting)
                .disabled(!ready)
                .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
        );
        v_flex()
            .id("start-form")
            .size_full()
            .child(
                v_flex()
                    .id("start-fields")
                    .ui_container()
                    .w_full()
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .w_full()
                            .min_h_full()
                            .items_center()
                            .justify_center()
                            .child(
                                v_flex()
                                    .w_full()
                                    .max_w(rems(32.))
                                    .gap_3()
                                    .p_4()
                                    .child(
                                        div()
                                            .id("start-prompt")
                                            .ui_container()
                                            .aria_label(format!("New thread in {name}"))
                                            .type_title()
                                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                            .child(format!("New thread in {name}")),
                                    )
                                    .child(self.picker.clone())
                                    .children(kind.map(|kind| self.render_folder(kind, cx)))
                                    .children(self.setup_block(cx))
                                    .child(h_flex().justify_end().child(start)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Adds the folder the person picked. On success its start screen opens, so the first thread
    /// is one step away.
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
