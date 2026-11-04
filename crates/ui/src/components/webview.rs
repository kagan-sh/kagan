//! `WebView`: one local page a plugin panel can place as its body.
//!
//! The page is kept on a global, not in element state. Element state is dropped when the panel
//! leaves the tree, and that would reload the page. A native view also keeps its last place, so a
//! panel that is off screen hides the page instead of leaving it where it was.

use std::sync::Arc;

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Refineable as _, Role, SharedString, Styled as _, WeakEntity, Window, div,
};
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, ComponentArgument, ComponentDescriptor,
    ComponentMaterializer, ComponentPayload, ComponentRegistry, ConstructorDescriptor,
    MaterializeRequest, MethodDescriptor, RegistryError, ScriptView, anyhow,
};

use super::component_error::ComponentError;

pub(crate) const REFUSED_SCHEME: &str = "WebView only opens an http or https page.";
pub(crate) const REFUSED_HOST: &str =
    "WebView only opens 127.0.0.1, localhost, or a host this plugin may fetch.";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const UNAVAILABLE: &str = "This page works on macOS and Windows only for now.";

/// The script's root is a `WebView`, so the panel can sit edge to edge.
pub(crate) fn fills_panel(view: &Entity<ScriptView>, cx: &App) -> bool {
    view.read(cx).snapshot().is_some_and(|snapshot| {
        snapshot
            .debug_tree()
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("WebView"))
    })
}

pub(super) fn allowed(url: &str, hosts: &[String]) -> Result<(), &'static str> {
    let parsed = url::Url::parse(url).map_err(|_| REFUSED_SCHEME)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(REFUSED_SCHEME);
    }
    let Some(host) = parsed.host_str() else {
        return Err(REFUSED_SCHEME);
    };
    if host.eq_ignore_ascii_case("127.0.0.1")
        || host.eq_ignore_ascii_case("localhost")
        || hosts
            .iter()
            .any(|granted| granted.eq_ignore_ascii_case(host))
    {
        Ok(())
    } else {
        Err(REFUSED_HOST)
    }
}

pub(super) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    registry
        .register(
            ComponentDescriptor::new("WebView", Arc::new(WebView))
                .with_constructors(vec![ConstructorDescriptor::new(
                    "WebView",
                    vec![super::string("id")],
                    |arguments| {
                        let [ComponentArgument::String(id)] = arguments else {
                            return Err("new WebView(id) expects one string".into());
                        };
                        super::nonempty(id)?;
                        Ok(ComponentPayload::new(id.clone()))
                    },
                )])
                .with_methods(vec![
                    MethodDescriptor::new(
                        "url",
                        vec![ArgumentDescriptor::new("url", ArgumentSchema::String)],
                        |arguments| match arguments {
                            [ComponentArgument::String(url)] => {
                                let url = url.clone();
                                let set: Set = Arc::new(move |props: &mut Props| {
                                    props.url = Some(url.clone());
                                });
                                Ok(ComponentPayload::new(set))
                            }
                            _ => Err("WebView.url expects one string".into()),
                        },
                    )
                    .with_documentation(
                        "Loads this http or https URL. The host must be 127.0.0.1, localhost, or a host in this plugin's powers.fetch.",
                    ),
                    MethodDescriptor::new(
                        "store",
                        vec![ArgumentDescriptor::new("keep", ArgumentSchema::Boolean)],
                        |arguments| match arguments {
                            [ComponentArgument::Boolean(keep)] => {
                                let keep = *keep;
                                let set: Set = Arc::new(move |props: &mut Props| {
                                    props.store = keep;
                                });
                                Ok(ComponentPayload::new(set))
                            }
                            _ => Err("WebView.store expects one boolean".into()),
                        },
                    )
                    .with_documentation(
                        "When true, keeps cookies and storage under this plugin's data folder across opens. Default false: private, gone when the page closes.",
                    ),
                ])
                .with_documentation(
                    "One page filling its parent. new WebView(id) takes the element id. url(url) loads an http or https address on 127.0.0.1, localhost, or a host in this plugin's powers.fetch. store(true) keeps cookies and storage under the plugin data folder. Any other address is refused, and so is a navigation that leaves those hosts.",
                ),
        )
        .map(|_| ())
}

