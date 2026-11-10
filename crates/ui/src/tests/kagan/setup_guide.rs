use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, PreferenceOp};
use serde_json::Value;

use crate::kagan::Kagan;
use crate::tests::kagan::{
    add_project, click, open, open_empty, open_with_plugins_then, setup_with, shell, shows,
    wait_until,
};
use crate::tests::settings::page::install_plugins;

const KEY: &str = "setup-guide";

fn stored(cx: &mut TestAppContext) -> Value {
    let read = cx.update(|cx| {
        Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: KEY.into() }), cx)
    });
    let OpResult::Setting(value) = cx.foreground_executor().block_test(read).unwrap() else {
        panic!("a preference value")
    };
    value
}

fn replay(cx: &mut TestAppContext, kagan: &Entity<Kagan>, window: AnyWindowHandle) {
    let _ = kagan;
    click(cx, window, "settings-gear");
    click(cx, window, "setup-guide-show");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_fresh_window_shows_the_guide_and_stores_that_it_is_shown(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_empty(cx);
    assert!(shows(cx, window, "setup-guide"));
    assert!(shows(cx, window, "setup-guide-step-2"));
    assert!(shows(cx, window, "setup-guide-step-3"));
    assert_eq!(stored(cx), Value::Bool(true));
}

// A data folder from before the guide existed: a standard project already holds work.
fn open_upgrader(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let data = setup_with(cx, false);
    let folder = data.path().join("work");
    std::fs::create_dir_all(&folder).unwrap();
    add_project(cx, &folder);
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    (kagan, window, data)
}

#[gpui_kit::test]
fn an_upgrader_with_a_project_never_sees_the_guide(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_upgrader(cx);
    assert!(!shows(cx, window, "setup-guide"));
    assert_eq!(stored(cx), Value::Bool(false));
}

#[gpui_kit::test]
fn hide_stores_the_choice_and_the_guide_stays_hidden(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_empty(cx);
    click(cx, window, "setup-guide-dismiss");
    cx.run_until_parked();
    assert!(!shows(cx, window, "setup-guide"));
    assert_eq!(stored(cx), Value::Bool(false));
}

#[gpui_kit::test]
fn replay_brings_the_guide_back_after_hide(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_empty(cx);
    click(cx, window, "setup-guide-dismiss");
    cx.run_until_parked();
    replay(cx, &kagan, window);
    assert!(shows(cx, window, "setup-guide"));
}

#[gpui_kit::test]
fn adding_a_project_marks_step_two_done(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_empty(cx);
    let folder = data.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    add_project(cx, &folder);
    wait_until(cx, |cx| shows(cx, window, "setup-guide-done-2"));
    assert!(!shows(cx, window, "setup-guide-step-2"));
}

#[gpui_kit::test]
fn starting_a_thread_marks_step_three_done(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_empty(cx);
    let folder = data.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project = add_project(cx, &folder);
    let run = kg_domain::Run {
        project_id: project.id,
        program: shell().into(),
        title: None,
        environment: kg_domain::EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started =
        cx.update(|cx| Workspace::dispatch(Op::Thread(kg_domain::ThreadOp::Run(run)), cx));
    cx.foreground_executor().block_test(started).unwrap();
    wait_until(cx, |cx| shows(cx, window, "setup-guide-done-3"));
}

#[gpui_kit::test]
fn an_installed_plugin_marks_step_four_done(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    replay(cx, &kagan, window);
    assert!(shows(cx, window, "setup-guide-done-4"));
}

#[gpui_kit::test]
fn general_settings_has_the_command_line_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::scroll_within(
            window,
            &gpui_kit::ElementId::Name("settings-page".into()),
            None,
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-3000.))),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    cx.run_until_parked();
    assert!(shows(cx, window, "setting-row-kg-command"));
}
