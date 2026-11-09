use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::h_flex;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Role, Styled as _,
    div, px,
};
use kg_plugins::{StatusItem, StatusTone};

use super::PLUGIN_ITEM_MAX;
use crate::theme::{Tone, TypeScale as _};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn plugin_item(item: StatusItem, cx: &App) -> impl IntoElement + use<> {
    let color = match item.tone {
        StatusTone::Quiet => cx.theme().muted_foreground,
        StatusTone::Busy => Tone::Busy.color(cx),
        StatusTone::Needs => Tone::Needs.color(cx),
        StatusTone::Failed => Tone::Failed.color(cx),
    };
    h_flex()
        .id(ElementId::Name(
            format!("activity-plugin-{}", item.id.0).into(),
        ))
        .ui_observe_as(Facts {
            role: Role::Status,
            name: item.title.clone().into(),
            value: Some(item.text.clone().into()),
            focus: None,
            hint: None,
            disabled: false,
            selected: None,
        })
        .flex_shrink_1()
        .max_w(px(PLUGIN_ITEM_MAX))
        .min_w_0()
        .items_center()
        .gap_1()
        .child(
            div()
                .min_w_0()
                .max_w(px(PLUGIN_ITEM_MAX / 2.))
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(item.title),
        )
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(color)
                .child(item.text),
        )
}