type Set = Arc<dyn Fn(&mut Props) + Send + Sync>;

struct Props {
    url: Option<String>,
    store: bool,
}

struct WebView;

impl ComponentMaterializer for WebView {
    fn materialize(&self, mut request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let id = request
            .payload()
            .downcast_ref::<String>()
            .ok_or_else(|| ComponentError::new("WebView received an incompatible id"))?
            .clone();
        let mut props = Props {
            url: None,
            store: false,
        };
        for method in request.methods() {
            if let Some(set) = method.payload().downcast_ref::<Set>() {
                set(&mut props);
            }
        }
        let url = props.url.unwrap_or_default();
        let store = props.store;
        let style = request.take_style();
        let children = request.take_children()?;
        if !children.is_empty() {
            anyhow::bail!("WebView takes no children");
        }
        request.with_window_app(move |window, cx| {
            let shown = place(&id, &url, store, window, cx);
            let mut element = frame(&id, &shown, cx);
            element.style().refine(&style);
            Ok(element.into_any_element())
        })
    }
}

struct Shown {
    note: Option<String>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    page: Option<Entity<crate::web_page::WebPage>>,
}

fn frame(
    id: &str,
    shown: &Shown,
    cx: &App,
) -> impl gpui_kit::Styled + gpui_kit::ParentElement + gpui_kit::IntoElement {
    let note = shown.note.clone();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let page = shown.page.clone();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let fill_note = page.is_none();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let fill_note = true;
    let element = div()
        .id(ElementId::Name(format!("webview-{id}").into()))
        .ui_container()
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .when_some(note, |element, note| {
            element.child(sentence(id, note.as_str(), fill_note, cx))
        });
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let element = element.when_some(page, |element, page| element.child(page).min_h_0().flex_1());
    element
}

fn sentence(id: &str, note: &str, fill: bool, cx: &App) -> AnyElement {
    div()
        .w_full()
        .when(fill, gpui_kit::Styled::flex_1)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id(ElementId::Name(format!("webview-{id}-note").into()))
                .ui_observe_as(Facts {
                    role: Role::Label,
                    ..Facts::button(note.to_owned())
                })
                .type_small()
                .text_color(cx.theme().danger)
                .child(SharedString::from(note.to_owned())),
        )
        .into_any_element()
}

