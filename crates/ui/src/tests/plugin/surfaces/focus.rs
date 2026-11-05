use crate::kagan::screen_read;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, Focusable as _, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{Op, PluginId, PluginOp};
use serde_json::json;

use super::{
    BLANK, FILL, assert_replaced, body, close_surface, emit_open, height_of, install_blank,
    install_fill, install_pair, item, open_surface, run, set_plugin, show_thread, surfaces, tab_to,
    terminal_focused, wide,
};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, present, press_key, repo_thread, run_shell,
    shows, wait_until,
};

const ALPHA: &str = "dev.test.alpha";
const BETA: &str = "dev.test.beta";

pub(in crate::tests) fn deliver_mark(cx: &mut TestAppContext, mark: Option<&str>) {
    let plugin = PluginId(FILL.into());
    let props = mark
        .map(|mark| json!({ "mark": mark }))
        .unwrap_or(json!(null));
    cx.update(|cx| {
        let window = cx.windows()[0].window_id();
        let ui = kg_plugins::HostUi::global(cx).expect("host");
        ui.update(cx, |ui, _| ui.deliver(window, &plugin, props));
    });
    cx.run_until_parked();
}

pub(in crate::tests) fn plugin_generation(cx: &mut TestAppContext, id: &str) -> Option<u64> {
    cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .generation(&PluginId(id.into()))
    })
}

#[gpui_kit::test]
fn list_follows_a_panel_that_fills_and_empties_in_place(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_fill);
    wide(cx, window);
    cx.run_until_parked();
    deliver_mark(cx, Some("filled"));
    open_surface(cx, FILL, "side").unwrap();
    let side = body(FILL, "side");
    wait_until(cx, |cx| shows(cx, window, "fill-side"));
    assert!(item(&surfaces(cx), FILL, "side").open);
    assert!(item(&surfaces(cx), FILL, "side").visible);
    let generation = plugin_generation(cx, FILL);

    deliver_mark(cx, Some("filled"));
    assert!(item(&surfaces(cx), FILL, "side").visible);
    assert!(height_of(cx, window, &side) > 0.);
    assert_eq!(label(cx, window, "fill-side"), "filled");
    assert_eq!(plugin_generation(cx, FILL), generation);

    deliver_mark(cx, None);
    assert!(!item(&surfaces(cx), FILL, "side").visible);
    assert!(height_of(cx, window, &side) <= 0.);
    assert_eq!(plugin_generation(cx, FILL), generation);

    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(FILL.into()),
            thread_id: None,
        }),
    )
    .unwrap();
    cx.run_until_parked();
    let reloaded = plugin_generation(cx, FILL);
    assert_ne!(reloaded, generation);
    assert!(!item(&surfaces(cx), FILL, "side").visible);
    assert!(height_of(cx, window, &side) <= 0.);

    deliver_mark(cx, Some("filled"));
    assert!(item(&surfaces(cx), FILL, "side").visible);
    assert!(height_of(cx, window, &side) > 0.);
    assert_eq!(plugin_generation(cx, FILL), reloaded);
    deliver_mark(cx, None);
    assert!(!item(&surfaces(cx), FILL, "side").visible);
    assert!(height_of(cx, window, &side) <= 0.);
    assert_eq!(plugin_generation(cx, FILL), reloaded);
}

pub(in crate::tests) fn disable_beta(cx: &mut TestAppContext) {
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(BETA.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn focus_tool_button(cx: &mut TestAppContext, window: AnyWindowHandle) {
    open_surface(cx, ALPHA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-tool-count"));
    tab_to(cx, window, "alpha-tool-count");
}

#[gpui_kit::test]
fn the_only_left_tool_focuses_a_trigger_that_is_on_screen(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    show_thread(cx, window);
    disable_beta(cx);
    close_keeps_the_tools_trigger(cx, window);
    disable_moves_the_only_tool_to_the_terminal(cx, window, &kagan);
    settings_keeps_its_focus_when_the_only_tool_goes(cx, window, &kagan);
    a_hidden_sidebar_does_not_focus_tools(cx, window);
    terminal_focus_stays_when_the_only_tool_goes(cx, window, &kagan);
    removal_of_the_only_tool_focuses_the_terminal(cx, window, &kagan);
}

pub(in crate::tests) fn close_keeps_the_tools_trigger(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
) {
    focus_tool_button(cx, window);
    close_surface(cx, ALPHA, "tool").unwrap();
    let tool = format!("surface-item-{ALPHA}-tool");
    assert!(shows(cx, window, &tool));
    assert_eq!(focused_of(cx, window, &tool), Some(true));
}

pub(in crate::tests) fn disable_moves_the_only_tool_to_the_terminal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    focus_tool_button(cx, window);
    set_plugin(cx, false);
    assert!(!present(cx, window, &format!("surface-item-{ALPHA}-tool")));
    assert!(terminal_focused(cx, window, kagan));
}

pub(in crate::tests) fn settings_keeps_its_focus_when_the_only_tool_goes(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    set_plugin(cx, true);
    focus_tool_button(cx, window);
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
    })
    .unwrap();
    assert!(!terminal_focused(cx, window, kagan));
    set_plugin(cx, false);
    assert!(!terminal_focused(cx, window, kagan));
    assert!(!present(cx, window, &format!("surface-item-{ALPHA}-tool")));
}

