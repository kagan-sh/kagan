use gpui_kit::{
    AnyElement, App, AppContext as _, Entity, IntoElement as _, ParentElement as _, Styled as _,
    Window, div,
};
use kg_domain::PluginId;
use kg_plugins::{PluginHost, SCHEME};

use super::{page_watch, sentence};
use crate::kagan::surface::Slot;
use crate::kagan::{Kagan, right, screen};
use crate::web::{self, BRIDGE};

#[derive(Default)]
pub(super) struct Pages {
    pub(super) pages: Vec<Held>,
}

impl gpui_kit::Global for Pages {}

pub(super) struct Held {
    pub(super) slot: String,
    pub(super) plugin: PluginId,
    pub(super) panel: String,
    pub(super) file: String,
    pub(super) generation: u64,
    pub(super) note: Option<String>,
    pub(super) view: Option<Entity<gpui_webview::WebView>>,
    pub(super) theme: web::ThemeCss,
    pub(super) _drain: gpui_kit::Task<()>,
    pub(super) ipc: async_channel::Sender<String>,
    pub(super) life: std::sync::Arc<()>,
    pub(super) shown: Option<bool>,
}

struct Shown {
    note: Option<String>,
    page: Option<Entity<gpui_webview::WebView>>,
}

struct Built {
    view: Entity<gpui_webview::WebView>,
    theme: web::ThemeCss,
    drain: gpui_kit::Task<()>,
    ipc: async_channel::Sender<String>,
    life: std::sync::Arc<()>,
    note: Option<String>,
}

pub(super) fn body(slot: &Slot, file: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    let shown = place(slot, file, window, cx);
    let note = shown.note.map(|note| sentence(&slot.id, &note, cx));
    let page = shown.page.map(gpui_kit::IntoElement::into_any_element);
    match (note, page) {
        (Some(note), None) => note,
        (Some(note), Some(page)) => div()
            .size_full()
            .flex()
            .flex_col()
            .child(note)
            .child(div().min_h_0().flex_1().child(page))
            .into_any_element(),
        (None, Some(page)) => page,
        (None, None) => div().into_any_element(),
    }
}

pub(super) fn deliver(
    plugin: &PluginId,
    panel: &str,
    message: &str,
    cx: &mut App,
) -> Result<(), String> {
    let Some(script) = web::page_event(message) else {
        return Err("a page message must be JSON".into());
    };
    let Some((view, shown)) = cx.try_global::<Pages>().and_then(|pages| {
        pages
            .pages
            .iter()
            .find(|page| page.plugin == *plugin && page.panel == panel)
            .map(|page| {
                let shown = page.shown.unwrap_or_else(|| {
                    page.view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).visible())
                });
                (page.view.clone(), shown)
            })
    }) else {
        return Err("the web panel is not on screen".into());
    };
    if !shown {
        return Err("the web panel is not on screen".into());
    }
    let Some(view) = view else {
        return Err("the page could not take the message: it is not open".into());
    };
    view.update(cx, |view, _| {
        view.evaluate_script(&script)
            .map_err(|error| format!("the page could not take the message: {error}"))
    })
}

pub(super) fn clear(cx: &mut App) {
    if cx.has_global::<Pages>() {
        cx.default_global::<Pages>().pages.clear();
    }
}

pub(super) fn drop_plugin(plugin: &PluginId, cx: &mut App) {
    if !cx.has_global::<Pages>() {
        return;
    }
    let indexes: Vec<usize> = cx
        .default_global::<Pages>()
        .pages
        .iter()
        .enumerate()
        .filter_map(|(index, page)| (page.plugin == *plugin).then_some(index))
        .collect();
    for index in indexes.into_iter().rev() {
        let page = cx.default_global::<Pages>().pages.remove(index);
        page.ipc.close();
        drop(page.life);
    }
}

pub(super) fn forget(slot: &str, cx: &mut App) {
    if let Some(index) = find(cx, slot) {
        let page = cx.default_global::<Pages>().pages.remove(index);
        page.ipc.close();
        // Dropping the life token expires any page job still waiting in `next()`.
        drop(page.life);
    }
}

