use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, Styled as _, div, px,
};
use kg_core::Updater;
use kg_domain::UpdateState;
use kg_plugins::StatusItem;

use super::log;
use super::plugins::plugin_item;
use super::{DOT, HEIGHT, Kagan};

const INSTALL_MAX: f32 = 240.;
use crate::kagan::controls::control;
use crate::kagan::navigation::NextThreadNeedingYou;
use crate::kagan::update;
use crate::theme::{Tone, TypeScale as _};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl Kagan {
    pub(super) fn strip_summary(
        &mut self,
        summary: SharedString,
        plugins: Vec<StatusItem>,
        working: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let needs = self.needs_you(cx);
        let open = self.activity.open();
        let needs_focus = self.activity.focus("needs", cx);
        let update_focus = self.activity.focus("update", cx);
        let install_focus = self.activity.focus("install", cx);
        let toggle_focus = self.activity.focus("toggle", cx);
        let plugins: Vec<(StatusItem, FocusHandle)> = plugins
            .into_iter()
            .map(|item| {
                let focus = self.activity.focus(&format!("plugin-{}", item.id.0), cx);
                (item, focus)
            })
            .collect();
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
            .child(
                h_flex()
                    .flex_shrink_1()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap_3()
                    .children(needs_part(needs, &needs_focus, cx))
                    .children(update_part(&update_focus, cx))
                    .children(self.install_part(&install_focus, cx))
                    .children(
                        plugins
                            .into_iter()
                            .map(|(item, focus)| plugin_item(item, &focus, cx)),
                    ),
            )
            .child(log::toggle(open, &toggle_focus, cx))
    }

    fn install_part(&self, focus: &FocusHandle, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                    .max_w(px(INSTALL_MAX))
                    .tooltip(install.clone())
                    .label(install)
                    .text_color(cx.theme().muted_foreground)
                    .track_focus(focus)
                    .on_click(cx.listener(|this, _, window, cx| this.manage_plugins(window, cx))),
            )
            .into_any_element(),
        )
    }
}

fn needs_part(needs: usize, focus: &FocusHandle, cx: &Context<Kagan>) -> Option<AnyElement> {
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
                .track_focus(focus)
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(NextThreadNeedingYou), cx);
                }),
        )
        .into_any_element()
    })
}

fn update_part(focus: &FocusHandle, cx: &Context<Kagan>) -> Option<AnyElement> {
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
                .track_focus(focus)
                .on_click(|_, window, cx| update::primary(window, cx)),
        )
        .into_any_element()
    })
}
