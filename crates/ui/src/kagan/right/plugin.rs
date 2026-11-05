use crate::theme::{SpaceStep, TypeScale as _, radius_sm, space};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, Styled as _, Window, div, px,
};
use kg_domain::PluginOp;

use super::ToggleDetails;
use crate::kagan::Kagan;
use crate::kagan::controls::{answer, send};
use crate::kagan::plugin_actions::{ReloadPlugin, ShowPluginLogs, TogglePluginEnabled};
use crate::kagan::surface::Slot;
use crate::kagan::tab_stop;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
fn recovery_row(id: &str, details: bool, focus: &FocusHandle, cx: &App) -> AnyElement {
    h_flex()
        .flex_wrap()
        .gap(px(space(SpaceStep::Xs, cx)))
        .child(send(format!("{id}-reload"), "Reload", focus, ReloadPlugin))
        .child(send(format!("{id}-logs"), "Logs", focus, ShowPluginLogs))
        .child(send(
            format!("{id}-disable"),
            "Disable",
            focus,
            TogglePluginEnabled,
        ))
        .child(send(
            format!("{id}-details"),
            if details { "Hide details" } else { "Details" },
            focus,
            ToggleDetails {
                panel: id.to_owned(),
            },
        ))
        .into_any_element()
}

pub(in crate::kagan) fn plugin_body(
    slot: &Slot,
    fill: bool,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let error = slot.error.clone().or_else(|| {
        slot.view
            .as_ref()?
            .read(cx)
            .build_error()
            .map(ToString::to_string)
    });
    match (error, &slot.view) {
        (Some(error), _) => failed_plugin(slot, &error, window, cx),
        (None, Some(view)) if fill => div().size_full().child(view.clone()).into_any_element(),
        (None, Some(view)) => div().child(view.clone()).into_any_element(),
        (None, None) => crate::kagan::web_panel::body(slot, window, cx),
    }
}

fn failed_plugin(
    slot: &Slot,
    error: &str,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let (id, title, plugin, details) = (slot.id.clone(), &slot.title, &slot.plugin, slot.details);
    let reason = error.lines().next().unwrap_or_default().to_owned();
    let focus = tab_stop(
        window,
        cx,
        ElementId::Name(format!("{id}-failed-focus").into()),
    );
    // Only the card text carries the row keys. The buttons sit beside it and send their actions
    // from where they have focus, so Space on a button never reaches the row.
    let card = div()
        .id(ElementId::Name(format!("{id}-failed").into()))
        .ui_observe_as(Facts {
            role: Role::ListBoxOption,
            focus: Some(focus.clone()),
            ..Facts::button(title.to_owned())
        })
        .track_focus(&focus)
        .tab_stop(true)
        .key_context("PluginRow")
        .rounded(px(radius_sm(cx)))
        .focus(|card| card.border_1().border_color(cx.theme().ring))
        .flex_col()
        .gap(px(space(SpaceStep::Xs, cx)))
        .child(div().type_body(cx).child(format!("Could not show {title}")))
        .child(
            div()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(reason),
        );
    let body_focus = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-failed-body-focus").into()),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone();
    let body = v_flex()
        .id(ElementId::Name(format!("{id}-failed-body").into()))
        .track_focus(&body_focus)
        .gap(px(space(SpaceStep::Sm, cx)))
        .child(card);
    let body = answer::<ReloadPlugin, _>(body, plugin, cx, |this, id, cx| {
        let op = PluginOp::Reload {
            id: id.clone(),
            thread_id: None,
        };
        this.plugin_op(op, "reload the plugin", |_, _, _| {}, cx);
    });
    let logs_id = plugin.clone();
    let body = body.on_action(cx.listener(move |this, _: &ShowPluginLogs, window, cx| {
        this.open_plugin_logs(&logs_id, window, cx);
    }));
    let body = answer::<TogglePluginEnabled, _>(body, plugin, cx, |this, id, cx| {
        this.toggle_plugin_enabled(id, cx);
    });
    let mut body = body.child(recovery_row(&id, details, &body_focus, cx));
    if details {
        body = body.child(
            div()
                .id(ElementId::Name(format!("{id}-error").into()))
                .ui_observe_as(Facts {
                    role: Role::Label,
                    value: Some(error.to_owned().into()),
                    ..Facts::button("Error")
                })
                .type_small(cx)
                .child(error.to_owned()),
        );
    }
    body.into_any_element()
}
