use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use kg_core::Updater;
use kg_domain::UpdateState;
use kg_plugins::StatusItem;

use super::log;
use super::plugins::plugin_item;
use super::{DOT, HEIGHT, Kagan};
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
        let install_part = self.activity.install.clone().map(|install| {
            div()
                .id("activity-install")
                .ui_container()
                .flex_shrink_1()
                .max_w(px(240.))
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(install)
        });
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
            .children(install_part)
            .children(plugins.into_iter().map(|item| plugin_item(item, cx)))
            .child(log::toggle(open, &self.activity.toggle_focus, cx))
    }
}

fn needs_part(needs: usize, cx: &mut Context<Kagan>) -> Option<AnyElement> {
    (needs > 0).then(|| {
        div()
            .id("activity-needs")
            .flex_none()
            .type_small(cx)
            .text_color(Tone::Needs.color(cx))
            .on_click(cx.listener(|_, _, window, cx| {
                cx.stop_propagation();
                window
                    .dispatch_action(Box::new(crate::kagan::navigation::NextThreadNeedingYou), cx);
            }))
            .ui_observe_as(Facts {
                role: Role::Button,
                name: "Needs you".into(),
                value: Some(needs.to_string().into()),
                disabled: false,
                selected: None,
                focus: None,
                hint: Some("Go to the next thread that needs you".into()),
            })
            .child(format!("{needs} needs you"))
            .into_any_element()
    })
}

fn update_part(cx: &mut Context<Kagan>) -> Option<AnyElement> {
    let available = {
        let updater = Updater::global(cx);
        let updater = updater.read(cx);
        match updater.state() {
            UpdateState::Available { version, .. } => {
                crate::kagan::update::hidden(cx).as_deref() != Some(version.as_str())
            }
            _ => false,
        }
    };
    available.then(|| {
        div()
            .id("activity-update")
            .ui_container()
            .flex_none()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child("Update available")
            .into_any_element()
    })
}
