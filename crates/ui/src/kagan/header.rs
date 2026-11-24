use gpui_kit::component::Placement;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{AnyElement, App, ElementId, IntoElement, SharedString, Styled as _, px};
use kg_domain::PluginId;
use kg_plugins::{CommandContribution, CommandWhen, PluginHost, RunCommand};

use super::controls::{control, dispatch};
use crate::ui_registry::Facts;

/// The prompt row draws this many buttons, however many the plugins declare.
const MOST_PROMPT: usize = 4;

pub(super) fn buttons(cx: &App) -> Vec<AnyElement> {
    placed(cx, |command| command.header)
        .into_iter()
        .map(|command| {
            let id = id_for(&command.plugin, &command.id);
            row(command, id, None, cx)
        })
        .collect()
}

/// The plugin commands that sit beside Send, before it.
pub(super) fn prompt(cx: &App) -> Vec<AnyElement> {
    placed(cx, |command| command.prompt)
        .into_iter()
        .take(MOST_PROMPT)
        .map(|command| {
            let id = prompt_id_for(&command.plugin, &command.id);
            // The bar is under the draft, so its tooltip opens below it, never over the text.
            row(command, id, Some(Placement::Bottom), cx)
        })
        .collect()
}

pub(super) fn menu(cx: &App) -> Vec<CommandContribution> {
    placed(cx, |command| command.menu)
}

pub(crate) fn id_for(plugin: &PluginId, id: &str) -> ElementId {
    ElementId::Name(format!("header/{}", encoded(plugin, id)).into())
}

pub(crate) fn menu_id_for(plugin: &PluginId, id: &str) -> ElementId {
    ElementId::Name(format!("thread-menu/{}", encoded(plugin, id)).into())
}

pub(crate) fn prompt_id_for(plugin: &PluginId, id: &str) -> ElementId {
    ElementId::Name(format!("prompt/{}", encoded(plugin, id)).into())
}

// One ghost button, with no icon, sized like the buttons beside it.
fn row(
    command: CommandContribution,
    id: ElementId,
    placement: Option<Placement>,
    cx: &App,
) -> AnyElement {
    let run = RunCommand::new(command.plugin.clone(), command.id.clone());
    let tip = command.description.as_deref().unwrap_or(&command.title);
    let tooltip = super::controls::tip(tip, &run, cx);
    let button = Button::new(id.clone())
        .small()
        .ghost()
        .label(command.title.clone())
        .tooltip(tooltip.clone())
        .max_w(px(120.));
    let button = match placement {
        Some(side) => button.tooltip_placement(side),
        None => button,
    };
    control(
        id.clone(),
        Facts {
            hint: Some(SharedString::from(tooltip.clone())),
            ..Facts::button(command.title.clone())
        },
        button.on_click(dispatch(run)),
    )
    .into_any_element()
}

// Command ids can hold any byte; %-escaping keeps the "/" separator unambiguous.
fn encoded(plugin: &PluginId, id: &str) -> String {
    let mut command = String::new();
    for byte in id.bytes() {
        if matches!(byte, b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_' | b'.') {
            command.push(byte as char);
        } else {
            use std::fmt::Write as _;
            // Writing to a String cannot fail.
            let _ = write!(command, "%{byte:02X}"); // Writing to a String cannot fail.
        }
    }
    // Plugin ids are reverse-DNS, so they contain neither '/' nor '%'.
    format!("{}/{command}", plugin.0)
}

fn placed(cx: &App, place: fn(&CommandContribution) -> bool) -> Vec<CommandContribution> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    host.read(cx)
        .commands()
        .into_iter()
        .filter(|command| command.when == CommandWhen::Thread && place(command))
        .collect()
}
