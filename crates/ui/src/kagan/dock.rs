use super::attention;
use gpui_kit::{App, Global, Subscription};
use kg_core::{Workspace, WorkspaceEvent};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use objc2_foundation::NSString;

struct DockWatch {
    _questions: Subscription,
    _workspace: Subscription,
    _quit: Subscription,
}
impl Global for DockWatch {}
fn label(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        count.to_string()
    }
}
pub(super) fn watch(cx: &mut App) {
    let attention = attention::global(cx);
    let workspace = Workspace::global(cx);
    let questions = cx.observe(&attention, |attention, cx| {
        update(attention.read(cx).count(cx));
    });
    let workspace = cx.subscribe(&workspace, |_, event, cx| {
        if matches!(
            event,
            WorkspaceEvent::Changed | WorkspaceEvent::Event { .. } | WorkspaceEvent::Waiting(_)
        ) {
            update(attention::global(cx).read(cx).count(cx));
        }
    });
    let quit = cx.on_app_quit(|_| {
        update(0);
        async {}
    });
    update(attention.read(cx).count(cx));
    cx.set_global(DockWatch {
        _questions: questions,
        _workspace: workspace,
        _quit: quit,
    });
}
fn update(count: usize) {
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let label = NSString::from_str(&label(count));
    NSApplication::sharedApplication(marker)
        .dockTile()
        .setBadgeLabel(Some(&label));
}
