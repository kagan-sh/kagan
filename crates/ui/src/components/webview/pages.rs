use gpui_kit::App;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use gpui_kit::{Entity, WeakEntity};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use gpui_shell::ScriptView;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use super::Held;
use super::Pages;

pub(super) fn write(cx: &mut App, index: usize, url: &str, note: Option<String>) {
    let slot = &mut cx.default_global::<Pages>().pages[index];
    url.clone_into(&mut slot.url);
    slot.note = note;
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(super) fn write_page(
    cx: &mut App,
    index: usize,
    url: &str,
    page: Entity<crate::web_page::WebPage>,
) {
    let slot = &mut cx.default_global::<Pages>().pages[index];
    url.clone_into(&mut slot.url);
    slot.note = None;
    slot.page = Some(page);
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(super) fn held(
    owner: WeakEntity<ScriptView>,
    id: &str,
    url: &str,
    hosts: &[String],
    store: Option<[u8; 16]>,
    page: Option<Entity<crate::web_page::WebPage>>,
    note: Option<String>,
) -> Held {
    Held {
        owner,
        id: id.to_owned(),
        url: url.to_owned(),
        hosts: hosts.to_vec(),
        store,
        note,
        page,
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(super) fn save(cx: &mut App, index: Option<usize>, slot: Held) {
    let pages = &mut cx.default_global::<Pages>().pages;
    if let Some(index) = index {
        pages[index] = slot;
    } else {
        pages.push(slot);
    }
}

pub(super) fn find(cx: &App, owner: gpui_kit::EntityId, id: &str) -> Option<usize> {
    cx.try_global::<Pages>()?
        .pages
        .iter()
        .position(|page| page.owner.entity_id() == owner && page.id == id)
}

// Drops native pages that hold these stores so `WebKit` releases them before removal.
pub(crate) fn drop_stores(stores: &[[u8; 16]], cx: &mut App) {
    #[cfg(target_os = "macos")]
    if !stores.is_empty() && cx.has_global::<Pages>() {
        cx.default_global::<Pages>().pages.retain(|page| {
            page.store
                .is_none_or(|id| !stores.iter().any(|store| store == &id))
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (stores, cx);
}

/// Hides a page whose panel is off screen, and drops one whose view is gone.
pub(crate) fn sync_pages(shown: &[gpui_kit::EntityId], cx: &mut App) {
    if !cx.has_global::<Pages>() {
        return;
    }
    let plan: Vec<(usize, bool, bool)> = {
        let pages = &cx.default_global::<Pages>().pages;
        pages
            .iter()
            .enumerate()
            .map(|(index, page)| {
                let dead = page.owner.upgrade().is_none();
                let on = !dead && page.note.is_none() && shown.contains(&page.owner.entity_id());
                (index, dead, on)
            })
            .collect()
    };
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let views: Vec<_> = {
            let pages = &cx.default_global::<Pages>().pages;
            plan.iter()
                .filter_map(|(index, dead, on)| {
                    pages[*index].page.clone().map(|page| (page, *dead, *on))
                })
                .collect()
        };
        for (page, dead, on) in views {
            let want = on && !dead;
            if page.read(cx).visible(cx) != want {
                page.update(cx, |page, cx| {
                    if want {
                        page.show(cx);
                    } else {
                        page.hide(cx);
                    }
                });
            }
        }
    }
    let pages = &mut cx.default_global::<Pages>().pages;
    for index in plan
        .into_iter()
        .rev()
        .filter_map(|(index, dead, _)| dead.then_some(index))
    {
        pages.remove(index);
    }
}

#[cfg(test)]
pub(crate) fn native_shown(cx: &App, id: &str) -> Option<bool> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        cx.try_global::<Pages>().and_then(|pages| {
            pages
                .pages
                .iter()
                .find(|page| page.id == id)
                .and_then(|page| page.page.as_ref().map(|view| view.read(cx).visible(cx)))
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (cx, id);
        None
    }
}
