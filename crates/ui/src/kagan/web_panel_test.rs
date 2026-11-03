use gpui_kit::App;
use kg_domain::PluginId;
#[cfg(target_os = "macos")]
use kg_plugins::PluginHost;

#[cfg(target_os = "macos")]
use super::{Held, Pages, find, page_watch, save};

/// Puts a raw IPC body on the page channel (generation is checked by the drain).
#[cfg(target_os = "macos")]
pub(crate) fn inject(plugin: &PluginId, panel: &str, raw: &str, cx: &App) -> bool {
    let Some(page) = cx.try_global::<Pages>().and_then(|pages| {
        pages
            .pages
            .iter()
            .find(|page| page.plugin == *plugin && page.panel == panel)
    }) else {
        return false;
    };
    page.ipc.try_send(raw.to_owned()).is_ok()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn inject(plugin: &PluginId, panel: &str, raw: &str, cx: &App) -> bool {
    let _ = (plugin, panel, raw, cx);
    false
}

/// Queues a page job with the open page's lifetime token, as the IPC drain does after checks.
#[cfg(target_os = "macos")]
pub(crate) fn queue_page(plugin: &PluginId, panel: &str, raw: &str, cx: &mut App) -> bool {
    let Some((alive, plugin, panel)) = cx.try_global::<Pages>().and_then(|pages| {
        pages
            .pages
            .iter()
            .find(|page| page.plugin == *plugin && page.panel == panel)
            .map(|page| {
                (
                    std::sync::Arc::downgrade(&page.life),
                    page.plugin.clone(),
                    page.panel.clone(),
                )
            })
    }) else {
        return false;
    };
    PluginHost::global(cx)
        .update(cx, |host, cx| {
            host.deliver_page(&plugin, &panel, raw, alive, cx)
        })
        .is_ok()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn queue_page(plugin: &PluginId, panel: &str, raw: &str, cx: &mut App) -> bool {
    let _ = (plugin, panel, raw, cx);
    false
}

#[cfg(target_os = "macos")]
pub(crate) fn mark_stale(slot: &str, cx: &mut App) -> bool {
    let Some(index) = find(cx, slot) else {
        return false;
    };
    cx.default_global::<Pages>().pages[index].generation = u64::MAX;
    true
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn mark_stale(slot: &str, cx: &mut App) -> bool {
    let _ = (slot, cx);
    false
}

#[cfg(target_os = "macos")]
pub(crate) fn testing_page(plugin: &PluginId, panel: &str, slot: &str, shown: bool, cx: &mut App) {
    super::forget(slot, cx);
    let generation = PluginHost::try_global(cx)
        .and_then(|host| host.read(cx).generation(plugin))
        .unwrap_or(1);
    let life = std::sync::Arc::new(());
    let alive = std::sync::Arc::downgrade(&life);
    let (sender, receiver) = async_channel::bounded(32);
    let ipc = sender.clone();
    let drain = page_watch::watch(
        receiver,
        plugin.clone(),
        panel.to_owned(),
        generation,
        alive,
        cx,
    );
    save(
        cx,
        Held {
            slot: slot.to_owned(),
            plugin: plugin.clone(),
            panel: panel.to_owned(),
            file: "ui/index.html".to_owned(),
            generation,
            note: None,
            view: None,
            theme: crate::web::ThemeCss::new(String::new()),
            _drain: drain,
            ipc,
            life,
            shown: Some(shown),
        },
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn testing_page(plugin: &PluginId, panel: &str, slot: &str, shown: bool, cx: &mut App) {
    let _ = (plugin, panel, slot, shown, cx);
}
