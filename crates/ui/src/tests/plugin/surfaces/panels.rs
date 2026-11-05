use crate::kagan::screen_read;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, Focusable as _, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, PluginId, PluginOp};

use super::{
    BLANK, body, focus_page_input, install_blank, install_pair, item, open_surface, run,
    show_thread, surfaces, tab_to, terminal_focused, titles, wide,
};
use crate::tests::kagan::{focused_of, label, open_with_plugins_then, shows, wait_until};

const ALPHA: &str = "dev.test.alpha";
const BETA: &str = "dev.test.beta";

#[gpui_kit::test]
fn a_right_panel_opens_as_a_tab_and_the_tool_still_opens(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_blank);
    wide(cx, window);
    assert!(titles(cx, &kagan).iter().all(|title| title != "Blank side"));
    open_surface(cx, BLANK, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, &body(BLANK, "side")));
    assert!(item(&surfaces(cx), BLANK, "side").visible);
    assert!(shows(cx, window, &format!("surface-item-{BLANK}-tool")));
    assert!(shows(cx, window, "right-tab-bar"));
}

#[gpui_kit::test]
fn a_right_panel_and_a_tool_open_together(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    open_surface(cx, ALPHA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert!(item(&surfaces(cx), ALPHA, "side").visible);
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);
}

pub(in crate::tests) fn set_plugin(cx: &mut TestAppContext, enabled: bool) {
    let op = if enabled {
        PluginOp::Enable {
            id: PluginId(ALPHA.into()),
        }
    } else {
        PluginOp::Disable {
            id: PluginId(ALPHA.into()),
        }
    };
    run(cx, Op::Plugin(op)).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn removing_a_focused_plugin_view_moves_focus_and_leaves_other_focus(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    show_thread(cx, window);
    reload_moves_focus_to_close(cx, window, data.path());
    disable_moves_focus_to_the_terminal(cx, window, &kagan);
    disable_leaves_terminal_focus(cx, window, &kagan);
    disable_moves_tool_focus_to_the_trigger(cx, window);
    removal_moves_focus_to_the_terminal(cx, window, &kagan);
}

pub(in crate::tests) fn reload_moves_focus_to_close(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    data: &Path,
) {
    focus_page_input(cx, window);
    let entry = data.join("alpha").join("main.js");
    let next = std::fs::read_to_string(&entry)
        .unwrap()
        .replace("page-ready", "page-next");
    std::fs::write(&entry, next).unwrap();
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(ALPHA.into()),
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| label(cx, window, "alpha-page") == "page-next");
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));
    assert!(shows(cx, window, "surface-page"));
}

pub(in crate::tests) fn disable_moves_focus_to_the_terminal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    focus_page_input(cx, window);
    set_plugin(cx, false);
    assert!(!shows(cx, window, "surface-page"));
    assert!(terminal_focused(cx, window, kagan));
}

pub(in crate::tests) fn disable_leaves_terminal_focus(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    set_plugin(cx, true);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    cx.update_window(window, |_, window, cx| {
        let view = screen_read::terminal_view(kagan.read(cx)).expect("terminal");
        window.focus(&view.focus_handle(cx), cx);
    })
    .unwrap();
    assert!(terminal_focused(cx, window, kagan));
    set_plugin(cx, false);
    assert!(terminal_focused(cx, window, kagan));
    assert!(!shows(cx, window, "surface-page"));
}

pub(in crate::tests) fn disable_moves_tool_focus_to_the_trigger(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
) {
    set_plugin(cx, true);
    open_surface(cx, ALPHA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-tool-count"));
    tab_to(cx, window, "alpha-tool-count");
    set_plugin(cx, false);
    assert_eq!(
        focused_of(cx, window, &format!("surface-item-{BETA}-tool")),
        Some(true)
    );
}

pub(in crate::tests) fn removal_moves_focus_to_the_terminal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    set_plugin(cx, true);
    show_thread(cx, window);
    focus_page_input(cx, window);
    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: PluginId(ALPHA.into()),
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval")
            .id
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "surface-page"));
    assert!(terminal_focused(cx, window, kagan));
}
