use gpui_kit::{AppContext as _, TestAppContext, px};
use kg_core::Workspace;
use kg_domain::{Op, SettingsOp, UiOp};
use serde_json::{Value, json};

use super::{aria_label, pane_width, run_op, saved, write_plugin};
use crate::tests::kagan::{
    chord, click, open, open_with_plugins_then, run_shell, set_preference, shows,
};

#[gpui_kit::test]
fn a_long_note_above_a_settings_row_does_not_block_its_click(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        let parent = "a-long-plugin-source-name".repeat(6);
        std::fs::create_dir(data.join(&parent)).unwrap();
        write_plugin(
            data,
            &format!("{parent}/plugin"),
            &json!({
                "id": "dev.test.long-note", "name": "Long note", "version": "1.0.0",
                "kagan": ">=0.1.0", "entry": "main.js", "instructions": "Test.", "powers": {}
            }),
            "export default class Plugin {}\n",
        );
    });
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-details-dev.test.long-note");
    let note =
        aria_label(cx, window, "setting-row-dev.test.long-note-details-note").unwrap_or_default();
    assert!(note.len() > 100, "{note}");
    run_op(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: "plugin-details-dev.test.long-note".into(),
            within: None,
            dy: 10_000.,
            window: 0,
        }),
    );
    assert!(!shows(cx, window, "plugin-install-open"));
    run_op(
        cx,
        Op::Ui(UiOp::Click {
            element_id: "plugin-install-open".into(),
            within: None,
            window: 0,
        }),
    );
    cx.run_until_parked();
    assert!(shows(cx, window, "plugin-source"));
}

#[gpui_kit::test]
fn kg_settings_open_shows_the_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let open = cx.update(|cx| Workspace::dispatch(Op::Settings(SettingsOp::Open), cx));
    cx.foreground_executor().block_test(open).unwrap();
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn entering_settings_hides_both_sidebars(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "needs-you"));
    click(cx, window, "settings-gear");
    assert!(
        !shows(cx, window, "project-prj_kagan"),
        "threads leave the sidebar"
    );
    assert!(
        shows(cx, window, "settings-page"),
        "the settings page fills the window"
    );
    assert!(!shows(cx, window, "needs-you"), "the right sidebar hides");
}

#[gpui_kit::test]
fn leaving_settings_restores_both_sidebars(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn the_right_sidebar_comes_back_as_it_was(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    assert!(shows(cx, window, "needs-you"));
    click(cx, window, "right-sidebar-toggle");
    assert!(!shows(cx, window, "needs-you"));
    click(cx, window, "settings-gear");
    click(cx, window, "settings-close");
    assert!(
        !shows(cx, window, "needs-you"),
        "still hidden, as the person left it"
    );
    assert!(shows(cx, window, "project-prj_kagan"));
}

#[gpui_kit::test]
fn sidebar_toggles_do_nothing_in_settings(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert_eq!(
        saved(cx, "main.left-sidebar"),
        Value::Null,
        "nothing is saved"
    );
    assert_eq!(
        saved(cx, "main.right-sidebar"),
        Value::Null,
        "nothing is saved"
    );
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn a_width_dragged_in_settings_is_not_kept(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let before = pane_width(cx, window);
    click(cx, window, "settings-gear");
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_left(px(330.), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        (pane_width(cx, window) - before).abs() < 1.,
        "leaving restores the width"
    );
    assert_eq!(
        saved(cx, "main.left-width"),
        Value::Null,
        "nothing is saved"
    );
}

#[gpui_kit::test]
fn selecting_a_thread_leaves_settings(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    let id = run_shell(cx);
    click(cx, window, "settings-gear");
    let select = id.clone();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.select(select, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
    assert!(shows(cx, window, "needs-you"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn the_shown_thread_is_not_on_screen_under_settings(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), None);
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn a_preference_set_by_kg_reaches_the_page(cx: &mut TestAppContext) {
    let (kagan, _window, _data) = open(cx);
    assert!(cx.update(|cx| kagan.read(cx).agent_integration(cx)));
    set_preference(cx, "agent-integration", json!(false));
    assert!(!cx.update(|cx| kagan.read(cx).agent_integration(cx)));
}
