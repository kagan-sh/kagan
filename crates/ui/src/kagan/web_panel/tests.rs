use crate::ui_observe::UiObserveExt as _;
use gpui_kit::App;
use gpui_kit::{
    AppContext as _, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
};
use kg_domain::PluginId;
use kg_plugins::PluginHost;

use super::native::{Held, Pages, find, save};
use super::page_watch;

// Puts a raw IPC body on the page channel. The drain, not this helper, checks the generation.
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

// Queues a page job with the open page's lifetime token, as the IPC drain does after its checks.
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

pub(crate) fn mark_stale(slot: &str, cx: &mut App) -> bool {
    let Some(index) = find(cx, slot) else {
        return false;
    };
    cx.default_global::<Pages>().pages[index].generation = u64::MAX;
    true
}

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

struct Panel {
    note: Option<&'static str>,
    page: bool,
}

impl gpui_kit::Render for Panel {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        let page = self.page.then(|| {
            gpui_kit::div()
                .id(gpui_kit::ElementId::Name("panel-page".into()))
                .size_full()
                .ui_observe_as(crate::ui_registry::Facts::button("panel-page"))
                .into_any_element()
        });
        gpui_kit::div()
            .size(gpui_kit::px(400.))
            .child(super::native::panel_body("panel", self.note, page, cx))
    }
}

fn drawn(
    cx: &mut gpui_kit::TestAppContext,
    note: Option<&'static str>,
    page: bool,
) -> (tempfile::TempDir, gpui_kit::VisualTestContext) {
    let data = tempfile::tempdir().unwrap();
    let opened = cx.update(|cx| {
        kg_core::Workspace::open(data.path().into(), std::env::current_exe().unwrap(), cx)
    });
    cx.foreground_executor().block_test(opened).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::theme::init_in(data.path().join("themes"), cx);
    });
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(400.), gpui_kit::px(300.)),
        |window, cx| {
            let view = cx.new(|_| Panel { note, page });
            gpui_kit::component::Root::new(view, window, cx)
        },
    );
    (
        data,
        gpui_kit::VisualTestContext::from_window(handle.into(), cx),
    )
}

fn placed(window: &gpui_kit::Window, name: &str) -> Option<(f32, f32)> {
    crate::ui_registry::find(window, &gpui_kit::ElementId::Name(name.to_owned().into())).map(
        |element| {
            let bounds = element.bounds;
            (f32::from(bounds.origin.y), f32::from(bounds.size.height))
        },
    )
}

#[gpui_kit::test]
fn a_warning_alone_fills_the_panel(cx: &mut gpui_kit::TestAppContext) {
    let (_themes, mut visual) = drawn(cx, Some("Saving page data needs macOS 14 or later."), false);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let (top, _) = placed(window, "web-panel-panel-note").expect("the warning is drawn");
        assert!(
            top > 100.,
            "the warning is centred in the panel, its top was {top}"
        );
    });
}

#[gpui_kit::test]
fn a_warning_over_a_page_leaves_the_page_its_room(cx: &mut gpui_kit::TestAppContext) {
    let (_themes, mut visual) = drawn(cx, Some("Saving page data needs macOS 14 or later."), true);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let (_, note) = placed(window, "web-panel-panel-note").expect("the warning is drawn");
        let (_, page) = placed(window, "panel-page").expect("the page is drawn");
        assert!(note < 100., "the warning keeps its own height, was {note}");
        assert!(
            page > 200.,
            "the page keeps the rest of the panel, was {page}"
        );
    });
}
