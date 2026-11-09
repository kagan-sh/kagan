use gpui_kit::component::button::Button;
use gpui_kit::component::h_flex;
use gpui_kit::{
    App, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, Window,
};
use kg_core::Workspace;
use kg_domain::{EnvironmentSpec, Op, OpResult, ProgramOp, ProjectId, Run, ThreadOp};

use super::{NO_AGENT, PLUGIN_PROMPT, follow_plugin_prompt};
use crate::kagan::Kagan;
use crate::ui_observe::UiObserveExt as _;
impl Kagan {
    pub(in crate::kagan) fn cancel_plugin_start(&mut self) {
        self.start.plugin_start = None;
        self.start.plugin_thread = None;
    }

    pub(in crate::kagan) fn add_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start.plugin_opening || self.start.plugin_start.is_some() {
            return;
        }
        self.start.plugin_opening = true;
        cx.notify();
        let list = Workspace::dispatch(Op::Program(ProgramOp::Discover), cx);
        let window = window.window_handle();
        // Detached, so a thread switch cannot drop a start that is still creating its thread.
        cx.spawn(async move |this, cx| {
            let found = match list.await {
                Ok(OpResult::Programs(list)) => Ok(list.agent().map(|agent| agent.command.clone())),
                Ok(_) => Err("kagan could not list the programs".to_owned()),
                Err(error) => Err(error.to_string()),
            };
            let program = match found {
                Ok(Some(program)) => program,
                other => {
                    let message = match other {
                        Err(message) => message,
                        Ok(_) => NO_AGENT.to_owned(),
                    };
                    _ = this.update(cx, |this, cx| {
                        this.start.plugin_opening = false;
                        this.note = Some(message.into());
                        cx.notify();
                    });
                    return;
                }
            };
            let task = cx.update(|cx| {
                Workspace::dispatch(
                    Op::Thread(ThreadOp::Run(Run {
                        project_id: ProjectId::personal(),
                        program,
                        title: Some("New plugin".into()),
                        environment: EnvironmentSpec::Personal,
                        visibility: None,
                        parent_thread_id: None,
                        quiet: false,
                        lifecycle_owner_thread_id: None,
                        metadata: None,
                    })),
                    cx,
                )
            });
            let id = match task.await {
                Ok(OpResult::Thread(thread)) => thread.id,
                other => {
                    _ = cx.update_window(window, |_, window, cx| {
                        _ = this.update(cx, |this, cx| {
                            this.start.plugin_opening = false;
                            this.finish(other, "start the plugin thread", window, cx);
                        });
                    });
                    return;
                }
            };
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.start.plugin_opening = false;
                    this.start.plugin_thread = Some(id.clone());
                    this.select(id.clone(), window, cx);
                    let handle = window.window_handle();
                    this.start.plugin_start = Some(cx.spawn(async move |this, cx| {
                        follow_plugin_prompt(this, handle, id, PLUGIN_PROMPT.into(), cx).await;
                    }));
                });
            });
        })
        .detach();
    }

    pub(in crate::kagan) fn plugin_prompt_visible(&self, cx: &App) -> bool {
        self.plugin_prompt_notice.is_some()
            && self.plugin_prompt_notice.as_ref() == self.shown_thread(cx).as_ref()
    }

    pub(in crate::kagan) fn plugin_prompt_notice(
        text: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let copied = text.to_owned();
        h_flex()
            .id("plugin-prompt-notice")
            .ui_container()
            .px_4()
            .py_2()
            .child(format!("Copy this prompt: {text}"))
            .child(
                Button::new("copy-plugin-prompt")
                    .label("Copy")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(copied.clone()));
                        this.plugin_prompt_notice = None;
                        cx.notify();
                    })),
            )
    }

    pub(in crate::kagan) fn new_personal_thread(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_start(ProjectId::personal(), window, cx);
    }
}