pub(super) fn sync(kagan: &Kagan, cx: &mut App) {
    if !cx.has_global::<Pages>() {
        return;
    }
    page_watch::drop_stale(cx);
    let open = listed(kagan, cx);
    let css = web::theme_css(cx);
    let plan: Vec<(usize, bool, bool)> = {
        let pages = &cx.default_global::<Pages>().pages;
        pages
            .iter()
            .enumerate()
            .map(|(index, page)| {
                let found = open.iter().find(|(id, _)| id == &page.slot);
                (
                    index,
                    found.is_none(),
                    found.is_some_and(|(_, drawn)| *drawn),
                )
            })
            .collect()
    };
    let views: Vec<_> = {
        let pages = &cx.default_global::<Pages>().pages;
        plan.iter()
            .filter_map(|(index, dead, drawn)| {
                pages[*index].view.clone().map(|view| (view, *dead, *drawn))
            })
            .collect()
    };
    for (view, dead, drawn) in views {
        let want = drawn && !dead;
        if view.read(cx).visible() != want {
            view.update(cx, |view, _| {
                if want {
                    view.show();
                } else {
                    view.hide();
                }
            });
        }
    }
    for index in plan
        .iter()
        .filter_map(|(index, dead, drawn)| drawn.then_some(*index).filter(|_| !*dead))
    {
        let changed = cx.default_global::<Pages>().pages[index].theme.get() != css;
        if changed {
            cx.default_global::<Pages>().pages[index]
                .theme
                .set(css.clone());
            if let Some(view) = cx.default_global::<Pages>().pages[index].view.clone() {
                paint_theme(&view, cx);
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

// Pushes the live Workbench theme to every open page, even when the window is not drawing.
pub(super) fn repaint(cx: &mut App) {
    if cx.has_global::<Pages>() {
        let css = web::theme_css(cx);
        let views: Vec<_> = cx
            .default_global::<Pages>()
            .pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| page.view.clone().map(|view| (index, view)))
            .collect();
        for (index, view) in views {
            if cx.default_global::<Pages>().pages[index].theme.get() == css {
                continue;
            }
            cx.default_global::<Pages>().pages[index]
                .theme
                .set(css.clone());
            paint_theme(&view, cx);
        }
    }
}

fn place(slot: &Slot, file: &str, window: &mut Window, cx: &mut App) -> Shown {
    let Some(host) = PluginHost::try_global(cx) else {
        return noted("the plugin is not loaded");
    };
    let (root, hosts, generation) = {
        let host = host.read(cx);
        let Some(root) = host.plugin_root(&slot.plugin) else {
            return noted("the plugin is not loaded");
        };
        (
            root,
            host.fetch_hosts(&slot.plugin),
            host.generation(&slot.plugin).unwrap_or(0),
        )
    };
    open_page(slot, file, &root, &hosts, generation, window, cx)
}

fn noted(reason: &str) -> Shown {
    Shown {
        note: Some(reason.to_owned()),
        page: None,
    }
}

fn open_page(
    slot: &Slot,
    file: &str,
    root: &std::path::Path,
    hosts: &[String],
    generation: u64,
    window: &mut Window,
    cx: &mut App,
) -> Shown {
    if let Some(index) = find(cx, &slot.id) {
        let same = {
            let held = &cx.default_global::<Pages>().pages[index];
            held.generation == generation && held.file == file
        };
        if same {
            refresh_theme(cx, index);
            let view = cx.default_global::<Pages>().pages[index].view.clone();
            if let Some(view) = &view {
                reveal(view, cx);
            }
            let note = cx.default_global::<Pages>().pages[index].note.clone();
            return Shown { note, page: view };
        }
        conceal_at(cx, index);
        cx.default_global::<Pages>().pages.remove(index);
    }
    match build(slot, file, root, hosts, generation, window, cx) {
        Ok(built) => {
            let view = built.view.clone();
            let note = built.note.clone();
            save(
                cx,
                Held {
                    slot: slot.id.clone(),
                    plugin: slot.plugin.clone(),
                    panel: slot.contribution.clone(),
                    file: file.to_owned(),
                    generation,
                    note: note.clone(),
                    view: Some(built.view),
                    theme: built.theme,
                    _drain: built.drain,
                    ipc: built.ipc,
                    life: built.life,
                    shown: None,
                },
            );
            Shown {
                note,
                page: Some(view),
            }
        }
        Err(message) => {
            let (ipc, disconnected) = async_channel::bounded(1);
            drop(disconnected);
            save(
                cx,
                Held {
                    slot: slot.id.clone(),
                    plugin: slot.plugin.clone(),
                    panel: slot.contribution.clone(),
                    file: file.to_owned(),
                    generation,
                    note: Some(message.clone()),
                    view: None,
                    theme: web::ThemeCss::new(String::new()),
                    _drain: gpui_kit::Task::ready(()),
                    ipc,
                    life: std::sync::Arc::new(()),
                    shown: None,
                },
            );
            noted(&message)
        }
    }
}

fn build(
    slot: &Slot,
    file: &str,
    root: &std::path::Path,
    hosts: &[String],
    generation: u64,
    window: &mut Window,
    cx: &mut App,
) -> Result<Built, String> {
    let theme = web::ThemeCss::new(web::theme_css(cx));
    let served_theme = theme.clone();
    let root = root.to_path_buf();
    let plugin_id = slot.plugin.0.clone();
    let hosts = hosts.to_vec();
    let url = web::page_url(&plugin_id, file)?;
    let life = std::sync::Arc::new(());
    let alive = std::sync::Arc::downgrade(&life);
    let (sender, receiver) = async_channel::bounded::<String>(32);
    let ipc = sender.clone();
    let plugin = slot.plugin.clone();
    let panel = slot.contribution.clone();
    let navigation = plugin_id.clone();
    let protocol_plugin = plugin_id.clone();
    let wants_store = PluginHost::global(cx)
        .read(cx)
        .web_store(&slot.plugin, &slot.contribution);
    let (store, store_note) = if !wants_store {
        (None, None)
    } else if crate::page::macos_major() < 14 {
        (None, Some(crate::page::STORE_NEEDS_MACOS.to_owned()))
    } else {
        let data = PluginHost::global(cx).read(cx).plugin_data(&slot.plugin);
        (
            Some(kg_plugins::keep_store(&data, &slot.contribution)?),
            None,
        )
    };
    let builder = crate::page::with_store(crate::page::private(wry::WebViewBuilder::new()), store)
        .with_url(&url)
        .with_navigation_handler(move |target| web::allows_navigation(&navigation, &target))
        .with_file_dialog(false)
        .with_drag_drop_handler(|_| true)
        .with_initialization_script(BRIDGE)
        .with_ipc_handler(move |request: wry::http::Request<String>| {
            let body = request.into_body();
            if body.len() <= kg_plugins::MESSAGE_CAP {
                let _ = sender.try_send(body);
            }
        })
        .with_custom_protocol(SCHEME.to_owned(), move |_id, request| {
            let css = served_theme.get();
            let served = web::serve(
                &root,
                &protocol_plugin,
                request.method().as_str(),
                &request.uri().to_string(),
                &hosts,
                &css,
            );
            http_response(served)
        });
    let view = gpui_webview::WebView::build(builder, window, cx)
        .map_err(|error| crate::page::failure(&error.to_string()))?;
    let drain = page_watch::watch(receiver, plugin, panel, generation, alive, cx);
    Ok(Built {
        view: cx.new(|_| view),
        theme,
        drain,
        ipc,
        life,
        note: store_note,
    })
}

fn http_response(
    served: crate::web::Served,
) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .status(served.status)
        .header(wry::http::header::CONTENT_TYPE, served.mime)
        .header(wry::http::header::CONTENT_SECURITY_POLICY, served.csp)
        .header(wry::http::header::CACHE_CONTROL, "no-store")
        .body(std::borrow::Cow::Owned(served.body))
        .unwrap_or_else(|_| wry::http::Response::new(std::borrow::Cow::Owned(Vec::new())))
}

fn listed(kagan: &Kagan, cx: &App) -> Vec<(String, bool)> {
    let mut open = Vec::new();
    let full = kagan.right_fill == right::Fill::Window && kagan.panes.right;
    let tab = right::selected_tab(kagan, cx);
    let right_on = kagan.right_drawn();
    for slot in &kagan.right_slots {
        if slot.web.is_some() {
            open.push((
                slot.id.clone(),
                right_on && tab.as_deref() == Some(slot.id.as_str()),
            ));
        }
    }
    let left = !full
        && (kagan.panes.left || (kagan.screen.drawn(screen::Side::Left) && !kagan.in_settings()));
    if let Some(slot) = &kagan.tool
        && slot.web.is_some()
    {
        open.push((slot.id.clone(), left));
    }
    if let Some(page) = kagan.screen.committed_page()
        && page.web.is_some()
    {
        let drawn = !full && kagan.screen.page().is_some_and(|shown| shown.id == page.id);
        open.push((page.id.clone(), drawn));
    }
    open
}

fn refresh_theme(cx: &mut App, index: usize) {
    let css = web::theme_css(cx);
    if cx.default_global::<Pages>().pages[index].theme.get() == css {
        return;
    }
    cx.default_global::<Pages>().pages[index].theme.set(css);
    if let Some(view) = cx.default_global::<Pages>().pages[index].view.clone() {
        paint_theme(&view, cx);
    }
}

fn paint_theme(view: &Entity<gpui_webview::WebView>, cx: &mut App) {
    let native = match crate::theme::ThemeState::global(cx).read(cx).mode {
        gpui_kit::component::ThemeMode::Light => wry::Theme::Light,
        gpui_kit::component::ThemeMode::Dark => wry::Theme::Dark,
    };
    let script = web::theme_refresh(cx);
    view.update(cx, |view, _| {
        let _ = view.set_theme(native);
        let _ = view.evaluate_script(&script);
    });
}

fn reveal(page: &Entity<gpui_webview::WebView>, cx: &mut App) {
    if !page.read(cx).visible() {
        page.update(cx, |page, _| page.show());
    }
}

fn conceal_at(cx: &mut App, index: usize) {
    let Some(page) = cx.default_global::<Pages>().pages[index].view.clone() else {
        return;
    };
    if page.read(cx).visible() {
        page.update(cx, |page, _| page.hide());
    }
}

pub(super) fn find(cx: &App, slot: &str) -> Option<usize> {
    cx.try_global::<Pages>()?
        .pages
        .iter()
        .position(|page| page.slot == slot)
}

pub(super) fn save(cx: &mut App, slot: Held) {
    cx.default_global::<Pages>().pages.push(slot);
}
