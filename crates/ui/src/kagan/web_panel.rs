//! A plugin panel whose body is an HTML file in the plugin folder.
//!
//! The page is shown only while that panel is on screen, and dropped when the panel closes.
//! The native view runs on macOS only. Other systems show a one-line notice.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, SharedString, Styled as _, Window, div,
};
#[cfg(target_os = "macos")]
use gpui_kit::{AppContext as _, Entity};
use kg_domain::PluginId;
use kg_plugins::{PluginHost, SCHEME};

use super::surface::Slot;
use super::{Kagan, screen};
#[cfg(target_os = "macos")]
#[path = "page_watch.rs"]
mod page_watch;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use crate::web::{self, BRIDGE};

pub(super) fn body(slot: &Slot, window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(file) = slot.web.clone() else {
        return div().into_any_element();
    };
    let shown = place(slot, &file, window, cx);
    frame(&slot.id, &slot.title, &shown, cx)
}

// Errors say why the page could not take the message, such as when it is off screen.
pub(crate) fn post(
    plugin: &PluginId,
    panel: &str,
    message: &str,
    cx: &mut App,
) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (plugin, panel, message, cx);
        return Err(web::ONLY_MACOS.into());
    }
    #[cfg(target_os = "macos")]
    deliver(plugin, panel, message, cx)
}

#[cfg(target_os = "macos")]
fn deliver(plugin: &PluginId, panel: &str, message: &str, cx: &mut App) -> Result<(), String> {
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
    #[cfg(target_os = "macos")]
    if cx.has_global::<Pages>() {
        cx.default_global::<Pages>().pages.clear();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = cx;
}

pub(crate) fn forget(slot: &str, cx: &mut App) {
    #[cfg(target_os = "macos")]
    if let Some(index) = find(cx, slot) {
        let page = cx.default_global::<Pages>().pages.remove(index);
        page.ipc.close();
        // Dropping the life token expires any page job still waiting in `next()`.
        drop(page.life);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (slot, cx);
}

pub(super) fn sync(kagan: &Kagan, cx: &mut App) {
    #[cfg(target_os = "macos")]
    sync_native(kagan, cx);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kagan, cx);
    }
}

// Pushes the live Workbench theme to every open page, even when the window is not drawing.
pub(super) fn repaint(cx: &mut App) {
    #[cfg(target_os = "macos")]
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
    #[cfg(not(target_os = "macos"))]
    let _ = cx;
}

fn frame(id: &str, title: &str, shown: &Shown, cx: &App) -> AnyElement {
    let note = shown.note.clone();
    #[cfg(target_os = "macos")]
    let page = shown.note.is_none().then(|| shown.page.clone()).flatten();
    div()
        .id(ElementId::Name(format!("web-panel-{id}").into()))
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .ui_observe_as(Facts {
            role: Role::Document,
            name: "Web panel".into(),
            value: Some(title.to_owned().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: Some("kg ui cannot drive this page".into()),
        })
        .when_some(note.as_deref(), |element, note| {
            element.child(sentence(id, note, cx))
        })
        .when(shown.note.is_none(), |element| {
            #[cfg(target_os = "macos")]
            {
                element.children(page)
            }
            #[cfg(not(target_os = "macos"))]
            {
                element
            }
        })
        .into_any_element()
}

fn sentence(id: &str, note: &str, cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id(ElementId::Name(format!("web-panel-{id}-note").into()))
                .ui_observe_as(Facts {
                    role: Role::Label,
                    name: note.to_owned().into(),
                    value: None,
                    disabled: false,
                    selected: None,
                    focus: None,
                    hint: None,
                })
                .type_small()
                .text_color(cx.theme().danger)
                .child(SharedString::from(note.to_owned())),
        )
        .into_any_element()
}

struct Shown {
    note: Option<String>,
    #[cfg(target_os = "macos")]
    page: Option<Entity<gpui_webview::WebView>>,
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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (file, window, root, hosts, generation);
        noted(web::ONLY_MACOS)
    }
    #[cfg(target_os = "macos")]
    open_page(slot, file, &root, &hosts, generation, window, cx)
}

fn noted(reason: &str) -> Shown {
    Shown {
        note: Some(reason.to_owned()),
        #[cfg(target_os = "macos")]
        page: None,
    }
}

#[cfg(target_os = "macos")]
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
            save(
                cx,
                Held {
                    slot: slot.id.clone(),
                    plugin: slot.plugin.clone(),
                    panel: slot.contribution.clone(),
                    file: file.to_owned(),
                    generation,
                    note: None,
                    view: Some(built.view),
                    theme: built.theme,
                    _drain: built.drain,
                    ipc: built.ipc,
                    life: built.life,
                    shown: None,
                },
            );
            Shown {
                note: None,
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

#[cfg(target_os = "macos")]
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
    let builder = crate::page::private(wry::WebViewBuilder::new())
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
    })
}

#[cfg(target_os = "macos")]
struct Built {
    view: Entity<gpui_webview::WebView>,
    theme: web::ThemeCss,
    drain: gpui_kit::Task<()>,
    ipc: async_channel::Sender<String>,
    life: std::sync::Arc<()>,
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn sync_native(kagan: &Kagan, cx: &mut App) {
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

#[cfg(target_os = "macos")]
fn listed(kagan: &Kagan, cx: &App) -> Vec<(String, bool)> {
    let mut open = Vec::new();
    let full = kagan.right_fill == super::right::Fill::Window && kagan.panes.right;
    let tab = super::right::selected_tab(kagan, cx);
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn reveal(page: &Entity<gpui_webview::WebView>, cx: &mut App) {
    if !page.read(cx).visible() {
        page.update(cx, |page, _| page.show());
    }
}

#[cfg(target_os = "macos")]
fn conceal_at(cx: &mut App, index: usize) {
    let Some(page) = cx.default_global::<Pages>().pages[index].view.clone() else {
        return;
    };
    if page.read(cx).visible() {
        page.update(cx, |page, _| page.hide());
    }
}

#[cfg(target_os = "macos")]
fn find(cx: &App, slot: &str) -> Option<usize> {
    cx.try_global::<Pages>()?
        .pages
        .iter()
        .position(|page| page.slot == slot)
}

#[cfg(target_os = "macos")]
fn save(cx: &mut App, slot: Held) {
    cx.default_global::<Pages>().pages.push(slot);
}

#[cfg(test)]
#[path = "web_panel_test.rs"]
mod web_panel_test;

#[cfg(test)]
pub(crate) use web_panel_test::{inject, mark_stale, queue_page, testing_page};

#[cfg(target_os = "macos")]
#[derive(Default)]
struct Pages {
    pages: Vec<Held>,
}

#[cfg(target_os = "macos")]
impl gpui_kit::Global for Pages {}

#[cfg(target_os = "macos")]
struct Held {
    slot: String,
    plugin: PluginId,
    panel: String,
    file: String,
    generation: u64,
    note: Option<String>,
    view: Option<Entity<gpui_webview::WebView>>,
    theme: web::ThemeCss,
    _drain: gpui_kit::Task<()>,
    ipc: async_channel::Sender<String>,
    life: std::sync::Arc<()>,
    shown: Option<bool>,
}
