use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::{IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};

use super::ROW;
use crate::kagan::ToggleActivityLog;
use crate::kagan::controls;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn strip_log(rows: Vec<(u64, SharedString)>, cx: &App) -> impl IntoElement + use<> {
    let rows: Vec<AnyElement> = if rows.is_empty() {
        vec![
            div()
                .id("activity-empty")
                .aria_label("Nothing has happened yet.")
                .ui_container()
                .flex_none()
                .h(px(ROW))
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child("Nothing has happened yet.")
                .into_any_element(),
        ]
    } else {
        rows.into_iter()
            .map(|(seq, line)| {
                h_flex()
                    .id(ElementId::Name(format!("activity-row-{seq}").into()))
                    .aria_label(line.clone())
                    .ui_container()
                    .flex_none()
                    .h(px(ROW))
                    .min_w_0()
                    .items_center()
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(div().min_w_0().text_ellipsis().child(line))
                    .into_any_element()
            })
            .collect()
    };
    v_flex()
        .id("activity-log")
        .ui_container()
        .flex_1()
        .min_h_0()
        .px_3()
        .children(rows)
}

pub(super) fn toggle(open: bool, focus: &FocusHandle, cx: &App) -> impl IntoElement + use<> {
    let name: SharedString = "Activity log".into();
    let tip = controls::tip(&name, &ToggleActivityLog, cx);
    h_flex()
        .id("activity-toggle")
        .occlude()
        .ui_observe_as(Facts {
            role: Role::Button,
            name: name.clone(),
            value: Some(if open { "shown" } else { "hidden" }.into()),
            disabled: false,
            selected: None,
            focus: Some(focus.clone()),
            hint: None,
        })
        .flex_none()
        .child(
            Button::new("activity-toggle-button")
                .small()
                .outline()
                .icon(if open {
                    IconName::PanelBottomOpen
                } else {
                    IconName::PanelBottom
                })
                .label("Activity log")
                .accessibility_label(name)
                .tooltip(tip)
                .track_focus(focus)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleActivityLog), cx)),
        )
}
