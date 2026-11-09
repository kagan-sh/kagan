use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use kg_domain::PluginId;
use kg_plugins::{PluginHost, RunCommand, StatusItem, StatusTone};

use super::PLUGIN_ITEM_MAX;
use crate::kagan::controls::{self, control};
use crate::theme::{Tone, TypeScale as _};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn plugin_item(item: StatusItem, focus: &FocusHandle, cx: &App) -> AnyElement {
    let color = match item.tone {
        StatusTone::Quiet => cx.theme().muted_foreground,
        StatusTone::Busy => Tone::Busy.color(cx),
        StatusTone::Needs => Tone::Needs.color(cx),
        StatusTone::Failed => Tone::Failed.color(cx),
    };
    let body = h_flex()
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
    let title = command_title(&item.id, &command, cx);
    let run = RunCommand::new(item.id.clone(), command);
    let command_tip = controls::tip(&title, &run, cx);
    let tooltip = match item.tooltip {
        Some(text) => format!("{text} — {command_tip}"),
        None => command_tip,
    };
    let facts = Facts {
        role: Role::Button,
        name: item.title.clone().into(),
        value: Some(item.text.clone().into()),
        focus: None,
        hint: Some(SharedString::from(tooltip.clone())),
        disabled: false,
        selected: None,
    };
    let button = Button::new(SharedString::from(format!("{id}-button")))
        .small()
        .outline()
        .max_w(px(PLUGIN_ITEM_MAX))
        .child(body)
        .track_focus(focus)
        .tooltip(tooltip)
        .on_click(controls::dispatch(run));
    control(ElementId::Name(id.into()), facts, button).into_any_element()
}

// The title the plugin gave the command, so the button can say what a click runs.
fn command_title(plugin: &PluginId, command: &str, cx: &App) -> String {
    PluginHost::try_global(cx)
        .and_then(|host| {
            host.read(cx)
                .commands()
                .into_iter()
                .find(|item| item.plugin == *plugin && item.id == command)
                .map(|item| item.title)
        })
        .unwrap_or_else(|| command.to_owned())
}
