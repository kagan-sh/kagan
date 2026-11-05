use crate::kagan::SurfaceCommit;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, InputEvent as _, TestAppContext, Window,
};
use kg_core::Workspace;
use kg_domain::{
    Appearance, Op, OpResult, PluginId, SurfaceItem, SurfaceOp, SurfacePlacement, ThemeOp,
    ThreadId, ThreadOp,
};
use serde_json::json;

use super::{
    BLANK, install_blank, install_pair, item, open_surface, replied, run, saved, show_thread,
    surfaces, terminal_focused, wide,
};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, run_shell, shows, thread_record, wait_until,
};

const ALPHA: &str = "dev.test.alpha";

pub(in crate::tests) fn screen(cx: &mut TestAppContext, id: &ThreadId) -> String {
    match run(
        cx,
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback: true,
            lines: None,
            terminal: None,
        }),
    )
    .unwrap()
    {
        OpResult::Screen(text) => text,
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn set_mode(cx: &mut TestAppContext, mode: Appearance) {
    run(
        cx,
        Op::Theme(ThemeOp::Set {
            theme: "fjord".into(),
            mode,
        }),
    )
    .unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn read_list(
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) -> Vec<SurfaceItem> {
    let step = kagan
        .update(cx, |kagan, cx| {
            kagan.commit(SurfaceOp::List, None, window, cx)
        })
        .expect("list");
    match step {
        SurfaceCommit::Ready(result) => match *result {
            OpResult::Surfaces(items) => items,
            other => panic!("{other:?}"),
        },
        SurfaceCommit::Paint => panic!("list does not wait for a frame"),
    }
}

pub(in crate::tests) fn emit_open(
    cx: &mut gpui_kit::App,
    plugin: &str,
    panel: &str,
    mark: &str,
) -> async_channel::Receiver<Result<(), String>> {
    let ui = kg_plugins::HostUi::global(cx).expect("host");
    let (reply, incoming) = async_channel::bounded(1);
    ui.update(cx, |_, cx| {
        cx.emit(kg_plugins::HostUiEvent::OpenPanel {
            plugin: PluginId(plugin.into()),
            panel: panel.into(),
            props: json!({ "mark": mark }),
            reply,
        });
    });
    incoming
}

pub(in crate::tests) fn assert_replaced(reply: &async_channel::Receiver<Result<(), String>>) {
    let err = replied(reply).unwrap_err();
    assert!(err.contains("replaced"), "{err}");
}

pub(in crate::tests) fn tap_then_settings(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    cx.update_window(window, |_, window, cx| {
        let keystroke = kg_core::parse_key("escape").unwrap();
        window.dispatch_event(
            gpui_kit::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(gpui_kit::KeyUpEvent { keystroke }.to_platform_input(), cx);
        kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_pending_surface_yields_to_archived(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let thread = run_shell(cx);
    click(cx, window, &format!("thread-{thread}"));
    cx.dispatch_action(window, crate::kagan::ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &thread).archived_at.is_some());
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    let left = saved(cx, "main.left-sidebar");
    let right = saved(cx, "main.right-sidebar");

    let reply = cx
        .update_window(window, |_, window, cx| {
            let incoming = emit_open(cx, ALPHA, "tool", "lost");
            window.defer(cx, |window, cx| {
                crate::ui_dispatch::click(window, &ElementId::Name("threads-home".into()), cx)
                    .expect("threads");
            });
            incoming
        })
        .unwrap();
    cx.run_until_parked();
    assert_replaced(&reply);
    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, "archived-search"));
    assert!(shows(cx, window, "archived-search"));
    assert_eq!(focused_of(cx, window, "archived-search"), Some(true));
    assert_eq!(
        label(cx, window, "archived-note"),
        "Restoring brings the thread back. The program stays stopped."
    );
    assert!(!shows(cx, window, "surface-page"));
    assert_ne!(label(cx, window, "alpha-side"), "lost");
    assert_eq!(saved(cx, "main.left-sidebar"), left);
    assert_eq!(saved(cx, "main.right-sidebar"), right);
}

#[gpui_kit::test]
fn a_pending_open_yields_to_a_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    let reply = cx
        .update_window(window, |_, window, cx| {
            let incoming = emit_open(cx, ALPHA, "page", "lost");
            let pending = read_list(&kagan, window, cx);
            assert!(!item(&pending, ALPHA, "page").open, "{pending:?}");
            assert!(!item(&pending, ALPHA, "page").visible, "{pending:?}");
            let (thread_reply, _thread_in) = async_channel::bounded(1);
            let ui = kg_plugins::HostUi::global(cx).expect("host");
            ui.update(cx, |_, cx| {
                cx.emit(kg_plugins::HostUiEvent::OpenThread {
                    id: second.clone(),
                    reply: thread_reply,
                });
            });
            incoming
        })
        .unwrap();
    cx.run_until_parked();
    assert_replaced(&reply);
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), ALPHA, "page").open);
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(second)
    );
}

#[gpui_kit::test]
fn a_pending_open_yields_to_settings(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    show_thread(cx, window);
    let reply = cx
        .update_window(window, |_, window, cx| {
            let incoming = emit_open(cx, ALPHA, "page", "lost");
            let kagan = kagan.clone();
            window.defer(cx, move |window, cx| {
                kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
            });
            incoming
        })
        .unwrap();
    cx.run_until_parked();
    assert_replaced(&reply);
    assert!(shows(cx, window, "settings-page"));
    assert!(!terminal_focused(cx, window, &kagan));
    assert!(!shows(cx, window, "surface-page"));
}

#[gpui_kit::test]
fn a_pending_close_yields_to_settings(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    show_thread(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    tap_then_settings(cx, window, &kagan);
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    assert!(!terminal_focused(cx, window, &kagan));
    assert!(!shows(cx, window, "surface-page"));
}

#[gpui_kit::test]
fn a_pending_open_yields_to_a_plugin_change(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let reply = cx
        .update_window(window, |_, window, cx| {
            let incoming = emit_open(cx, ALPHA, "page", "lost");
            let id = PluginId(ALPHA.into());
            window.defer(cx, move |_, cx| {
                kg_plugins::PluginHost::global(cx)
                    .update(cx, |host, cx| host.disable_loaded(&id, cx));
            });
            incoming
        })
        .unwrap();
    cx.run_until_parked();
    assert_replaced(&reply);
    assert!(!shows(cx, window, "surface-page"));
    assert!(surfaces(cx).iter().all(|item| item.plugin.0 != ALPHA));
}

#[gpui_kit::test]
fn list_hides_a_waiting_surface_and_a_zero_height_body(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_blank);
    wide(cx, window);
    cx.run_until_parked();
    assert_eq!(
        item(&surfaces(cx), BLANK, "side").placement,
        SurfacePlacement::Right
    );
    assert!(!item(&surfaces(cx), BLANK, "side").open);

    let reply = cx
        .update_window(window, |_, window, cx| {
            let incoming = emit_open(cx, BLANK, "tool", "gone");
            let pending = read_list(&kagan, window, cx);
            assert!(!item(&pending, BLANK, "tool").open, "{pending:?}");
            assert!(!item(&pending, BLANK, "tool").visible, "{pending:?}");
            incoming
        })
        .unwrap();
    cx.run_until_parked();
    let err = replied(&reply).unwrap_err();
    assert!(err.contains("not on screen"), "{err}");
    assert!(!item(&surfaces(cx), BLANK, "tool").open);
    assert!(!item(&surfaces(cx), BLANK, "tool").visible);
}
