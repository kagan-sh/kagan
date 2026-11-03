//! `WebView`: one local page a plugin panel can place as its body.
//!
//! The page is kept on a global, not in element state. Element state is dropped when the panel
//! leaves the tree, and that would reload the page. A native view also keeps its last place, so a
//! panel that is off screen hides the page instead of leaving it where it was.

use std::cell::Cell;
use std::sync::Arc;

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
#[cfg(target_os = "macos")]
use gpui_kit::AppContext as _;
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

use super::fail::ComponentError;

pub(crate) const REFUSED_SCHEME: &str = "WebView only opens an http or https page.";
pub(crate) const REFUSED_HOST: &str =
    "WebView only opens 127.0.0.1, localhost, or a host this plugin may fetch.";
thread_local! {
    static INSET: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn note_inset(inset: bool) {
    INSET.set(inset);
}

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
            nudge(window, cx);
            let mut element = frame(&id, &shown, cx);
            element.style().refine(&style);
            Ok(element.into_any_element())
        })
    }
}

struct Shown {
    note: Option<String>,
    #[cfg(target_os = "macos")]
    page: Option<Entity<gpui_webview::WebView>>,
}

fn frame(
    id: &str,
    shown: &Shown,
    cx: &App,
) -> impl gpui_kit::Styled + gpui_kit::ParentElement + gpui_kit::IntoElement {
    let note = shown.note.clone();
    #[cfg(target_os = "macos")]
    let page = shown.note.is_none().then(|| shown.page.clone()).flatten();
    div()
        .id(ElementId::Name(format!("webview-{id}").into()))
        .ui_container()
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .when_some(note, |element, note| {
            element.child(sentence(id, note.as_str(), cx))
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
}

fn sentence(id: &str, note: &str, cx: &App) -> AnyElement {
    div()
        .size_full()
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
    #[cfg(target_os = "macos")]
    Open,
}

fn gate(url: &str, hosts: &[String]) -> Gate {
    if url.is_empty() {
        return Gate::Empty;
    }
    if let Err(reason) = allowed(url, hosts) {
        return Gate::Note(reason);
    }
    #[cfg(target_os = "macos")]
    return Gate::Open;
    #[cfg(not(target_os = "macos"))]
    Gate::Note(crate::web::ONLY_MACOS)
}

fn place(id: &str, url: &str, store: bool, window: &mut Window, cx: &mut App) -> Shown {
    let fetched = kg_plugins::PluginHost::try_global(cx)
        .and_then(|host| host.read(cx).view_fetch(window.current_view()));
    let Some((owner, plugin, hosts)) = fetched else {
        return loose(id, url, window, cx);
    };
    #[cfg(target_os = "macos")]
    let keep = store.then(|| {
        let data = kg_plugins::PluginHost::global(cx)
            .read(cx)
            .plugin_data(&plugin);
        kg_plugins::keep_store(&data, id)
    });
    #[cfg(not(target_os = "macos"))]
    let keep = {
        let _ = (store, &plugin);
        None
    };
    owned(&owner, id, url, &hosts, keep, window, cx)
}

fn loose(
    #[cfg(target_os = "macos")] id: &str,
    #[cfg(not(target_os = "macos"))] _id: &str,
    url: &str,
    #[cfg(target_os = "macos")] window: &mut Window,
    #[cfg(not(target_os = "macos"))] _window: &mut Window,
    #[cfg(target_os = "macos")] cx: &mut App,
    #[cfg(not(target_os = "macos"))] _cx: &mut App,
) -> Shown {
    match gate(url, &[]) {
        Gate::Empty => blank(),
        Gate::Note(reason) => noted(reason),
        #[cfg(target_os = "macos")]
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
    keep: Option<Result<[u8; 16], String>>,
    #[cfg(target_os = "macos")] window: &mut Window,
    #[cfg(not(target_os = "macos"))] _window: &mut Window,
    cx: &mut App,
) -> Shown {
    let index = find(cx, owner.entity_id(), id);
    match gate(url, hosts) {
        Gate::Empty => {
            if let Some(index) = index {
                conceal_at(cx, index);
                #[cfg(target_os = "macos")]
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
        #[cfg(target_os = "macos")]
        Gate::Open => match keep {
            Some(Err(message)) => {
                if let Some(index) = index {
                    conceal_at(cx, index);
                    write(cx, index, url, Some(message.clone()));
                }
                noted(&message)
            }
            other => {
                let store = other.and_then(Result::ok);
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

#[cfg(target_os = "macos")]
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
        #[cfg(target_os = "macos")]
        page: None,
    }
}

fn noted(reason: &str) -> Shown {
    Shown {
        note: Some(reason.to_owned()),
        #[cfg(target_os = "macos")]
        page: None,
    }
}

#[cfg(target_os = "macos")]
fn page_shown(page: Entity<gpui_webview::WebView>) -> Shown {
    Shown {
        note: None,
        page: Some(page),
    }
}

#[cfg(target_os = "macos")]
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
            page.update(cx, |page, _| page.load_url(url));
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

#[cfg(target_os = "macos")]
fn build(
    url: &str,
    hosts: &[String],
    store: Option<[u8; 16]>,
    window: &mut Window,
    cx: &mut App,
) -> Result<Entity<gpui_webview::WebView>, String> {
    let hosts = hosts.to_vec();
    let builder = crate::web::builder(url, move |target| allowed(&target, &hosts).is_ok(), store);
    match gpui_webview::WebView::build(builder, window, cx) {
        Ok(view) => Ok(cx.new(|_| view)),
        Err(error) => Err(format!("The page could not open: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn reveal(page: &Entity<gpui_webview::WebView>, cx: &mut App) {
    if !page.read(cx).visible() {
        page.update(cx, |page, _| page.show());
    }
}

#[cfg(target_os = "macos")]
fn conceal(page: &Entity<gpui_webview::WebView>, cx: &mut App) {
    if page.read(cx).visible() {
        page.update(cx, |page, _| page.hide());
    }
}

#[cfg(target_os = "macos")]
fn conceal_at(cx: &mut App, index: usize) {
    let Some(page) = cx.default_global::<Pages>().pages[index].page.clone() else {
        return;
    };
    conceal(&page, cx);
}

#[cfg(not(target_os = "macos"))]
fn conceal_at(_cx: &mut App, _index: usize) {}

fn write(cx: &mut App, index: usize, url: &str, note: Option<String>) {
    let slot = &mut cx.default_global::<Pages>().pages[index];
    url.clone_into(&mut slot.url);
    slot.note = note;
}

#[cfg(target_os = "macos")]
fn write_page(cx: &mut App, index: usize, url: &str, page: Entity<gpui_webview::WebView>) {
    let slot = &mut cx.default_global::<Pages>().pages[index];
    url.clone_into(&mut slot.url);
    slot.note = None;
    slot.page = Some(page);
}

#[cfg(target_os = "macos")]
fn held(
    owner: WeakEntity<ScriptView>,
    id: &str,
    url: &str,
    hosts: &[String],
    store: Option<[u8; 16]>,
    page: Option<Entity<gpui_webview::WebView>>,
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

#[cfg(target_os = "macos")]
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

fn nudge(window: &mut Window, cx: &mut App) {
    if !INSET.get() {
        return;
    }
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
    #[cfg(target_os = "macos")]
    hosts: Vec<String>,
    #[cfg(target_os = "macos")]
    store: Option<[u8; 16]>,
    #[cfg(target_os = "macos")]
    page: Option<Entity<gpui_webview::WebView>>,
}

/// Hides a page whose panel is off screen, and drops one whose view is gone.
pub(crate) fn sync_pages(shown: &[gpui_kit::EntityId], cx: &mut App) {
    INSET.set(false);
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
    #[cfg(target_os = "macos")]
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
            if page.read(cx).visible() != want {
                page.update(cx, |page, _| {
                    if want {
                        page.show();
                    } else {
                        page.hide();
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
    #[cfg(target_os = "macos")]
    {
        cx.try_global::<Pages>().and_then(|pages| {
            pages
                .pages
                .iter()
                .find(|page| page.id == id)
                .and_then(|page| page.page.as_ref().map(|view| view.read(cx).visible()))
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (cx, id);
        None
    }
}
