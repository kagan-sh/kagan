use std::sync::OnceLock;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::{Context, IntoElement as _};
use kg_plugins::PluginHost;
use serde::Deserialize;

use super::Kagan;
use super::row::{Group, Row, group as row_group, row};
use crate::kagan::controls::control;
use crate::ui_registry::Facts;

#[derive(Deserialize)]
pub(super) struct Suggested {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) source: String,
}

#[derive(Deserialize)]
struct Shipped {
    plugins: Vec<Suggested>,
}

pub(super) fn list() -> &'static [Suggested] {
    static LIST: OnceLock<Vec<Suggested>> = OnceLock::new();
    LIST.get_or_init(|| {
        serde_json::from_str::<Shipped>(include_str!("suggested_plugins.json"))
            .map(|shipped| shipped.plugins)
            .unwrap_or_default()
    })
}

// Shown only while no plugin index is added, and only for plugins not installed yet.
pub(super) fn group(cx: &mut Context<Kagan>) -> Option<Group> {
    let host = PluginHost::try_global(cx)?;
    let host = host.read(cx);
    if !host.indexes().ok()?.is_empty() {
        return None;
    }
    let installed: Vec<String> = host
        .status()
        .iter()
        .map(|status| status.id.to_string())
        .collect();
    let rows: Vec<Row> = list()
        .iter()
        .filter(|suggested| !installed.contains(&suggested.id))
        .map(|suggested| {
            let source = suggested.source.clone();
            let install = control(
                format!("suggested-install-{}", suggested.id),
                Facts::button("Install"),
                Button::new(format!("suggested-install-button-{}", suggested.id))
                    .small()
                    .outline()
                    .label("Install")
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.install_source(source.clone(), cx)),
                    ),
            );
            row(
                &suggested.id,
                &suggested.name,
                &suggested.description,
                install.into_any_element(),
            )
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    Some(row_group("suggested", "Suggested plugins", rows))
}

#[cfg(test)]
mod tests {
    use super::list;

    #[test]
    fn the_shipped_list_reads_and_pins_every_plugin() {
        let plugins = list();
        assert_eq!(plugins.len(), 5);
        for plugin in plugins {
            assert!(plugin.id.starts_with("dev.kagan."), "{}", plugin.id);
            assert!(
                plugin
                    .source
                    .starts_with("git:https://github.com/kagan-sh/")
                    && plugin
                        .source
                        .rsplit_once('@')
                        .is_some_and(|(_, pin)| !pin.is_empty()),
                "{}",
                plugin.source
            );
        }
    }
}
