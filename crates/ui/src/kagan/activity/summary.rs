use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, Styled as _, div, px,
};
use kg_core::Updater;
use kg_domain::UpdateState;
use kg_plugins::StatusItem;

use super::log;
use super::plugins::plugin_item;
use super::{DOT, HEIGHT, Kagan};
use crate::kagan::controls::control;
use crate::kagan::navigation::NextThreadNeedingYou;
use crate::kagan::update;
use crate::theme::{Tone, TypeScale as _};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl Kagan {
    pub(super) fn strip_summary(
        &self,
        summary: SharedString,
        plugins: Vec<StatusItem>,
        working: usize,
        needs: usize,
        open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let dot = div()
            .flex_none()
            .size(px(DOT))
            .rounded_full()
            .bg(if working > 0 {
                Tone::Busy.color(cx)
            } else {
                cx.theme().muted_foreground
            });
        let head = h_flex()
            .id("activity-summary")
            .ui_observe_as(Facts {
                role: Role::Status,
                name: "Activity".into(),
                value: Some(summary.clone()),
                disabled: false,
                selected: None,
                focus: None,
                hint: None,
            })
            .flex_1()
            .min_w(px(120.))
            .items_center()
            .gap_2()
            .child(dot)
            .child(div().min_w_0().text_ellipsis().child(summary));
        h_flex()
            .h(px(HEIGHT))
            .flex_none()
            .items_center()
            .gap_3()
            .px_3()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child(head)
            .children(needs_part(needs, cx))
            .children(update_part(cx))
            .children(self.install_part(cx))
            .children(plugins.into_iter().map(|item| plugin_item(item, cx)))
            .child(log::toggle(open, &self.activity.toggle_focus, cx))
    }

    fn install_part(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let install: SharedString = self.activity.install.clone()?;
        Some(
            control(
                "activity-install",
                Facts {
                    role: Role::Button,
                    name: install.clone(),
                    value: None,
                    disabled: false,
                    selected: None,
                    focus: None,
                    hint: Some("Show the installed plugins in Settings".into()),
                },
                Button::new("activity-install-button")
                    .small()
                    .ghost()
                    .label(install)
                    .text_color(cx.theme().muted_foreground)
                    .on_click(cx.listener(|this, _, window, cx| this.manage_plugins(window, cx))),
            )
            .into_any_element(),
        )
    }
}

fn needs_part(needs: usize, cx: &Context<Kagan>) -> Option<AnyElement> {
    (needs > 0).then(|| {
        control(
            "activity-needs",
            Facts {
                role: Role::Button,
                name: "Needs you".into(),
                value: Some(needs.to_string().into()),
                disabled: false,
                selected: None,
                focus: None,
                hint: Some("Go to the next thread that needs you".into()),
            },
            Button::new("activity-needs-button")
                .small()
                .ghost()
                .label(format!("{needs} needs you"))
                .text_color(Tone::Needs.color(cx))
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(NextThreadNeedingYou), cx);
                }),
        )
        .into_any_element()
    })
}

fn update_part(cx: &Context<Kagan>) -> Option<AnyElement> {
    let available = {
        let updater = Updater::global(cx);
        let updater = updater.read(cx);
        match updater.state() {
            UpdateState::Available { version, .. } => {
                update::hidden(cx).as_deref() != Some(version.as_str())
            }
            _ => false,
        }
    };
    available.then(|| {
        control(
            "activity-update",
            Facts {
                role: Role::Button,
                name: "Update available".into(),
                value: None,
                disabled: false,
                selected: None,
                focus: None,
                hint: None,
            },
            Button::new("activity-update-button")
                .small()
                .ghost()
                .label("Update available")
                .text_color(cx.theme().muted_foreground)
                .on_click(|_, window, cx| update::primary(window, cx)),
        )
        .into_any_element()
    })
}