enum Gate {
    Empty,
    Note(&'static str),
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Open,
}

fn gate(url: &str, hosts: &[String]) -> Gate {
    if url.is_empty() {
        return Gate::Empty;
    }
    if let Err(reason) = allowed(url, hosts) {
        return Gate::Note(reason);
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return Gate::Open;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Gate::Note(UNAVAILABLE)
}

fn place(id: &str, url: &str, store: bool, window: &mut Window, cx: &mut App) -> Shown {
    let fetched = kg_plugins::PluginHost::try_global(cx)
        .and_then(|host| host.read(cx).view_fetch(window.current_view()));
    let Some((owner, plugin, hosts)) = fetched else {
        return loose(id, url, window, cx);
    };
    #[cfg(target_os = "macos")]
    let keep = store.then(|| {
        let host = kg_plugins::PluginHost::global(cx).read(cx);
        if !host.allows_page_store(&plugin) {
            return Ok(None);
        }
        if crate::web_page::macos_major() < 14 {
            return Err(crate::web_page::STORE_NEEDS_MACOS.to_owned());
        }
        let data = host.plugin_data(&plugin);
        kg_plugins::keep_store(&data, id)
            .map(Some)
            .map_err(|error| error.to_string())
    });
    #[cfg(not(target_os = "macos"))]
    let keep = {
        let _ = (store, &plugin);
        None
    };
    owned(&owner, id, url, &hosts, keep, window, cx)
}

fn loose(
    #[cfg(any(target_os = "macos", target_os = "windows"))] id: &str,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] _id: &str,
    url: &str,
    #[cfg(any(target_os = "macos", target_os = "windows"))] window: &mut Window,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] _window: &mut Window,
    #[cfg(any(target_os = "macos", target_os = "windows"))] cx: &mut App,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] _cx: &mut App,
) -> Shown {
    match gate(url, &[]) {
        Gate::Empty => blank(),
        Gate::Note(reason) => noted(reason),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Gate::Open => {
            let key = ElementId::Name(format!("webview-loose-{id}-{url}").into());
            let state =
                window.use_keyed_state(key, cx, |window, cx| build(url, &[], None, window, cx));
            match state.read(cx) {
                Ok(page) => page_shown(page.clone()),
                Err(message) => Shown {
                    note: Some(message.clone()),
                    page: None,
                },
            }
        }
    }
}

fn owned(
    owner: &WeakEntity<ScriptView>,
    id: &str,
    url: &str,
    hosts: &[String],
    #[cfg(any(target_os = "macos", target_os = "windows"))] keep: Option<
        Result<Option<[u8; 16]>, String>,
    >,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] _keep: Option<
        Result<Option<[u8; 16]>, String>,
    >,
    #[cfg(any(target_os = "macos", target_os = "windows"))] window: &mut Window,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] _window: &mut Window,
    cx: &mut App,
) -> Shown {
    let index = find(cx, owner.entity_id(), id);
    match gate(url, hosts) {
        Gate::Empty => {
            if let Some(index) = index {
                conceal_at(cx, index);
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                {
                    cx.default_global::<Pages>().pages[index].page = None;
                }
                write(cx, index, "", None);
            }
            blank()
        }
        Gate::Note(reason) => {
            if let Some(index) = index {
                conceal_at(cx, index);
                write(cx, index, url, Some(reason.to_owned()));
            }
            noted(reason)
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Gate::Open => match keep {
            Some(Err(message)) if message == crate::web_page::STORE_NEEDS_MACOS => {
                let mut shown = open_owned(
                    Open {
                        owner: owner.clone(),
                        id,
                        url,
                        hosts,
                        store: None,
                        index,
                    },
                    window,
                    cx,
                );
                shown.note = Some(message.clone());
                if let Some(index) = find(cx, owner.entity_id(), id) {
                    cx.default_global::<Pages>().pages[index].note = Some(message);
                }
                shown
            }
            Some(Err(message)) => {
                if let Some(index) = index {
                    conceal_at(cx, index);
                    write(cx, index, url, Some(message.clone()));
                }
                noted(&message)
            }
            other => {
                let store = other.and_then(Result::ok).flatten();
                open_owned(
                    Open {
                        owner: owner.clone(),
                        id,
                        url,
                        hosts,
                        store,
                        index,
                    },
                    window,
                    cx,
                )
            }
        },
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct Open<'a> {
    owner: WeakEntity<ScriptView>,
    id: &'a str,
    url: &'a str,
    hosts: &'a [String],
    store: Option<[u8; 16]>,
    index: Option<usize>,
}

fn blank() -> Shown {
    Shown {
        note: None,
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        page: None,
    }
}

fn noted(reason: &str) -> Shown {
    Shown {
        note: Some(reason.to_owned()),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        page: None,
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn page_shown(page: Entity<crate::web_page::WebPage>) -> Shown {
    Shown {
        note: None,
        page: Some(page),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn open_owned(open: Open<'_>, window: &mut Window, cx: &mut App) -> Shown {
    let Open {
        owner,
        id,
        url,
        hosts,
        store,
        index,
    } = open;
    if let Some(index) = index {
        let (saved_url, saved_hosts, saved_store, note, page) = {
            let slot = &cx.default_global::<Pages>().pages[index];
            (
                slot.url.clone(),
                slot.hosts.clone(),
                slot.store,
                slot.note.clone(),
                slot.page.clone(),
            )
        };
        if saved_url == url && saved_hosts == hosts && saved_store == store {
            if let Some(page) = page {
                reveal(&page, cx);
                return page_shown(page);
            }
            if note.is_some() {
                return Shown { note, page: None };
            }
        } else if saved_hosts == hosts
            && saved_store == store
            && let Some(page) = page.clone()
        {
            page.update(cx, |page, cx| page.load_url(url, cx));
            reveal(&page, cx);
            write_page(cx, index, url, page.clone());
            return page_shown(page);
        } else if let Some(page) = page {
            conceal(&page, cx);
            cx.default_global::<Pages>().pages[index].page = None;
        }
    }
    match build(url, hosts, store, window, cx) {
        Ok(page) => {
            save(
                cx,
                index,
                held(owner, id, url, hosts, store, Some(page.clone()), None),
            );
            page_shown(page)
        }
        Err(message) => {
            let note = Some(message.clone());
            save(
                cx,
                index,
                held(owner, id, url, hosts, store, None, note.clone()),
            );
            Shown { note, page: None }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build(
    url: &str,
    hosts: &[String],
    store: Option<[u8; 16]>,
    window: &mut Window,
    cx: &mut App,
) -> Result<Entity<crate::web_page::WebPage>, String> {
    let hosts = hosts.to_vec();
    let url = url.to_owned();
    crate::web_page::open(window, cx, move |builder| {
        crate::web_page::with_store(builder, store)
            .with_url(&url)
            .with_navigation_handler(move |target| allowed(&target, &hosts).is_ok())
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn reveal(page: &Entity<crate::web_page::WebPage>, cx: &mut App) {
    if !page.read(cx).visible(cx) {
        page.update(cx, |page, cx| page.show(cx));
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn conceal(page: &Entity<crate::web_page::WebPage>, cx: &mut App) {
    if page.read(cx).visible(cx) {
        page.update(cx, |page, cx| page.hide(cx));
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn conceal_at(cx: &mut App, index: usize) {
    let Some(page) = cx.default_global::<Pages>().pages[index].page.clone() else {
        return;
    };
    conceal(&page, cx);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn conceal_at(_cx: &mut App, _index: usize) {}

fn write(cx: &mut App, index: usize, url: &str, note: Option<String>) {
    let slot = &mut cx.default_global::<Pages>().pages[index];
    url.clone_into(&mut slot.url);
    slot.note = note;
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn write_page(cx: &mut App, index: usize, url: &str, page: Entity<crate::web_page::WebPage>) {
    let slot = &mut cx.default_global::<Pages>().pages[index];
    url.clone_into(&mut slot.url);
    slot.note = None;
    slot.page = Some(page);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn held(
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn save(cx: &mut App, index: Option<usize>, slot: Held) {
    let pages = &mut cx.default_global::<Pages>().pages;
    if let Some(index) = index {
        pages[index] = slot;
    } else {
        pages.push(slot);
    }
}

fn find(cx: &App, owner: gpui_kit::EntityId, id: &str) -> Option<usize> {
    cx.try_global::<Pages>()?
        .pages
        .iter()
        .position(|page| page.owner.entity_id() == owner && page.id == id)
}

/// Re-renders the window once after a plugin panel's first snapshot, so the panel can drop its
/// padding when that snapshot fills it.
pub(crate) fn nudge_once(window: &mut Window, cx: &mut App) {
    let owner = window.current_view();
    let pages = cx.default_global::<Pages>();
    if pages.nudged.contains(&owner) {
        return;
    }
    let Some(kagan) = window.window_handle().downcast::<crate::kagan::Kagan>() else {
        return;
    };
    pages.nudged.push(owner);
    window.defer(cx, move |_, cx| {
        let _ = kagan.update(cx, |_, _, cx| cx.notify());
    });
}

#[derive(Default)]
struct Pages {
    pages: Vec<Held>,
    nudged: Vec<gpui_kit::EntityId>,
}

impl gpui_kit::Global for Pages {}

struct Held {
    owner: WeakEntity<ScriptView>,
    id: String,
    url: String,
    note: Option<String>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    hosts: Vec<String>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    store: Option<[u8; 16]>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    page: Option<Entity<crate::web_page::WebPage>>,
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
    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        cx.try_global::<Pages>().and_then(|pages| {
            pages
                .pages
                .iter()
                .find(|page| page.id == id)
                .and_then(|page| page.page.as_ref().map(|view| view.read(cx).visible(cx)))
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (cx, id);
        None
    }
}