pub(in crate::tests) fn a_hidden_sidebar_does_not_focus_tools(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
) {
    set_plugin(cx, true);
    focus_tool_button(cx, window);
    click(cx, window, "left-sidebar-toggle");
    let tool = format!("surface-item-{ALPHA}-tool");
    assert!(shows(cx, window, &tool));
    set_plugin(cx, false);
    assert_ne!(focused_of(cx, window, &tool), Some(true));
    click(cx, window, "left-sidebar-toggle");
}

pub(in crate::tests) fn terminal_focus_stays_when_the_only_tool_goes(
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

pub(in crate::tests) fn open_changed_file(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    data: &Path,
) {
    repo_thread(cx, window, data);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
}

#[gpui_kit::test]
fn a_page_and_a_review_take_turns_in_the_center(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_changed_file(cx, window, data.path());
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), ALPHA, "page").open);

    open_surface(cx, ALPHA, "page").unwrap();
    assert!(shows(cx, window, "surface-page"));
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
    assert!(!shows(cx, window, "review"));

    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.open_review("notes.txt".into(), window, cx);
        });
    })
    .unwrap();
    wait_until(cx, |cx| shows(cx, window, "review"));
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), ALPHA, "page").open);

    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    assert!(!shows(cx, window, "review"));
    press_key(cx, window, "escape");
    assert!(!shows(cx, window, "review"));
    assert!(!shows(cx, window, "surface-page"));
}

#[gpui_kit::test]
fn a_pending_page_yields_to_a_review(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    let reply = cx
        .update_window(window, |_, window, cx| {
            let incoming = emit_open(cx, ALPHA, "page", "lost");
            let kagan = kagan.clone();
            window.defer(cx, move |window, cx| {
                kagan.update(cx, |kagan, cx| {
                    kagan.open_review("notes.txt".into(), window, cx);
                });
            });
            incoming
        })
        .unwrap();
    cx.run_until_parked();
    assert_replaced(&reply);
    assert!(shows(cx, window, "review"));
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), ALPHA, "page").open);
}

#[gpui_kit::test]
fn a_failed_page_open_keeps_the_review(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_blank);
    wide(cx, window);
    open_changed_file(cx, window, data.path());
    // The page body takes whatever height the center has left, so a short window is the miss.
    cx.simulate_window_resize(window, size(px(800.), px(40.)));
    cx.run_until_parked();
    let missed = open_surface(cx, BLANK, "page").unwrap_err();
    assert!(missed.to_string().contains("not on screen"), "{missed}");
    wide(cx, window);
    assert!(shows(cx, window, "review"));
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), BLANK, "page").open);
}

#[gpui_kit::test]
fn a_tool_and_a_side_leave_the_review_open(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_changed_file(cx, window, data.path());
    open_surface(cx, ALPHA, "tool").unwrap();
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, &body(ALPHA, "side")));
    assert!(shows(cx, window, "right-overlay-panel"));
    assert!(shows(cx, window, "review"));
    assert!(shows(cx, window, &body(ALPHA, "tool")));
}

#[gpui_kit::test]
fn a_replaced_page_open_does_not_restore_the_review(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_changed_file(cx, window, data.path());
    let second = run_shell(cx);
    let reply = cx
        .update_window(window, |_, _window, cx| {
            let incoming = emit_open(cx, ALPHA, "page", "lost");
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
    assert!(!shows(cx, window, "review"));
    assert!(!shows(cx, window, "surface-page"));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(second)
    );
}

pub(in crate::tests) fn removal_of_the_only_tool_focuses_the_terminal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    set_plugin(cx, true);
    focus_tool_button(cx, window);
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
    assert!(!present(cx, window, &format!("surface-item-{ALPHA}-tool")));
    assert!(terminal_focused(cx, window, kagan));
}
