#[cfg(target_os = "macos")]
use super::dock;
use super::{Kagan, MIN_HEIGHT, TITLE, attention, layout, notify, web_panel};
use crate::kagan::dispatch;

use gpui_kit::component::TitleBar;
use gpui_kit::{
    Action, AnyWindowHandle, App, AppContext as _, Context, Entity, Global, WeakEntity, Window,
    WindowBounds, WindowOptions, px, size,
};
use kg_domain::{Op, ProjectId, ThreadId, WindowOp};
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

pub(crate) fn show(cx: &mut App) -> gpui_kit::Result<()> {
    crate::presentation::activate(cx);
    if let Some(open) = cx.try_global::<OpenWindow>().map(|open| open.handle)
        && cx
            .update_window(open, |_, window, cx| {
                crate::presentation::activate_window(window, cx);
            })
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

pub(crate) fn front(cx: &App) -> Option<(AnyWindowHandle, Entity<Kagan>)> {
    let open = cx.try_global::<OpenWindow>()?;
    Some((open.handle, open.kagan.upgrade()?))
}

pub(crate) fn show_settings(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    with_front(cx, Kagan::open_settings);
    Ok(())
}

pub(crate) fn show_skills(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    with_front(cx, Kagan::show_skills);
    Ok(())
}

pub(crate) fn show_project_settings(project: ProjectId, cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    with_front(cx, move |kagan, window, cx| {
        kagan.open_project_page(project, window, cx);
    });
    Ok(())
}

pub(crate) fn show_plugin_settings(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    with_front(cx, Kagan::manage_plugins);
    Ok(())
}

pub(crate) fn open_thread(id: &ThreadId, cx: &mut App) {
    with_front(cx, |kagan, window, cx| kagan.open_thread(id, window, cx));
}

pub(crate) fn show_preview(thread: Option<ThreadId>, path: String, cx: &mut App) {
    if front(cx).is_none() {
        return;
    }
    crate::presentation::activate(cx);
    with_front(cx, |kagan, window, cx| {
        if let Some(id) = thread.as_ref()
            && kagan.shown_thread(cx).as_ref() != Some(id)
        {
            kagan.select(id.clone(), window, cx);
        }
        kagan.show_preview(thread, path, window, cx);
    });
}

pub(crate) fn start_plugin(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    with_front(cx, Kagan::add_plugin);
    Ok(())
}

pub(crate) fn with_front(
    cx: &mut App,
    act: impl FnOnce(&mut Kagan, &mut Window, &mut Context<Kagan>),
) {
    let Some((handle, kagan)) = front(cx) else {
        return;
    };
    _ = cx.update_window(handle, |_, window, cx| {
        kagan.update(cx, |kagan, cx| act(kagan, window, cx));
    });
}

pub(super) fn dispatch(op: WindowOp, cx: &mut App) {
    dispatch::logged(Op::Window(op), cx);
}

pub(super) fn bind<A: Action>(cx: &mut App, op: WindowOp) {
    cx.on_action(move |_: &A, cx| dispatch(op, cx));
}

pub(super) fn release_window(this: &mut Kagan, cx: &mut App) {
    web_panel::clear(cx);
    let id = this.window.window_id();
    this.cancel_plugin_start();
    this.slots.surface_check = None;
    this.slots.surface_hold = false;
    if let Some(host) = HostUi::global(cx) {
        host.update(cx, |host, cx| host.remove_window(id, cx));
    }
}

pub(crate) fn window_options(cx: &mut App) -> WindowOptions {
    crate::prepare_window(
        WindowOptions {
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
        },
        cx,
    )
}
