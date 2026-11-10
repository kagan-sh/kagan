use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};

use serde_json::json;

use kg_domain::{Op, PluginId, PluginOp};
use kg_plugins::PluginHost;

use super::{install_plugins, run_op, saved, tab_to};
use crate::tests::kagan::{click, open, open_with_plugins_then, present, selected_of, shows};

#[gpui_kit::test]
fn picking_a_section_shows_only_its_rows(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-theme"));
    assert!(present(cx, window, "setting-row-mode"));
    assert!(!present(cx, window, "setting-row-notify"));

    click(cx, window, "settings-section-general");
    assert!(present(cx, window, "setting-row-notify"));
    assert!(!present(cx, window, "setting-row-theme"));
}

#[gpui_kit::test]
fn search_finds_a_row_in_another_section_and_jumps_there(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "text size");
    cx.run_until_parked();
    assert!(present(cx, window, "settings-result-appearance-0"));
    assert!(!present(cx, window, "settings-section-general"));
    assert!(
        present(cx, window, "setting-row-notify"),
        "the chosen section stays on screen while searching"
    );

    click(cx, window, "settings-result-appearance-0");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-terminal-font-size"));
    assert!(present(cx, window, "settings-section-general"));
}

#[gpui_kit::test]
fn enter_in_the_search_box_opens_the_first_result(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "text size");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-terminal-font-size"));
}

#[gpui_kit::test]
fn arrows_move_the_section_list_and_enter_opens_a_section(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_keystrokes(window, "tab");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "down");
    cx.simulate_keystrokes(window, "down");
    cx.simulate_keystrokes(window, "up");
    cx.run_until_parked();
    assert!(!present(cx, window, "setting-row-theme"));

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-theme"));
}

#[gpui_kit::test]
fn a_plugin_with_settings_is_listed_under_plugins(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(present(
        cx,
        window,
        "settings-section-plugin-dev.test.prefs"
    ));

    click(cx, window, "settings-section-plugin-dev.test.prefs");
    assert!(present(cx, window, "setting-row-dev.test.prefs/sounds"));
    assert!(!present(cx, window, "setting-row-notify"));
}

#[gpui_kit::test]
fn the_ask_before_archiving_switch_saves_the_preference(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-general");
    assert!(present(cx, window, "setting-row-confirm-archive"));
    click(cx, window, "confirm-archive");
    assert_eq!(saved(cx, "confirm-archive"), json!(true));
    click(cx, window, "confirm-archive");
    assert_eq!(saved(cx, "confirm-archive"), json!(false));
}

#[gpui_kit::test]
fn enter_after_typing_opens_the_first_result_not_the_old_cursor(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-updates");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "a");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-general"),
        Some(true)
    );
}

#[gpui_kit::test]
fn down_in_the_search_box_moves_to_the_next_result(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "text size");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-keyboard"),
        Some(true)
    );
}

#[gpui_kit::test]
fn a_plugin_section_comes_back_after_its_plugin_reloads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    cx.update(|cx| {
        PluginHost::global(cx).update(cx, |host, cx| {
            host.disable_loaded(&PluginId("dev.test.prefs".into()), cx);
        });
    });
    cx.run_until_parked();
    assert!(present(cx, window, "setting-row-notify"));

    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId("dev.test.prefs".into()),
            thread_id: None,
        }),
    );
    cx.run_until_parked();
    assert!(present(cx, window, "setting-row-dev.test.prefs/sounds"));
}

#[gpui_kit::test]
fn a_result_in_a_lower_group_scrolls_into_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    assert!(!shows(cx, window, "setting-row-FindInDiff"));
    click(cx, window, "settings-search");
    cx.simulate_input(window, "find in diff");
    cx.run_until_parked();
    click(cx, window, "settings-result-shortcuts-review-0");
    cx.run_until_parked();
    assert!(shows(cx, window, "setting-row-FindInDiff"));
}

#[gpui_kit::test]
fn the_chosen_section_is_kept_when_settings_reopens(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
}

fn height_of(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into()))
            .map(|element| f32::from(element.bounds.size.height))
    })
    .unwrap()
    .unwrap()
}

#[gpui_kit::test]
fn the_cursor_ring_does_not_change_a_row_height(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_keystrokes(window, "tab");
    cx.run_until_parked();
    let at_rest = height_of(cx, window, "settings-section-appearance");
    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    let with_ring = height_of(cx, window, "settings-section-appearance");
    assert_eq!(at_rest, with_ring, "the ring moved the row");
}

fn bounds_of(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> (f32, f32, f32, f32) {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into())).map(|element| {
            let bounds = element.bounds;
            (
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            )
        })
    })
    .unwrap()
    .unwrap()
}

#[gpui_kit::test]
fn the_first_group_starts_one_page_gap_below_the_top(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let (_, page_top, _, _) = bounds_of(cx, window, "settings-page");
    let (_, group_top, _, _) = bounds_of(cx, window, "settings-group-agents");
    let gap = cx.update(|cx| crate::theme::space(crate::theme::SpaceStep::Xl, cx));
    assert!(
        (group_top - page_top - gap).abs() < 0.5,
        "first group at {} below the top, expected {gap}",
        group_top - page_top
    );
}

#[gpui_kit::test]
fn a_note_runs_past_half_the_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-general");
    let (_, _, row_width, _) = bounds_of(cx, window, "setting-row-notify");
    let (_, _, note_width, _) = bounds_of(cx, window, "setting-row-notify-note");
    assert!(
        note_width > row_width * 0.6,
        "the note is {note_width} px of a {row_width} px row"
    );
}

#[gpui_kit::test]
fn a_plain_row_is_inset_16_by_12(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-general");
    let (row_x, row_y, _, _) = bounds_of(cx, window, "setting-row-notify");
    let (title_x, title_y, _, _) = bounds_of(cx, window, "setting-row-notify-title");
    let (across, down) = cx.update(|cx| {
        (
            crate::theme::space(crate::theme::SpaceStep::Lg, cx),
            crate::theme::space(crate::theme::SpaceStep::Md, cx),
        )
    });
    assert!(
        (title_x - row_x - across).abs() < 0.5,
        "across {}",
        title_x - row_x
    );
    assert!(
        (title_y - row_y - down).abs() < 0.5,
        "down {}",
        title_y - row_y
    );
}

#[gpui_kit::test]
fn the_title_and_the_search_box_share_a_left_edge(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let (title_x, _, _, _) = bounds_of(cx, window, "settings-title");
    let (search_x, _, _, _) = bounds_of(cx, window, "settings-search");
    assert!(
        (title_x - search_x).abs() < 0.5,
        "title {title_x}, search {search_x}"
    );
}

#[gpui_kit::test]
fn a_one_group_section_has_no_repeated_heading(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-updates");
    assert!(!present(cx, window, "settings-heading-updates"));
}

#[gpui_kit::test]
fn the_settings_list_and_search_report_their_roles(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let role = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("settings-sections".into()))
                .and_then(|element| element.role)
        })
        .unwrap();
    assert_eq!(role, Some(gpui_kit::Role::List));
}
