use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{
    InteractionOption, Op, OpResult, PreferenceOp, ProjectId, SettingsOp, ThreadId, ThreadOp,
};
use serde_json::json;

use crate::tests::sidebar::layout::focus_row;

use super::{
    click, focused_of, open, row_top, run_op, run_shell, selected_of, set_preference, shows,
    thread_record, wait_until,
};

fn later(cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_millis(50));
}

fn above(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    higher: &ThreadId,
    lower: &ThreadId,
) -> bool {
    let higher = format!("thread-{}", higher.0);
    let lower = format!("thread-{}", lower.0);
    shows(cx, window, &higher)
        && shows(cx, window, &lower)
        && row_top(cx, window, &higher) < row_top(cx, window, &lower)
}

fn ask(cx: &mut TestAppContext, id: &ThreadId) {
    later(cx);
    run_op(
        cx,
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Still there?".into(),
            options: vec![
                InteractionOption {
                    id: "yes".into(),
                    label: "Yes".into(),
                    role: None,
                },
                InteractionOption {
                    id: "no".into(),
                    label: "No".into(),
                    role: None,
                },
            ],
            fields: None,
        }),
    );
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_list_menu_hides_an_empty_project(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    run_shell(cx);
    assert!(shows(cx, window, "project-personal"));
    assert!(shows(cx, window, "project-prj_kagan"));

    click(cx, window, "sidebar-list-options");
    assert_eq!(
        selected_of(cx, window, "sidebar-hide-empty-projects"),
        Some(false)
    );
    click(cx, window, "sidebar-hide-empty-projects");
    wait_until(cx, |cx| !shows(cx, window, "project-personal"));
    assert!(shows(cx, window, "project-prj_kagan"));
}

#[gpui_kit::test]
fn the_chosen_project_stays_when_empty_projects_are_hidden(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "project-prj_kagan");
    cx.run_until_parked();

    click(cx, window, "sidebar-list-options");
    click(cx, window, "sidebar-hide-empty-projects");
    wait_until(cx, |cx| !shows(cx, window, "project-personal"));
    assert!(shows(cx, window, "project-prj_kagan"));
}

#[gpui_kit::test]
fn the_project_page_on_screen_stays_when_empty_projects_are_hidden(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    run_op(
        cx,
        Op::Settings(SettingsOp::Project {
            id: ProjectId("personal".into()),
            section: None,
        }),
    );
    cx.run_until_parked();

    click(cx, window, "sidebar-list-options");
    click(cx, window, "sidebar-hide-empty-projects");
    cx.run_until_parked();
    assert!(shows(cx, window, "project-personal"));
}

#[gpui_kit::test]
fn the_list_options_menu_opens_from_the_keyboard(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    focus_row(cx, window, "sidebar-list-options");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(selected_of(cx, window, "sidebar-order-current"), Some(true));
}

#[gpui_kit::test]
fn escape_from_the_list_options_menu_returns_focus_to_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    focus_row(cx, window, "sidebar-list-options");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(shows(cx, window, "sidebar-list-menu"));

    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "sidebar-list-menu"));
    assert_eq!(focused_of(cx, window, "sidebar-list-options"), Some(true));
}

#[gpui_kit::test]
fn arrow_keys_on_the_list_options_button_keep_focus_on_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    focus_row(cx, window, "sidebar-list-options");
    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    assert_eq!(focused_of(cx, window, "sidebar-list-options"), Some(true));
    cx.simulate_keystrokes(window, "up");
    cx.run_until_parked();
    assert_eq!(focused_of(cx, window, "sidebar-list-options"), Some(true));
}

#[gpui_kit::test]
fn attention_and_unread_reorder_threads_and_pinned_stays_first(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let older = run_shell(cx);
    later(cx);
    let newer = run_shell(cx);
    wait_until(cx, |cx| above(cx, window, &newer, &older));

    click(cx, window, "sidebar-list-options");
    assert_eq!(selected_of(cx, window, "sidebar-order-current"), Some(true));
    click(cx, window, "sidebar-order-attention");
    wait_until(cx, |cx| {
        matches!(
            run_op(
                cx,
                Op::Preference(PreferenceOp::Get {
                    key: kg_domain::prefs::SIDEBAR_ORDER.into(),
                }),
            ),
            OpResult::Setting(value) if value == json!("attention")
        )
    });

    ask(cx, &older);
    assert!(thread_record(cx, &older).unread());
    assert!(
        thread_record(cx, &older).latest_attention_at
            > thread_record(cx, &newer).latest_attention_at
    );
    wait_until(cx, |cx| above(cx, window, &older, &newer));

    set_preference(cx, kg_domain::prefs::SIDEBAR_ORDER, json!("current"));
    wait_until(cx, |cx| above(cx, window, &newer, &older));

    set_preference(cx, kg_domain::prefs::SIDEBAR_UNREAD_FIRST, json!(true));
    wait_until(cx, |cx| above(cx, window, &older, &newer));

    run_op(
        cx,
        Op::Thread(ThreadOp::SetPinned {
            id: newer.clone(),
            pinned: true,
        }),
    );
    cx.run_until_parked();
    wait_until(cx, |cx| above(cx, window, &newer, &older));
}
