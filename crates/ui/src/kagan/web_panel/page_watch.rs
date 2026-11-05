use std::sync::Weak;

use gpui_kit::App;
use kg_domain::PluginId;
use kg_plugins::PluginHost;

use super::native::Pages;

pub(super) fn watch(
    receiver: async_channel::Receiver<String>,
    plugin: PluginId,
    panel: String,
    generation: u64,
    alive: Weak<()>,
    cx: &mut App,
) -> gpui_kit::Task<()> {
    cx.spawn(async move |cx| {
        while let Ok(raw) = receiver.recv().await {
            let plugin = plugin.clone();
            let panel = panel.clone();
            let alive = alive.clone();
            cx.update(|cx| {
                if !live(&plugin, &panel, generation, cx) {
                    return;
                }
                if let Some(host) = PluginHost::try_global(cx) {
                    host.update(cx, |host, cx| {
                        if let Err(error) = host.deliver_page(&plugin, &panel, &raw, alive, cx) {
                            eprintln!("kg: the page message was not delivered: {error}");
                        }
                    });
                }
            });
        }
    })
}

pub(super) fn drop_stale(cx: &mut App) {
    let stale: Vec<usize> = {
        let found: Vec<_> = cx
            .default_global::<Pages>()
            .pages
            .iter()
            .enumerate()
            .map(|(index, page)| (index, page.plugin.clone(), page.generation))
            .collect();
        found
            .into_iter()
            .filter_map(|(index, plugin, generation)| {
                let current =
                    PluginHost::try_global(cx).and_then(|host| host.read(cx).generation(&plugin));
                (current != Some(generation)).then_some(index)
            })
            .collect()
    };
    for index in stale.into_iter().rev() {
        cx.default_global::<Pages>().pages.remove(index);
    }
}

fn live(plugin: &PluginId, panel: &str, generation: u64, cx: &App) -> bool {
    let Some(host) = PluginHost::try_global(cx) else {
        return false;
    };
    if host.read(cx).generation(plugin) != Some(generation) {
        return false;
    }
    cx.try_global::<Pages>().is_some_and(|pages| {
        pages.pages.iter().any(|page| {
            page.plugin == *plugin && page.panel == panel && page.generation == generation
        })
    })
}
