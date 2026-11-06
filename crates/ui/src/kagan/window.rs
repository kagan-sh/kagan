#[cfg(target_os = "macos")]
use super::dock;
use super::{Kagan, MIN_HEIGHT, TITLE, attention, layout, notify, web_panel};

use gpui_kit::component::TitleBar;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Entity, Global, WeakEntity, WindowBounds, WindowOptions,
    px, size,
};
use kg_domain::ThreadId;
use kg_plugins::HostUi;

struct OpenWindow {
    handle: AnyWindowHandle,
    kagan: WeakEntity<Kagan>,
}
impl Global for OpenWindow {}

pub(crate) fn open(cx: &mut App) -> gpui_kit::Result<Entity<Kagan>> {
    let (handle, kagan) = gpui_kit::open_window(window_options(cx), cx, |window, cx| {
        cx.new(|cx| Kagan::new(window, cx))
    })?;
    cx.set_global(OpenWindow {
        handle,
        kagan: kagan.downgrade(),
    });
    Ok(kagan)
}

/// Brings the app and its window to the front. A window that is already open comes forward; a
/// new one opens only when there is none.
pub(crate) fn show(cx: &mut App) -> gpui_kit::Result<()> {
    cx.activate(true);
    if let Some(open) = cx.try_global::<OpenWindow>().map(|open| open.handle)
        && cx
            .update_window(open, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Ok(());
    }
    open(cx).map(|_| ())
}

pub(crate) fn watch_waiting(cx: &mut App) {
    attention::init(cx);
    #[cfg(target_os = "macos")]
    dock::watch(cx);
    notify::init(cx);
    notify::watch(cx);
}

pub(crate) fn shows(id: &ThreadId, cx: &App) -> bool {
    cx.try_global::<OpenWindow>()
        .and_then(|open| open.kagan.upgrade())
        .is_some_and(|kagan| kagan.read(cx).shown_thread(cx).as_ref() == Some(id))
}

/// Shows the settings page in the window, opening the window first when none is open.
pub(crate) fn front(cx: &App) -> Option<(AnyWindowHandle, Entity<Kagan>)> {
    let open = cx.try_global::<OpenWindow>()?;
    Some((open.handle, open.kagan.upgrade()?))
}

pub(crate) fn show_settings(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    let Some(open) = cx.try_global::<OpenWindow>() else {
        return Ok(());
    };
    let (handle, kagan) = (open.handle, open.kagan.clone());
    if let Some(kagan) = kagan.upgrade() {
        _ = cx.update_window(handle, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
        });
    }
    Ok(())
}

pub(crate) fn show_plugin_settings(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    if let Some((handle, kagan)) = front(cx) {
        _ = cx.update_window(handle, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.manage_plugins(window, cx));
        });
    }
    Ok(())
}

/// Shows one file in the Preview tab, selecting the named thread first. Does nothing when no
/// window is open.
pub(crate) fn show_preview(thread: Option<ThreadId>, path: String, cx: &mut App) {
    let Some((handle, kagan)) = front(cx) else {
        return;
    };
    cx.activate(true);
    _ = cx.update_window(handle, |_, window, cx| {
        kagan.update(cx, |kagan, cx| {
            if let Some(id) = thread.as_ref()
                && kagan.shown_thread(cx).as_ref() != Some(id)
            {
                kagan.select(id.clone(), window, cx);
            }
            kagan.show_preview(thread, path, window, cx);
        });
    });
}

pub(crate) fn start_plugin(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    if let Some((handle, kagan)) = front(cx) {
        _ = cx.update_window(handle, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.add_plugin(window, cx));
        });
    }
    Ok(())
}

pub(super) fn release_window(this: &mut Kagan, cx: &mut App) {
    web_panel::clear(cx);
    let id = this.window.window_id();
    this.cancel_plugin_start();
    this.surface_check = None;
    this.surface_hold = false;
    if let Some(host) = HostUi::global(cx) {
        host.update(cx, |host, cx| host.remove_window(id, cx));
    }
}

pub(crate) fn window_options(cx: &mut App) -> WindowOptions {
    crate::prepare_window(WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds::centered(
            None,
            size(px(1120.), px(720.)),
            cx,
        ))),
        titlebar: Some(gpui_kit::TitlebarOptions {
            title: Some(TITLE.into()),
            ..TitleBar::title_bar_options()
        }),
        window_min_size: Some(size(
            px(layout::CENTER_MIN + layout::RAIL_WIDTH),
            px(MIN_HEIGHT),
        )),
        ..TitleBar::window_options()
    })
}
