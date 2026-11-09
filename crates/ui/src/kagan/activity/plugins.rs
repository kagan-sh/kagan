use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use kg_plugins::{RunCommand, StatusItem, StatusTone};

use super::PLUGIN_ITEM_MAX;
use crate::kagan::controls::control;
use crate::theme::{Tone, TypeScale as _};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn plugin_item(item: StatusItem, cx: &App) -> AnyElement {
    let color = match item.tone {
        StatusTone::Quiet => cx.theme().muted_foreground,
        StatusTone::Busy => Tone::Busy.color(cx),
        StatusTone::Needs => Tone::Needs.color(cx),
        StatusTone::Failed => Tone::Failed.color(cx),
    };
    let body = h_flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .min_w_0()
                .max_w(px(PLUGIN_ITEM_MAX / 2.))
                .text_ellipsis()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(item.title.clone()),
        )
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .type_small(cx)
                .text_color(color)
                .child(item.text.clone()),
        );
    let id = format!("activity-plugin-{}", item.id.0);
    let Some(command) = item.command.clone() else {
        let line = h_flex()
            .id(ElementId::Name(id.into()))
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
            .child(body);
        return match item.tooltip {
            Some(tip) => line
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .into_any_element(),
            None => line.into_any_element(),
        };
    };
    let plugin = item.id.clone();
    let facts = Facts {
        role: Role::Button,
        name: item.title.clone().into(),
        value: Some(item.text.clone().into()),
        focus: None,
        hint: item.tooltip.clone().map(SharedString::from),
        disabled: false,
        selected: None,
    };
    let button = Button::new(SharedString::from(format!("{id}-button")))
        .small()
        .ghost()
        .child(body)
        .on_click(move |_, window, cx| {
            window.dispatch_action(
                Box::new(RunCommand::new(plugin.clone(), command.clone())),
                cx,
            );
        });
    let button = match item.tooltip {
        Some(tip) => button.tooltip(tip),
        None => button,
    };
    control(ElementId::Name(id.into()), facts, button).into_any_element()
}
