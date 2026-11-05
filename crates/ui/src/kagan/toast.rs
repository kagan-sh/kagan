use super::Kagan;
use super::controls;
use crate::theme::{SpaceStep, TypeScale as _, space};

use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    App, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use kg_domain::ThreadId;
use kg_plugins::RunCommand;

#[derive(Clone)]
pub(super) struct Toast {
    pub(super) message: SharedString,
    pub(super) action: Option<ToastCommand>,
    pub(super) undo: Vec<ThreadId>,
}

impl Toast {
    pub(super) fn said(message: String) -> Self {
        Self {
            message: message.into(),
            action: None,
            undo: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ToastCommand {
    pub(super) plugin: kg_domain::PluginId,
    pub(super) command: String,
    pub(super) label: SharedString,
}

impl Kagan {
    pub(super) fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toast_epoch = self.toast_epoch.wrapping_add(1);
        let epoch = self.toast_epoch;
        let stays = toast.action.is_some();
        let pause = if toast.undo.is_empty() {
            std::time::Duration::from_secs(5)
        } else {
            std::time::Duration::from_secs(10)
        };
        self.toast = Some(toast);
        cx.notify();
        if stays {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(pause).await;
            _ = this.update(cx, |this, cx| {
                if this.toast_epoch == epoch {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn render_toast(toast: &Toast, cx: &mut Context<Self>) -> impl IntoElement {
        let message = toast.message.clone();
        let ids = toast.undo.clone();
        let undo = (!ids.is_empty()).then(|| {
            toast_button(
                "toast-undo",
                "Undo".into(),
                cx.listener(move |this, _, window, cx| this.undo_archive(ids.clone(), window, cx)),
            )
        });
        let action = toast.action.clone().map(|action| {
            let run = RunCommand::new(action.plugin.clone(), action.command.clone());
            toast_button(
                "toast-action",
                action.label.clone(),
                move |_, window, cx| window.dispatch_action(Box::new(run.clone()), cx),
            )
        });
        h_flex()
            .id("toast")
            .ui_container()
            .absolute()
            .left(px(space(SpaceStep::Lg, cx)))
            .right(px(space(SpaceStep::Lg, cx)))
            .bottom(px(space(SpaceStep::Lg, cx)))
            .min_w_0()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .px(px(space(SpaceStep::Lg, cx)))
            .py(px(space(SpaceStep::Sm, cx)))
            .gap(px(space(SpaceStep::Md, cx)))
            .type_body(cx)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .id("toast-message")
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip({
                        let message = message.clone();
                        move |window, cx| Tooltip::new(message.clone()).build(window, cx)
                    })
                    .role(gpui_kit::Role::Status)
                    .aria_label(message.clone())
                    .child(message.clone())
                    .ui_observe_as(Facts {
                        role: gpui_kit::Role::Status,
                        value: Some(message.clone()),
                        ..Facts::button(message)
                    }),
            )
            .children(undo)
            .children(action)
            .child(toast_button(
                "toast-dismiss",
                "Dismiss".into(),
                cx.listener(|this, _, _, cx| {
                    this.toast = None;
                    cx.notify();
                }),
            ))
    }
}

pub(super) fn toast_button(
    id: &'static str,
    label: SharedString,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    controls::control(
        id,
        Facts::button(label.clone()),
        Button::new(SharedString::from(format!("{id}-control")))
            .small()
            .ghost()
            .label(label)
            .on_click(on_click),
    )
}
