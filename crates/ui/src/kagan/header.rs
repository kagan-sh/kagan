use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, SharedString, Styled as _, px};
use kg_domain::PluginId;
use kg_plugins::{CommandWhen, PluginHost, RunCommand};

use super::shown::{control, dispatch};
use crate::ui_registry::Facts;

pub(super) fn buttons(cx: &App) -> Vec<AnyElement> {
    commands(cx)
        .into_iter()
        .map(|(plugin, id, title)| {
            let run = RunCommand::new(plugin.clone(), id.clone());
            let tooltip = tip(&title, &plugin, &id, cx);
            let key = id_for(&plugin, &id);
            control(
                key.clone(),
                Facts {
                    hint: Some(SharedString::from(tooltip.clone())),
                    ..Facts::button(title.clone())
                },
                Button::new(key)
                    .small()
                    .ghost()
                    .label(title)
                    .tooltip(tooltip)
                    .max_w(px(120.))
                    .on_click(dispatch(run)),
            )
            .into_any_element()
        })
        .collect()
}

pub(crate) fn id_for(plugin: &PluginId, id: &str) -> ElementId {
    // Plugin ids are reverse-DNS, so U+001F cannot appear in one. The command id is the suffix.
    ElementId::Name(format!("header\u{1F}{}\u{1F}{id}", plugin.0).into())
}

fn commands(cx: &App) -> Vec<(PluginId, String, String)> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    host.read(cx)
        .commands()
        .into_iter()
        .filter(|command| command.header() && command.when() == CommandWhen::Thread)
        .map(|command| {
            (
                command.plugin().clone(),
                command.id().to_owned(),
                command.title().to_owned(),
            )
        })
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
