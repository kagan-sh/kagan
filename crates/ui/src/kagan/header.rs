use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, SharedString, Styled as _, px};
use kg_domain::PluginId;
use kg_plugins::{CommandContribution, CommandWhen, PluginHost, RunCommand};

use super::controls::{control, dispatch};
use crate::ui_registry::Facts;

pub(super) fn buttons(cx: &App) -> Vec<AnyElement> {
    placed(cx, |command| command.header)
        .into_iter()
        .map(|command| {
            let run = RunCommand::new(command.plugin.clone(), command.id.clone());
            let tooltip = tip(&command.title, &command.plugin, &command.id, cx);
            let key = id_for(&command.plugin, &command.id);
            control(
                key.clone(),
                Facts {
                    hint: Some(SharedString::from(tooltip.clone())),
                    ..Facts::button(command.title.clone())
                },
                Button::new(key)
                    .small()
                    .ghost()
                    .label(command.title)
                    .tooltip(tooltip)
                    .max_w(px(120.))
                    .on_click(dispatch(run)),
            )
            .into_any_element()
        })
        .collect()
}

/// The commands a plugin placed in the shown thread's actions menu.
pub(super) fn menu(cx: &App) -> Vec<CommandContribution> {
    placed(cx, |command| command.menu)
}

pub(crate) fn id_for(plugin: &PluginId, id: &str) -> ElementId {
    ElementId::Name(format!("header/{}", encoded(plugin, id)).into())
}

pub(crate) fn menu_id_for(plugin: &PluginId, id: &str) -> ElementId {
    ElementId::Name(format!("thread-menu/{}", encoded(plugin, id)).into())
}

/// `<plugin>/<command>`, with `%` escapes so a command name cannot merge into the plugin id.
fn encoded(plugin: &PluginId, id: &str) -> String {
    let mut command = String::new();
    for byte in id.bytes() {
        if matches!(byte, b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_' | b'.') {
            command.push(byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(command, "%{byte:02X}");
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

fn tip(title: &str, plugin: &PluginId, id: &str, cx: &App) -> String {
    let keys = cx
        .key_bindings()
        .borrow()
        .bindings()
        .find_map(|binding| {
            binding
                .action()
                .as_any()
                .downcast_ref::<RunCommand>()
                .and_then(|run| {
                    (run.plugin() == plugin && run.command() == id).then(|| {
                        binding
                            .keystrokes()
                            .iter()
                            .map(|key| Kbd::format(key.inner()))
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                })
        })
        .filter(|keys| !keys.is_empty());
    match keys {
        Some(keys) => format!("{title} ({keys})"),
        None => title.to_owned(),
    }
}
