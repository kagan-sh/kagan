use std::path::Path;
use std::time::Duration;

use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::{ActiveTheme as _, ThemeMode};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, InputEvent as _, Modifiers, MouseMoveEvent,
    Pixels, Point, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase, point, px, size,
};
use kg_core::Workspace;
use kg_domain::{
    Appearance, Color, ConfigOp, KeymapOp, Op, OpResult, Plugin, PluginId, PluginOp, SettingsOp,
    ThemeFile, ThemeOp, UiOp,
};
use serde_json::{Value, json};

use crate::tests::kagan::{
    NAVIGATOR, add_project, chord, click, drag_to, focused_of, install_navigator, label, open,
    open_with_plugins_then, plugin_log, present, press as press_at, press_key, release, run_shell,
    selected_of, set_preference, shows, value_of, wait_until,
};
use crate::tests::sidebar::layout::span;
use crate::theme::follow_system;

const PREFS: &str = "dev.test.prefs";
const VIEW: &str = "dev.test.view";
const OTHER: &str = "dev.test.other";
const OLD: &str = "dev.test.old";

#[gpui_kit::test]
fn system_mode_follows_the_operating_system(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    assert!(
        !cx.update(|cx| cx.theme().is_dark()),
        "a light system paints light"
    );

    cx.update(|cx| follow_system(ThemeMode::Dark, cx));
    cx.run_until_parked();
    assert!(
        cx.update(|cx| cx.theme().is_dark()),
        "the window follows a switch to dark"
    );

    cx.update(|cx| follow_system(ThemeMode::Light, cx));
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.theme().is_dark()), "and back to light");
}

#[gpui_kit::test]
fn a_fixed_mode_ignores_the_operating_system(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    set_mode(cx, Appearance::Dark);
    cx.update(|cx| follow_system(ThemeMode::Light, cx));
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.theme().is_dark()), "dark stays dark");

    set_mode(cx, Appearance::System);
    assert!(
        !cx.update(|cx| cx.theme().is_dark()),
        "system takes the light system back"
    );
}

#[gpui_kit::test]
fn settings_opens_from_its_shortcut_and_escape_goes_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn the_gear_opens_and_closes_the_settings_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));
    click(cx, window, "settings-close");
    assert!(!shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn the_gear_leaves_settings_when_clicked_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));
    click(cx, window, "settings-gear");
    assert!(!shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn settings_is_one_page_and_manage_plugins_scrolls_to_installed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "setting-row-theme"));
    assert!(!shows(cx, window, "settings-search"));
    assert!(!shows(cx, window, "settings-menu"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    click(cx, window, "plugins-manage");
    assert!(shows(cx, window, "setting-row-install"));
}

#[gpui_kit::test]
fn settings_saves_terminal_size_and_notifications(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "terminal-size-bigger");
    wait_until(cx, |cx| saved(cx, "terminal-font-size") == json!(13));
    click(cx, window, "notify");
    wait_until(cx, |cx| saved(cx, "notify") == json!(false));
    assert_eq!(value_of(cx, window, "notify").as_deref(), Some("off"));
}

#[gpui_kit::test]
fn clicking_a_setting_out_of_view_scrolls_to_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
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
fn scrolling_an_element_out_of_view_first_brings_it_into_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    assert!(!shows(cx, window, "plugin-install-open"));
    run_op(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: "plugin-install-open".into(),
            within: None,
            dy: 0.,
            window: 0,
        }),
    );
    assert!(shows(cx, window, "plugin-install-open"));
}

#[gpui_kit::test]
fn the_settings_scrollbar_hugs_the_window_edge(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    for width in [1100., 1500.] {
        cx.simulate_window_resize(window, size(px(width), px(720.)));
        cx.run_until_parked();
        let (center_x, center_width) = span(cx, window, "center").unwrap();
        let edge = center_x + center_width;
        let (close_x, close_width) = span(cx, window, "settings-close").unwrap();
        let (_, column) = span(cx, window, "settings-group-appearance").unwrap();
        let gap = (center_width - column) / 2.;
        assert!(
            (edge - close_x - close_width - gap).abs() < 1.,
            "the column is not centered at {width}"
        );
        // gpui-kit draws the bar along the right side of the element that scrolls.
        let bar = point(px(edge) - Scrollbar::width() / 2., px(300.));
        wheel(cx, window, bar, 10_000.);
        let top = top_of(cx, window, "setting-row-theme");
        wheel(cx, window, bar, -50.);
        assert!(
            (top_of(cx, window, "setting-row-theme") - (top - 50.)).abs() < 0.5,
            "the page does not scroll under its bar at {width}"
        );
    }
    wheel(cx, window, point(px(700.), px(300.)), 10_000.);
    assert!(!shows(cx, window, "plugin-install-open"));
    run_op(
        cx,
        Op::Ui(UiOp::Click {
            element_id: "plugin-install-open".into(),
            within: None,
            window: 0,
        }),
    );
    assert!(shows(cx, window, "plugin-source"));
}

#[gpui_kit::test]
fn the_settings_thumb_follows_the_page_down(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    cx.simulate_window_resize(window, size(px(1100.), px(720.)));
    cx.run_until_parked();
    let (center_x, center_width) = span(cx, window, "center").unwrap();
    let x = px(center_x + center_width) - Scrollbar::width() / 2.;
    let bottom = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("settings-page".into()))
                .map(|page| page.bounds.bottom())
        })
        .unwrap()
        .unwrap();
    wheel(cx, window, point(x, px(300.)), -10_000.);
    let top = top_of(cx, window, "setting-row-theme");
    let grab = point(x, bottom - px(10.));
    let drop = grab - point(px(0.), px(100.));
    press_at(cx, window, grab, 1, Modifiers::default());
    drag_to(cx, window, drop);
    release(cx, window, drop, 1, Modifiers::default());
    cx.run_until_parked();
    assert!(
        top_of(cx, window, "setting-row-theme") > top + 100.,
        "the thumb is not at the bottom of the bar once the page is scrolled down"
    );
}

fn top_of(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into()))
            .map(|element| f32::from(element.bounds.origin.y))
    })
    .unwrap()
    .unwrap()
}

fn wheel(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>, dy: f32) {
    let event = ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: gpui_kit::ScrollSource::Pointer,
    };
    let hover = MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::default(),
    };
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(hover.to_platform_input(), cx);
        crate::ui_dispatch::render_frame(window, cx);
        window.dispatch_event(event.to_platform_input(), cx);
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

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

#[gpui_kit::test]
fn agent_integration_saved_as_off_shows_off_at_start(cx: &mut TestAppContext) {
    let _data = setup_only(cx);
    set_preference(cx, "agent-integration", json!(false));
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    assert!(!cx.update(|cx| kagan.read(cx).agent_integration(cx)));
}

#[gpui_kit::test]
fn a_theme_card_reports_whether_it_is_the_chosen_theme(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let before = get_theme(cx).theme;
    assert_eq!(
        selected_of(cx, window, &format!("theme-{before}")),
        Some(true)
    );

    if before == "ember" {
        return;
    }
    click(cx, window, "theme-ember");
    assert_eq!(selected_of(cx, window, "theme-ember"), Some(true));
    assert_eq!(
        selected_of(cx, window, &format!("theme-{before}")),
        Some(false)
    );
}

#[gpui_kit::test]
fn the_theme_picker_saves_the_theme(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "theme-ember");
    assert_eq!(get_theme(cx).theme, "ember");
}

#[gpui_kit::test]
fn the_mode_choice_saves_the_mode(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "mode-dark");
    assert_eq!(get_theme(cx).mode, Appearance::Dark);
    click(cx, window, "mode-system");
    assert_eq!(get_theme(cx).mode, Appearance::System);
}

#[gpui_kit::test]
fn a_plugin_setting_saves_from_its_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(present(cx, window, "setting-row-dev.test.prefs/sounds"));

    click(cx, window, "plugin-dev.test.prefs/sounds");
    assert_eq!(plugin_config(cx, "sounds"), json!(false));

    click(cx, window, "option-dev.test.prefs/density/compact");
    assert_eq!(plugin_config(cx, "density"), json!("compact"));
}

#[gpui_kit::test]
fn a_project_setting_picks_from_a_dropdown(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "None"
    );
    click(cx, window, "setting-project-dev.test.prefs/board");
    wait_until(cx, |cx| {
        shows(cx, window, "project-pick-dev.test.prefs/board/prj_kagan")
    });
    click(cx, window, "project-pick-dev.test.prefs/board/prj_kagan");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "board"), json!("prj_kagan"));
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "Kagan project"
    );
    click(cx, window, "setting-project-dev.test.prefs/board");
    wait_until(cx, |cx| {
        shows(cx, window, "project-none-dev.test.prefs/board")
    });
    click(cx, window, "project-none-dev.test.prefs/board");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "board"), Value::Null);
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "None"
    );
}

#[gpui_kit::test]
fn a_missing_project_setting_keeps_its_id(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    store
        .set_plugin_value(&PluginId(PREFS.into()), "board", &json!("prj_gone"))
        .unwrap();
    drop(store);
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
    click(cx, window, "settings-gear");
    assert_eq!(plugin_config(cx, "board"), json!("prj_gone"));
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "Missing"
    );
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/board-note").unwrap_or_default();
    assert!(
        note.contains("gone") && note.contains("Pick another"),
        "{note}"
    );
}

#[gpui_kit::test]
fn a_project_menu_scrolls_to_its_last_of_25_projects(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    let mut last = None;
    for n in 0..25 {
        let folder = data.path().join(format!("project-{n:02}"));
        std::fs::create_dir(&folder).unwrap();
        last = Some(add_project(cx, &folder).id);
    }
    let last = last.unwrap();
    cx.simulate_window_resize(window, size(px(1100.), px(420.)));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "setting-project-dev.test.prefs/board");
    let pick = format!("project-pick-dev.test.prefs/board/{}", last.0);
    wait_until(cx, |cx| {
        shows(cx, window, "project-none-dev.test.prefs/board")
    });
    assert!(
        !shows(cx, window, &pick),
        "the last project starts below the short window"
    );
    run_op(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: pick.clone(),
            within: None,
            dy: 0.,
            window: 0,
        }),
    );
    wait_until(cx, |cx| shows(cx, window, &pick));
    let (row_bottom, viewport) = cx
        .update_window(window, |_, window, _| {
            let row = crate::ui_registry::find(window, &ElementId::Name(pick.clone().into()))
                .unwrap()
                .bounds;
            (row.bottom(), window.viewport_size().height)
        })
        .unwrap();
    assert!(
        row_bottom <= viewport,
        "the revealed row sits inside the window: bottom {row_bottom:?}, window {viewport:?}"
    );
    click(cx, window, &pick);
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "board"), json!(last.0));
}

#[gpui_kit::test]
fn removing_the_project_while_settings_is_open_shows_missing(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    let folder = data.path().join("removed-project");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "setting-project-dev.test.prefs/board");
    let pick = format!("project-pick-dev.test.prefs/board/{}", project.id.0);
    wait_until(cx, |cx| shows(cx, window, &pick));
    click(cx, window, &pick);
    cx.run_until_parked();
    let removal = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Remove {
                id: project.id.clone(),
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().next().is_some())
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(removal).unwrap();
    cx.run_until_parked();
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "Missing"
    );
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/board-note").unwrap_or_default();
    assert!(note.contains("gone"), "{note}");
}

#[gpui_kit::test]
fn review_number_type_after_reload(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_string_retries);
    cx.run_until_parked();
    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/retries");
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "nope");
    cx.run_until_parked();
    set_retries_type(data.path(), "number");
    reload_prefs(cx);
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "4");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "retries"), json!(4));
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store
            .plugin_value(&PluginId(PREFS.into()), "retries")
            .unwrap(),
        Some(json!(4)),
        "the saved value is a JSON number, not text"
    );
}

fn install_string_retries(data: &Path) {
    install_plugins(data);
    set_retries_type(data, "string");
}

fn set_retries_type(data: &Path, kind: &str) {
    let path = data.join("prefs-plugin").join("kagan.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let retries = &mut manifest["settings"]["retries"];
    retries["type"] = json!(kind);
    retries["default"] = if kind == "string" {
        json!("3")
    } else {
        json!(3)
    };
    std::fs::write(&path, manifest.to_string()).unwrap();
}

fn reload_prefs(cx: &mut TestAppContext) {
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
}

#[gpui_kit::test]
fn a_text_setting_shows_a_change_from_kg_without_saving(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert_eq!(settings_text(cx, "label"), "hi");

    set_plugin_config(cx, "label", json!("from kg"));
    cx.run_until_parked();
    assert_eq!(settings_text(cx, "label"), "from kg");
    assert_eq!(
        plugin_config(cx, "label"),
        json!("from kg"),
        "the change was not saved again"
    );
}

#[gpui_kit::test]
fn a_folder_approval_shows_in_settings(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");

    let folder = dunce::canonicalize(data.path())
        .unwrap()
        .display()
        .to_string();
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Set {
                    key: "folder".into(),
                    value: Some(json!(folder.clone())),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    set.detach();
    cx.run_until_parked();

    let id = cx
        .update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .approvals()
                .next()
                .map(|approval| approval.id.clone())
        })
        .expect("the folder waits for approval");
    assert!(
        shows(cx, window, &format!("question-{}", id.0)),
        "the approval is in Settings"
    );
    assert!(
        !shows(cx, window, "needs-you"),
        "the right sidebar stays hidden"
    );

    let answer = cx.update(|cx| Workspace::answer_standalone(id, "approve".into(), cx));
    cx.foreground_executor().block_test(answer).unwrap();
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "folder"), json!(folder));
}

#[gpui_kit::test]
fn a_plugin_with_only_a_settings_view_gets_a_page(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(
        cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))),
        "its own view is mounted"
    );
}

#[gpui_kit::test]
fn the_installed_switch_disables_and_enables_a_plugin(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(plugin_enabled(cx));

    click(cx, window, &format!("plugin-{PREFS}"));
    assert!(!plugin_enabled(cx), "the switch turns the plugin off");

    click(cx, window, &format!("plugin-{PREFS}"));
    assert!(plugin_enabled(cx), "and on again");
}

#[gpui_kit::test]
fn a_plugin_that_did_not_start_offers_update_and_remove(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_old_plugin);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, &format!("plugin-details-{OLD}"));
    assert!(
        !present(cx, window, &format!("plugin-update-{OLD}")),
        "a folder source has no Update"
    );
    assert!(present(cx, window, &format!("plugin-reload-{OLD}")));
    assert!(present(cx, window, &format!("plugin-remove-{OLD}")));

    // The author fixes the manifest and updates. A start that works clears the old reason.
    let fixed = json!({
        "id": OLD,
        "name": "Old",
        "version": "1.1.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A command only.",
        "powers": {},
        "commands": [{"id": "hi", "title": "Say hi"}]
    });
    std::fs::write(
        data.path().join("old-plugin").join("kagan.json"),
        fixed.to_string(),
    )
    .unwrap();
    click(cx, window, &format!("plugin-reload-{OLD}"));
    cx.run_until_parked();
    assert_eq!(listed_error(cx, OLD), None, "kg plugin list shows no error");
    assert_eq!(status_warnings(cx, OLD), None, "kg status shows no warning");
}

/// The start failure `kg plugin list` shows for a plugin.
fn listed_error(cx: &mut TestAppContext, id: &str) -> Option<String> {
    let list = cx.update(|cx| Workspace::dispatch(Op::Plugin(PluginOp::List), cx));
    match cx.foreground_executor().block_test(list).unwrap() {
        OpResult::Plugins(plugins) => plugins
            .into_iter()
            .find(|plugin| plugin.id.0 == id)
            .and_then(|plugin| plugin.error),
        other => panic!("{other:?}"),
    }
}

/// The warning `kg status` shows for a plugin.
fn status_warnings(cx: &mut TestAppContext, id: &str) -> Option<String> {
    let status = cx.update(|cx| {
        Workspace::dispatch(
            Op::Status {
                thread_id: None,
                project_id: None,
                environment_id: None,
            },
            cx,
        )
    });
    match cx.foreground_executor().block_test(status).unwrap() {
        OpResult::Status { warnings, .. } => warnings
            .into_iter()
            .find(|warning| warning.starts_with(&format!("plugin {id}:"))),
        other => panic!("{other:?}"),
    }
}

/// A plugin installed before its toolbar was refused: it cannot start.
fn install_old_plugin(data: &Path) {
    write_plugin(
        data,
        "old-plugin",
        &json!({
            "id": OLD,
            "name": "Old",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A plugin with a toolbar.",
            "powers": {},
            "ui": {"toolbar": [{"id": "old-open", "view": "OldButton"}]}
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
}

#[gpui_kit::test]
fn two_plugins_with_the_same_key_stay_apart(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "setting-row-dev.test.other/sounds");

    click(cx, window, "plugin-dev.test.other/sounds");
    assert_eq!(
        plugin_config_of(cx, OTHER, "sounds"),
        json!(true),
        "the other plugin changed"
    );
    assert_eq!(
        plugin_config_of(cx, PREFS, "sounds"),
        json!(true),
        "the first plugin did not"
    );
}

#[gpui_kit::test]
fn disabling_a_plugin_with_its_page_open_moves_the_page_and_drops_its_view(
    cx: &mut TestAppContext,
) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))));

    run_op(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(VIEW.into()),
        }),
    );
    assert!(!cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))));
    assert!(shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn reloading_a_plugin_with_its_page_open_mounts_its_view_again(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let before = cx.update(|cx| {
        kagan
            .read(cx)
            .settings_view_generation(&PluginId(VIEW.into()))
    });
    assert!(before.is_some());

    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(VIEW.into()),
            thread_id: None,
        }),
    );
    let host = cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .generation(&PluginId(VIEW.into()))
    });
    let after = cx.update(|cx| {
        kagan
            .read(cx)
            .settings_view_generation(&PluginId(VIEW.into()))
    });
    assert_ne!(before, after, "the view was made again");
    assert_eq!(after, host, "and it matches the plugin's new load");
    assert!(cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))));
}

#[gpui_kit::test]
fn an_entry_and_its_control_expose_a_label(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert_eq!(
        aria_label(cx, window, "theme-ember").as_deref(),
        Some("Ember theme")
    );
}

#[gpui_kit::test]
fn a_plugin_text_field_states_its_value_and_saves_on_enter(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    assert_eq!(value_of(cx, window, &field), Some("hi".into()));

    click(cx, window, &field);
    assert_eq!(focused_of(cx, window, &field), Some(true));
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, &field),
        Some("hi!".into()),
        "the field shows what is typed"
    );
    assert_eq!(
        plugin_config(cx, "label"),
        json!("hi"),
        "typing alone saves nothing"
    );

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
}

#[gpui_kit::test]
fn a_plugin_number_field_saves_a_finite_number_on_enter(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/retries");
    assert_eq!(value_of(cx, window, &field), Some("3".into()));
    assert_eq!(plugin_config(cx, "retries"), json!(3));

    click(cx, window, &field);
    cx.simulate_keystrokes(window, "backspace");
    cx.simulate_input(window, "2.5");
    cx.run_until_parked();
    assert_eq!(
        plugin_config(cx, "retries"),
        json!(3),
        "typing alone saves nothing"
    );

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "retries"), json!(2.5));
    assert_eq!(value_of(cx, window, &field), Some("2.5".into()));

    click(cx, window, &field);
    for _ in 0..3 {
        cx.simulate_keystrokes(window, "backspace");
    }
    cx.simulate_input(window, "  2.50 ");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "retries"), json!(2.5));
    assert_eq!(
        value_of(cx, window, &field),
        Some("2.5".into()),
        "Enter keeps the canonical number in the field"
    );
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/retries-note"
    ));

    click(cx, window, &field);
    for _ in 0..3 {
        cx.simulate_keystrokes(window, "backspace");
    }
    cx.simulate_input(window, "nope");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        plugin_config(cx, "retries"),
        json!(2.5),
        "a non-number is refused"
    );
    assert_eq!(value_of(cx, window, &field), Some("nope".into()));
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/retries-note").as_deref(),
        Some("Type a number.")
    );
}

#[gpui_kit::test]
fn a_plugin_turns_off_and_on_from_the_installed_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let switch = format!("plugin-{PREFS}");
    wait_until(cx, |cx| present(cx, window, &switch));
    assert!(plugin_enabled(cx));

    click(cx, window, &switch);
    wait_until(cx, |cx| !plugin_enabled(cx));
    click(cx, window, &switch);
    wait_until(cx, plugin_enabled);

    click(cx, window, &format!("plugin-details-{PREFS}"));
    assert!(
        !present(cx, window, &format!("plugin-update-{PREFS}")),
        "a folder source has no Update"
    );
    assert!(present(cx, window, &format!("plugin-reload-{PREFS}")));
    assert!(present(cx, window, &format!("plugin-logs-{PREFS}")));
    assert!(present(cx, window, &format!("plugin-remove-{PREFS}")));
}

fn aria_label(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.label.as_deref().map(str::to_owned))
    })
    .unwrap()
}

fn run_op(cx: &mut TestAppContext, op: Op) {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

fn plugin_config_of(cx: &mut TestAppContext, plugin: &str, key: &str) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(plugin.into()),
                op: ConfigOp::Get { key: key.into() },
                thread_id: None,
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

/// Opens the workspace without a window, so a preference can be set first.
fn setup_only(cx: &mut TestAppContext) -> tempfile::TempDir {
    crate::tests::kagan::setup(cx)
}

fn pane_width(cx: &mut TestAppContext, window: AnyWindowHandle) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name("sidebar-pane".into()))
            .map_or(0., |element| f32::from(element.bounds.size.width))
    })
    .unwrap()
}

fn saved(cx: &mut TestAppContext, key: &str) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(kg_domain::PreferenceOp::Get { key: key.into() }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn focused(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .is_some_and(|element| element.focused == Some(true))
    })
    .unwrap()
}

/// Presses Tab until the named control has the focus.
fn tab_to(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    for _ in 0..40 {
        if focused(cx, window, name) {
            return;
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    panic!("{name} never took the focus");
}

fn settings_text(cx: &mut TestAppContext, key: &str) -> String {
    let plugin = PluginId(PREFS.into());
    cx.update(|cx| crate::kagan::Kagan::settings_text(cx, &plugin, key))
}

fn set_plugin_config(cx: &mut TestAppContext, key: &str, value: Value) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Set {
                    key: key.into(),
                    value: Some(value),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

fn plugin_enabled(cx: &mut TestAppContext) -> bool {
    let list = cx.update(|cx| Workspace::dispatch(Op::Plugin(PluginOp::List), cx));
    match cx.foreground_executor().block_test(list).unwrap() {
        OpResult::Plugins(plugins) => plugins
            .iter()
            .find(|plugin| plugin.id.0 == PREFS)
            .is_some_and(|plugin| plugin.enabled),
        other => panic!("{other:?}"),
    }
}

fn set_mode(cx: &mut TestAppContext, mode: Appearance) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Theme(ThemeOp::Set {
                theme: "fjord".into(),
                mode,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

fn get_theme(cx: &mut TestAppContext) -> kg_domain::ThemeSettings {
    let get = cx.update(|cx| Workspace::dispatch(Op::Theme(ThemeOp::Get), cx));
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Theme(settings) => settings,
        other => panic!("{other:?}"),
    }
}

/// Two plugins: one declares settings (a switch, a select, a text, a folder), and one contributes
/// only its own settings view.
fn install_plugins(data: &Path) {
    write_plugin(
        data,
        "prefs-plugin",
        &json!({
            "id": PREFS,
            "name": "Prefs",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A test plugin with settings.",
            "powers": {},
            "settings": {
                "sounds": {"type": "boolean", "title": "Sounds", "default": true},
                "density": {
                    "type": "select",
                    "title": "Density",
                    "options": ["comfortable", "compact"],
                    "default": "comfortable"
                },
                "label": {"type": "string", "title": "Label", "default": "hi"},
                "board": {"type": "project", "title": "Board"},
                "retries": {"type": "number", "title": "Retries", "default": 3},
                "folder": {"type": "folder", "title": "Folder"}
            }
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
    write_plugin(
        data,
        "other-plugin",
        &json!({
            "id": OTHER,
            "name": "Other",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "Another test plugin with a setting of the same key.",
            "powers": {},
            "settings": {
                "sounds": {"type": "boolean", "title": "Sounds", "default": false}
            }
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
    write_plugin(
        data,
        "view-plugin",
        &json!({
            "id": VIEW,
            "name": "View",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A test plugin with its own settings view.",
            "powers": {},
            "ui": {"settings": "ViewSettings"}
        }),
        "// @ts-check\nimport { div, View } from \"gpui-kit\";\n\
         export default class Plugin extends View {}\n\
         export class ViewSettings extends View {\n  render() {\n    return div().child(\"A custom view\");\n  }\n}\n",
    );
}

pub(crate) fn write_plugin(data: &Path, folder: &str, manifest: &Value, entry: &str) {
    // A folder setting is a power the person approves at install: the grant names it.
    let folders: Value = manifest["settings"]
        .as_object()
        .map_or(json!({}), |settings| {
            settings
                .iter()
                .filter(|(_, setting)| setting["type"] == "folder")
                .map(|(key, setting)| (key.clone(), setting["title"].clone()))
                .collect()
        });
    let root = data.join(folder);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), entry).unwrap();
    let id = manifest["id"].as_str().unwrap().to_owned();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": folders}),
            error: None,
        })
        .unwrap();
    drop(store);
}

fn plugin_config(cx: &mut TestAppContext, key: &str) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Get { key: key.into() },
                thread_id: None,
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
fn leaving_settings_at_480_brings_back_the_window_layout(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(480.), px(720.)));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "settings-page"),
        "escape kept settings open"
    );
    assert!(
        !shows(cx, window, "project-prj_kagan"),
        "the sidebar came back at 480"
    );
    let (_, center) = span(cx, window, "center").unwrap();
    assert!(
        center >= 480. - 0.5,
        "center {center} after settings at 480"
    );
}

#[gpui_kit::test]
fn the_keyboard_page_shows_bound_keycaps_only(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "shortcut-NewThread");
    assert!(shows(cx, window, "setting-row-NewThread"));
    assert!(!present(cx, window, "setting-row-NewThread-note"));
    assert_eq!(
        aria_label(cx, window, "setting-row-NewThread").as_deref(),
        Some("New thread")
    );
    assert_eq!(
        aria_label(cx, window, "shortcut-NewThread").as_deref(),
        Some(bound_keycap(cx, "NewThread").as_str())
    );
    for absent in [
        "OpenPalette",
        "ShowHelp",
        "NextFile",
        "PrevFile",
        "RenameThread",
    ] {
        assert!(
            !present(cx, window, &format!("setting-row-{absent}")),
            "{absent} is not bound"
        );
    }
    assert!(!present(cx, window, "settings-heading-shortcuts"));
}

#[gpui_kit::test]
fn keyboard_groups_shortcuts_by_where_they_apply(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    for (section, row) in [
        ("window", "NewThread"),
        ("thread", "StopThread"),
        ("new-thread", "UseProjectFolder"),
        ("review", "NextChange"),
    ] {
        let row = format!("setting-row-{row}");
        click(cx, window, &row);
        cx.update_window(window, |_, window, _| {
            let element =
                crate::ui_registry::find(window, &ElementId::Name(row.clone().into())).unwrap();
            assert!((f32::from(element.bounds.size.height) - 24.).abs() < 0.5);
            assert!(
                element.path.iter().any(|id| *id
                    == ElementId::Name(format!("settings-group-shortcuts-{section}").into()))
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn a_saved_shortcut_changes_the_keycap(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    run_op(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: "NewThread".into(),
            key: "f10".into(),
            reset: false,
        }),
    );
    click(cx, window, "settings-gear");
    click(cx, window, "shortcut-NewThread");
    assert_eq!(
        aria_label(cx, window, "shortcut-NewThread").as_deref(),
        Some(bound_keycap(cx, "NewThread").as_str())
    );
    assert_ne!(
        aria_label(cx, window, "shortcut-NewThread").as_deref(),
        Some("f10")
    );
}

#[gpui_kit::test]
fn appearance_shows_a_missing_theme_and_keeps_its_id(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_theme(cx, "ocean");
    click(cx, window, "settings-gear");
    assert_eq!(
        aria_label(cx, window, "theme-error").as_deref(),
        Some("Theme ocean is missing. Using Kagan until it returns.")
    );
    assert_eq!(get_theme(cx).theme, "ocean");
    assert_eq!(selected_of(cx, window, "theme-fjord"), Some(false));
}

#[gpui_kit::test]
fn a_broken_theme_file_shows_its_error_on_its_card(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tide.json"), "{").unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let label = aria_label(cx, window, "theme-tide").unwrap_or_default();
    assert!(label.starts_with("tide theme, does not load: "), "{label}");
    assert_eq!(selected_of(cx, window, "theme-fjord"), Some(true));

    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        aria_label(cx, window, "theme-tide").as_deref(),
        Some("Tide theme")
    );
    click(cx, window, "theme-tide");
    assert_eq!(selected_of(cx, window, "theme-tide"), Some(true));
}

#[gpui_kit::test]
fn an_oversized_odd_or_special_theme_file_shows_on_its_card(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("big.json"),
        format!("{TIDE}{}", " ".repeat(64 * 1024)),
    )
    .unwrap();
    let long = TIDE.replace("\"Tide\"", &format!("\"{}\"", "t".repeat(65)));
    std::fs::write(path.join("long.json"), long).unwrap();
    #[cfg(unix)]
    assert!(
        std::process::Command::new("mkfifo")
            .arg(path.join("pipe.json"))
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let mut cards = vec![
        ("theme-big", "big theme, does not load: larger than 64 KiB"),
        (
            "theme-long",
            "long theme, does not load: theme name must be one line of at most 64 characters",
        ),
    ];
    if cfg!(unix) {
        cards.push((
            "theme-pipe",
            "pipe theme, does not load: not a regular file",
        ));
    }
    for (card, label) in cards {
        assert_eq!(aria_label(cx, window, card).as_deref(), Some(label));
    }
    assert_eq!(
        aria_label(cx, window, "theme-tide").as_deref(),
        Some("Tide theme"),
        "the folder still loads"
    );
}

#[gpui_kit::test]
fn an_invalid_edit_keeps_the_last_good_theme_and_says_so_once(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    set_theme(cx, "tide");
    let tide = painted_bg(cx);
    assert_eq!(tide, Color::new(0xf2_f2f2));

    std::fs::write(path.join("tide.json"), TIDE.replace("#f2f2f2", "nope")).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(painted_bg(cx), tide, "the last good colors stay");
    let message = aria_label(cx, window, "toast-message").unwrap_or_default();
    assert!(
        message.starts_with("Theme tide has an error, so the window keeps Tide: "),
        "{message}"
    );
    click(cx, window, "toast-dismiss");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(!shows(cx, window, "toast"), "the error is said once");

    std::fs::write(path.join("tide.json"), TIDE.replace("#f2f2f2", "#fafafa")).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        painted_bg(cx),
        Color::new(0xfa_fafa),
        "a fixed file applies"
    );
}

#[gpui_kit::test]
fn appearance_lists_built_in_user_and_plugin_themes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_theme_plugin);
    let path = data.path().join("themes");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tide.json"), TIDE).unwrap();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    for (card, label) in [
        ("theme-fjord", "Ember Fjord theme, in use"),
        ("theme-tide", "Tide theme"),
        ("theme-dev.test.themes/midnight", "Midnight theme"),
    ] {
        assert_eq!(aria_label(cx, window, card).as_deref(), Some(label));
    }

    click(cx, window, "theme-dev.test.themes/midnight");
    assert_eq!(get_theme(cx).theme, "dev.test.themes/midnight");
    assert_eq!(painted_bg(cx), Color::new(0x10_1010));
}

#[gpui_kit::test]
fn disabling_a_plugin_drops_its_theme_and_says_so_once(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_theme_plugin);
    set_theme(cx, "dev.test.themes/midnight");
    assert_eq!(painted_bg(cx), Color::new(0x10_1010));

    run_op(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(THEMES.into()),
        }),
    );
    click(cx, window, "settings-gear");
    assert!(!present(cx, window, "theme-dev.test.themes/midnight"));
    assert_eq!(
        aria_label(cx, window, "toast-message").as_deref(),
        Some("Theme dev.test.themes/midnight went away, so the window uses Ember Fjord.")
    );
    assert_eq!(painted_bg(cx), Color::new(0xee_f1f5), "the default paints");
    assert_eq!(get_theme(cx).theme, "dev.test.themes/midnight");

    run_op(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(THEMES.into()),
        }),
    );
    assert_eq!(
        painted_bg(cx),
        Color::new(0x10_1010),
        "it returns with the plugin"
    );
}

#[gpui_kit::test]
fn new_theme_copies_the_theme_in_use_and_opens_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "theme-new");
    let path = data.path().join("themes").join("fjord-copy.json");
    let file = ThemeFile::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file.name, "Ember Fjord copy");
    let url = cx.opened_url().unwrap_or_default();
    assert!(
        url.starts_with("file://") && url.ends_with("/themes/fjord-copy.json"),
        "{url}"
    );

    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(
        aria_label(cx, window, "theme-fjord-copy").as_deref(),
        Some("Ember Fjord copy theme")
    );
    click(cx, window, "theme-new");
    assert!(data.path().join("themes/fjord-copy-2.json").is_file());
}

#[gpui_kit::test]
fn kg_theme_new_copies_a_named_theme_and_keeps_existing_files(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let new = |id: &str, from: Option<&str>| {
        Op::Theme(ThemeOp::New {
            id: id.into(),
            from: from.map(Into::into),
        })
    };
    let made = try_op(cx, new("warm", Some("ember")));
    let path = std::fs::canonicalize(data.path())
        .unwrap()
        .join("themes/warm.json");
    assert_eq!(made.unwrap(), OpResult::Files(vec![path.clone()]));
    let file = ThemeFile::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file.name, "Ember copy");

    let again = try_op(cx, new("warm", Some("grey")))
        .unwrap_err()
        .to_string();
    assert!(again.ends_with("warm.json already exists"), "{again}");
    let built_in = try_op(cx, new("ember", None)).unwrap_err().to_string();
    assert_eq!(built_in, "theme ember is built in; choose another id");
    let missing = try_op(cx, new("cold", Some("ocean")))
        .unwrap_err()
        .to_string();
    assert_eq!(missing, "theme ocean");
}

#[gpui_kit::test]
fn open_themes_folder_makes_the_folder_and_opens_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data.path().join("themes");
    assert!(!path.exists());
    click(cx, window, "settings-gear");
    click(cx, window, "theme-folder");
    assert!(path.is_dir());
    let url = cx.opened_url().unwrap_or_default();
    assert!(
        url.starts_with("file://") && url.ends_with("/themes"),
        "{url}"
    );
}

fn painted_bg(cx: &mut TestAppContext) -> Color {
    cx.update(|cx| crate::theme::ThemeState::global(cx).read(cx).palette.bg)
}

fn try_op(cx: &mut TestAppContext, op: Op) -> Result<OpResult, kg_domain::OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let result = cx.foreground_executor().block_test(task);
    cx.run_until_parked();
    result
}

const THEMES: &str = "dev.test.themes";

fn install_theme_plugin(data: &Path) {
    write_plugin(
        data,
        "themes-plugin",
        &json!({
            "id": THEMES,
            "name": "Themes",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "powers": {},
            "themes": [{"id": "midnight", "file": "themes/midnight.json"}]
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
    let themes = data.join("themes-plugin/themes");
    std::fs::create_dir(&themes).unwrap();
    let midnight = TIDE
        .replace("Tide", "Midnight")
        .replace("#f2f2f2", "#101010");
    std::fs::write(themes.join("midnight.json"), midnight).unwrap();
}

#[gpui_kit::test]
fn general_describes_what_integration_still_sets(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "setting-row-agent-integration-note");
    let note = aria_label(cx, window, "setting-row-agent-integration-note").unwrap_or_default();
    assert!(
        note.contains("claude") && note.contains("codex") && note.contains("cursor-agent"),
        "{note}"
    );
    assert!(note.contains("--new-session-id or --resume"), "{note}");
    assert!(
        note.contains("--mcp-config")
            && note.contains("--session-id or --resume")
            && note.contains("-c mcp_servers.kagan.*"),
        "{note}"
    );
    assert!(note.contains("Off drops kagan’s tools"), "{note}");
    assert!(
        note.contains("KG_ variables") && note.contains("PATH"),
        "{note}"
    );
    assert!(!note.contains("unchanged"), "{note}");
}

#[gpui_kit::test]
fn close_leaves_settings_and_enter_on_the_page_does_not(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "settings-gear");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    tab_to(cx, window, "settings-close");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn a_text_draft_waits_for_enter_and_revert_puts_the_saved_value_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi"));
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/label-note").as_deref(),
        Some("Enter saves.")
    );
    assert!(shows(cx, window, "setting-revert-dev.test.prefs/label"));

    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/label-note").as_deref(),
        Some("Saving…")
    );
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
    assert!(!present(cx, window, "setting-revert-dev.test.prefs/label"));

    click(cx, window, &field);
    cx.simulate_input(window, "?");
    cx.run_until_parked();
    click(cx, window, "setting-revert-dev.test.prefs/label");
    assert_eq!(value_of(cx, window, &field), Some("hi!".into()));
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
}

#[gpui_kit::test]
fn a_newer_draft_survives_the_reply(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    cx.dispatch_keystroke(window, kg_core::parse_key("x").unwrap());
    cx.simulate_keystrokes(window, "tab");
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, &field), Some("hi!x".into()));
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
    assert_eq!(focused_of(cx, window, &field), Some(false));
}

#[gpui_kit::test]
fn a_failed_text_save_keeps_the_draft_until_revert(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    cx.update(|cx| {
        kg_plugins::PluginHost::global(cx).update(cx, |host, cx| {
            host.disable_loaded(&PluginId(PREFS.into()), cx);
        });
    });
    cx.run_until_parked();
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
    assert_eq!(value_of(cx, window, &field), Some("hi!".into()));
    assert_eq!(plugin_config(cx, "label"), json!("hi"));
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/label-note").as_deref(),
        Some("Enter saves.")
    );
    click(cx, window, "setting-revert-dev.test.prefs/label");
    assert_eq!(value_of(cx, window, &field), Some("hi".into()));
}

#[gpui_kit::test]
fn a_failed_switch_keeps_the_saved_value_and_shows_the_error(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("plugin-{PREFS}/sounds").into()),
            cx,
        )
        .unwrap();
        kg_plugins::PluginHost::global(cx).update(cx, |host, cx| {
            host.disable_loaded(&PluginId(PREFS.into()), cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
    assert_eq!(
        value_of(cx, window, &format!("plugin-{PREFS}/sounds")).as_deref(),
        Some("on")
    );
    assert!(
        !present(cx, window, "setting-row-dev.test.prefs/sounds-note"),
        "a disabled plugin's failed toggle does not stay after it returns"
    );
    click(cx, window, &format!("plugin-{PREFS}/sounds"));
    assert_eq!(plugin_config(cx, "sounds"), json!(false));
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/sounds-note"
    ));
}

#[gpui_kit::test]
fn cancelling_a_folder_picker_and_a_file_path_do_not_change_the_setting(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("choose-{PREFS}/folder").into()),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "folder"), serde_json::Value::Null);
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/folder-note"
    ));

    let file = data.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("choose-{PREFS}/folder").into()),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |_| Some(vec![file.clone()]));
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "folder"), serde_json::Value::Null);
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/folder-note").unwrap_or_default();
    assert!(note.contains("existing folder"), "{note}");
}

#[gpui_kit::test]
fn an_older_text_save_cannot_overwrite_a_newer_one(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    cx.dispatch_keystroke(window, kg_core::parse_key("x").unwrap());
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    until_saved(cx, "label", &json!("hi!"));
    assert!(
        cx.background_executor.tick(),
        "the first save still has work left"
    );
    assert_eq!(
        host_setting(cx, "label"),
        json!("hi!"),
        "the newer text stays queued until the first save finishes"
    );
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi!x"));
    assert_eq!(value_of(cx, window, &field), Some("hi!x".into()));
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/label-note"
    ));
}

#[gpui_kit::test]
fn an_older_select_cannot_overwrite_a_newer_one(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    press(cx, window, "option-dev.test.prefs/density/compact");
    press(cx, window, "option-dev.test.prefs/density/comfortable");
    until_saved(cx, "density", &json!("compact"));
    assert!(
        cx.background_executor.tick(),
        "the first save still has work left"
    );
    assert_eq!(
        host_setting(cx, "density"),
        json!("compact"),
        "the second choice waits until the first save finishes"
    );
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "density"), json!("comfortable"));
    assert_eq!(
        selected_of(cx, window, "option-dev.test.prefs/density/comfortable"),
        Some(true)
    );
}

#[gpui_kit::test]
fn an_older_folder_save_cannot_overwrite_a_newer_one(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let older = data.path().join("older");
    let newer = data.path().join("newer");
    std::fs::create_dir(&older).unwrap();
    std::fs::create_dir(&newer).unwrap();
    choose_folder(cx, window, &older);
    cx.run_until_parked();
    assert_eq!(approval_count(cx), 1);
    assert_eq!(host_setting(cx, "folder"), serde_json::Value::Null);
    choose_folder(cx, window, &newer);
    cx.run_until_parked();
    assert_eq!(
        approval_count(cx),
        1,
        "the newer folder waits behind the approval already open"
    );
    approve_waiting(cx);
    let stored = plugin_config(cx, "folder");
    assert_eq!(stored, json!(canonical(&older)), "{stored}");
    assert_eq!(approval_count(cx), 1);
    approve_waiting(cx);
    let stored = plugin_config(cx, "folder");
    assert_eq!(stored, json!(canonical(&newer)), "{stored}");
    assert!(waiting(cx).is_none());
}

#[gpui_kit::test]
fn an_external_save_clears_a_stale_setting_error(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let file = data.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    choose_folder(cx, window, &file);
    cx.run_until_parked();
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/folder-note").unwrap_or_default();
    assert!(note.contains("existing folder"), "{note}");

    let folder = data.path().join("chosen");
    std::fs::create_dir(&folder).unwrap();
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Set {
                    key: "folder".into(),
                    value: Some(json!(folder)),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    approve_waiting(cx);
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/folder-note"
    ));
    assert_eq!(
        plugin_config(cx, "folder"),
        json!(canonical(&data.path().join("chosen")))
    );
}

#[gpui_kit::test]
fn installed_names_how_to_make_a_plugin_when_none_are_installed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let note = aria_label(cx, window, "setting-row-none-note").unwrap_or_default();
    assert!(note.contains("kg plugin new <name>"), "{note}");
    assert!(note.contains("pages"), "{note}");
    assert!(present(cx, window, "plugin-install-open"));
}

#[gpui_kit::test]
fn an_enabled_plugin_says_what_it_added_without_a_side(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let note = aria_label(cx, window, &format!("setting-row-{PREFS}-note")).unwrap_or_default();
    assert_eq!(note, "Adds 6 settings.");
    assert!(!note.starts_with("On"), "{note}");
    assert!(!note.contains("right") && !note.contains("left"), "{note}");
}

#[gpui_kit::test]
fn plugin_settings_follow_the_order_in_the_manifest(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_ordered);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let zeta = row_top(cx, window, "setting-row-dev.example.order/zeta");
    let alpha = row_top(cx, window, "setting-row-dev.example.order/alpha");
    assert!(zeta < alpha, "zeta {zeta} then alpha {alpha}");
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId("dev.example.order".into()),
            thread_id: None,
        }),
    );
    let zeta = row_top(cx, window, "setting-row-dev.example.order/zeta");
    let alpha = row_top(cx, window, "setting-row-dev.example.order/alpha");
    assert!(zeta < alpha, "reload kept zeta {zeta} before alpha {alpha}");
}

#[gpui_kit::test]
fn a_folder_row_does_not_update_from_its_key(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let before = cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .generation(&PluginId(PREFS.into()))
    });
    click(cx, window, &format!("plugin-row-{PREFS}"));
    cx.simulate_keystrokes(window, "u");
    cx.run_until_parked();
    let after = cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .generation(&PluginId(PREFS.into()))
    });
    assert_eq!(before, after);
}

#[gpui_kit::test]
fn a_git_plugin_offers_update_and_a_new_power_waits_in_settings(cx: &mut TestAppContext) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--initial-branch=main"]);
    write_git_plugin(repo.path(), false);
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "--quiet", "-m", "one"]);
    let source = format!("git:{}@main", repo.path().display());
    let (_kagan, window, _data) = open_with_plugins_then(cx, |_| {});
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let installing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source,
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    approve_waiting(cx);
    cx.foreground_executor().block_test(installing).unwrap();
    click(cx, window, "plugin-details-dev.example.git");
    assert!(present(cx, window, "plugin-update-dev.example.git"));
    click(cx, window, "plugin-row-dev.example.git");
    cx.simulate_keystrokes(window, "u");
    cx.run_until_parked();
    assert!(waiting(cx).is_none(), "the same commit does not ask again");

    write_git_plugin(repo.path(), true);
    git(repo.path(), &["commit", "--quiet", "-am", "two"]);
    click(cx, window, "plugin-row-dev.example.git");
    cx.simulate_keystrokes(window, "u");
    cx.run_until_parked();
    let question = waiting(cx).expect("a new power waits");
    assert!(
        shows(cx, window, &format!("question-{}", question.0)),
        "the approval is in Settings"
    );
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn write_git_plugin(dir: &std::path::Path, clipboard: bool) {
    let powers = if clipboard {
        r#"{ "clipboard": { "write": true } }"#
    } else {
        "{}"
    };
    std::fs::write(
        dir.join("kagan.json"),
        format!(
            r#"{{
                "id": "dev.example.git",
                "name": "Git",
                "version": "1.0.0",
                "kagan": ">=0.1.0",
                "entry": "main.js",
                "instructions": "Git.",
                "powers": {powers}
            }}"#
        ),
    )
    .unwrap();
    std::fs::write(dir.join("main.js"), "export default class Plugin {}\n").unwrap();
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(window, &ElementId::Name(name.to_owned().into()), cx)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    })
    .unwrap();
}

fn choose_folder(cx: &mut TestAppContext, window: AnyWindowHandle, path: &Path) {
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("choose-{PREFS}/folder").into()),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    let path = path.to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![path]));
}

fn host_setting(cx: &mut TestAppContext, key: &str) -> Value {
    cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .settings(&PluginId(PREFS.into()))
            .into_iter()
            .find(|(name, _, _)| name == key)
            .map_or(Value::Null, |(_, _, value)| value)
    })
}

fn approval_count(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).approvals().count())
}

fn until_saved(cx: &mut TestAppContext, key: &str, want: &Value) {
    for _ in 0..200 {
        if &host_setting(cx, key) == want {
            return;
        }
        assert!(cx.background_executor.tick(), "{key} never reached {want}");
    }
    panic!("{key} stayed {}", host_setting(cx, key));
}

fn canonical(path: &Path) -> String {
    dunce::canonicalize(path).unwrap().display().to_string()
}

fn approve_waiting(cx: &mut TestAppContext) {
    let id = waiting(cx).expect("an approval is waiting");
    let answer = cx.update(|cx| Workspace::answer_standalone(id, "approve".into(), cx));
    cx.foreground_executor().block_test(answer).unwrap();
    cx.run_until_parked();
}

fn waiting(cx: &mut TestAppContext) -> Option<kg_domain::PendingInteractionId> {
    cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .map(|approval| approval.id.clone())
    })
}

fn bound_keycap(cx: &mut TestAppContext, action: &str) -> String {
    cx.update(|cx| {
        let held = cx.key_bindings();
        let bindings = held.borrow();
        let stroke = bindings.bindings().find(|binding| {
            let name = binding.action().name();
            name.rsplit("::").next() == Some(action) && crate::kagan::is_chord(binding.action())
        });
        stroke
            .and_then(|binding| binding.keystrokes().first())
            .map(|key| gpui_kit::component::kbd::Kbd::format(key.inner()))
            .unwrap_or_default()
    })
}

fn set_theme(cx: &mut TestAppContext, theme: &str) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Theme(ThemeOp::Set {
                theme: theme.into(),
                mode: Appearance::System,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

fn row_top(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(f32::MAX, |element| f32::from(element.bounds.origin.y))
    })
    .unwrap()
}

fn install_ordered(data: &Path) {
    let root = data.join("order-plugin");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("kagan.json"),
        r#"{
            "id": "dev.example.order",
            "name": "Order",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "Order.",
            "powers": {},
            "settings": {
                "zeta": { "type": "boolean", "title": "Zeta", "default": true },
                "alpha": { "type": "string", "title": "Alpha", "default": "a" }
            }
        }"#,
    )
    .unwrap();
    std::fs::write(root.join("main.js"), "export default class Plugin {}\n").unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.example.order".into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": {}}),
            error: None,
        })
        .unwrap();
}

const TIDE: &str = r##"{
  "name": "Tide",
  "light": {
    "bg": "#f2f2f2", "surface": "#e7eaf2", "raised": "#dae0ef", "ink": "#0f1c57",
    "muted": "#4c5775", "line": "#b4bdd3", "accent": "#1b2f86", "primaryForeground": "#f2f2f2",
    "working": "#1b2f86", "needs": "#755200", "failed": "#a32938", "idle": "#4c5775"
  },
  "dark": {
    "bg": "#0a1238", "surface": "#0f1c57", "raised": "#1b2f86", "ink": "#c5cde3",
    "muted": "#a4b2d6", "line": "#384b82", "accent": "#9bb5ff", "primaryForeground": "#0a1238",
    "working": "#9bb5ff", "needs": "#e8cf80", "failed": "#ffa1ad", "idle": "#a4b2d6"
  }
}"##;

#[gpui_kit::test]
fn cancel_install_with_nothing_installing_says_so(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open_with_plugins_then(cx, |_| {});
    let cancel = cx.update(|cx| Workspace::dispatch(Op::Plugin(PluginOp::CancelInstall), cx));
    let error = cx.foreground_executor().block_test(cancel).unwrap_err();
    assert_eq!(
        error,
        kg_domain::OpError::Invalid("no git install is running".into())
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_plugin_name_the_disk_cannot_hold_says_so_plainly(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open_with_plugins_then(cx, |_| {});
    let name = "a".repeat(300);
    let dir = data.path().join(&name);
    let created = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::New {
                name,
                dir: Some(dir),
            }),
            cx,
        )
    });
    let error = cx.foreground_executor().block_test(created).unwrap_err();
    assert_eq!(
        error,
        kg_domain::OpError::Invalid("the plugin name is too long".into())
    );
}

#[gpui_kit::test]
fn an_index_added_in_settings_finds_a_plugin_and_install_asks_for_approval(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |_| {});
    let team = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(team.path().join(".kagan")).unwrap();
    std::fs::write(
        team.path().join(".kagan/index.json"),
        json!({
            "schemaVersion": 1,
            "name": "team",
            "plugins": [{ "name": "echo", "description": "Echoes text", "source": "echo" }]
        })
        .to_string(),
    )
    .unwrap();
    let echo = team.path().join("echo");
    std::fs::create_dir(&echo).unwrap();
    std::fs::write(
        echo.join("kagan.json"),
        json!({
            "id": "dev.test.echo", "name": "Echo", "version": "1.0.0", "kagan": ">=0.1.0",
            "entry": "main.js", "instructions": "Test.",
            "powers": { "clipboard": { "write": true } }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(echo.join("main.js"), "export default class Plugin {}\n").unwrap();
    let location = dunce::canonicalize(team.path())
        .unwrap()
        .display()
        .to_string();

    click(cx, window, "settings-gear");
    assert!(!shows(cx, window, "plugin-search"), "no index, no search");
    click(cx, window, "plugin-index");
    cx.simulate_input(window, &location);
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| shows(cx, window, "setting-row-index-0"));
    assert_eq!(value_of(cx, window, "plugin-index"), Some(String::new()));

    click(cx, window, "plugin-search");
    cx.simulate_input(window, "echo");
    click(cx, window, "plugin-search-run");
    wait_until(cx, |cx| {
        present(cx, window, "plugin-listing-install-team/echo")
    });
    click(cx, window, "plugin-listing-install-team/echo");
    wait_until(cx, |cx| {
        cx.read(|cx| Workspace::global(cx).read(cx).approvals().next().is_some())
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .unwrap()
    });
    assert!(
        approval
            .title
            .starts_with("Install Echo 1.0.0 (dev.test.echo)?\nFrom the folder ")
            && approval.title.contains(", listed by index team at "),
        "{}",
        approval.title
    );
    let accepted =
        cx.update(|cx| Workspace::answer_standalone(approval.id.clone(), "approve".into(), cx));
    cx.foreground_executor().block_test(accepted).unwrap();
    wait_until(cx, |cx| present(cx, window, "plugin-row-dev.test.echo"));

    click(cx, window, "plugin-index-remove-0");
    wait_until(cx, |cx| !present(cx, window, "setting-row-index-0"));
    assert!(
        !present(cx, window, "plugin-listing-install-team/echo"),
        "a removed index's results go"
    );
}

fn nav(command: &str) -> String {
    format!("shortcut-plugin-{NAVIGATOR}-{command}")
}

fn nav_note(cx: &mut TestAppContext, window: AnyWindowHandle, command: &str) -> Option<String> {
    aria_label(
        cx,
        window,
        &format!("setting-row-plugin-{NAVIGATOR}-{command}-note"),
    )
}

/// The keycaps a row shows for a key text, as its label reads them.
fn keycap(key: &str) -> String {
    key.split_whitespace()
        .map(|key| gpui_kit::component::kbd::Kbd::format(&kg_core::parse_key(key).unwrap()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn plugin_keys(cx: &mut TestAppContext) -> Value {
    let get = cx.update(|cx| Workspace::dispatch(Op::Keymap(KeymapOp::Get), cx));
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value["plugins"].clone(),
        other => panic!("{other:?}"),
    }
}

fn keyboard_with_navigator(cx: &mut TestAppContext) -> (AnyWindowHandle, tempfile::TempDir) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |data| {
        install_navigator(data);
        install_twin(data);
    });
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    (window, data)
}

const TWIN: &str = "dev.kagan.twin";

/// A second plugin whose only command wants the key Navigator's Show already has.
fn install_twin(data: &Path) {
    let root = data.join("plugins").join("twin");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = json!({
        "id": TWIN, "name": "Twin", "version": "1.0.0", "kagan": ">=0.1.0", "entry": "main.js",
        "commands": [{ "id": "echo", "title": "Echo", "key": "secondary-shift-y" }]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        "import { View } from 'gpui-kit';\nexport default class Plugin extends View { render() { return ''; } }\n",
    )
    .unwrap();
    kg_store::Store::open(data.join("kagan.sqlite3"))
        .unwrap()
        .insert_plugin(&Plugin {
            id: PluginId(TWIN.into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: json!({ "powers": {}, "downloads": {}, "folders": {} }),
            error: None,
        })
        .unwrap();
}

#[gpui_kit::test]
fn two_plugins_that_want_one_key_both_say_so(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    let echo = format!("shortcut-plugin-{TWIN}-echo");
    click(cx, window, &format!("setting-row-plugin-{TWIN}-echo"));
    assert_eq!(aria_label(cx, window, &echo).as_deref(), Some("Not set"));
    assert_eq!(
        aria_label(cx, window, &format!("setting-row-plugin-{TWIN}-echo-note")),
        Some(format!(
            "{} is already used by Show in Navigator.",
            keycap("secondary-shift-y")
        ))
    );
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    assert_eq!(
        aria_label(cx, window, &nav("show")),
        Some(keycap("secondary-shift-y"))
    );
    assert_eq!(
        nav_note(cx, window, "show").as_deref(),
        Some("Echo in Twin also wants this key.")
    );
}

#[gpui_kit::test]
fn a_key_the_program_needs_is_not_saved(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-probe"));
    for key in ["a", "ctrl-c"] {
        click(cx, window, &format!("{}-key", nav("probe")));
        cx.simulate_keystrokes(window, key);
        cx.run_until_parked();
        assert_eq!(
            aria_label(cx, window, &nav("probe")).as_deref(),
            Some("Not set")
        );
        assert!(
            nav_note(cx, window, "probe")
                .is_some_and(|note| note.starts_with("That key belongs to the program.")),
            "{key}"
        );
        assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:probe")], Value::Null);
    }
}

#[gpui_kit::test]
fn every_plugin_command_has_a_keyboard_row_and_a_clash_shows_on_both(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-probe"));
    assert_eq!(
        aria_label(
            cx,
            window,
            &format!("settings-heading-shortcuts-plugin-{NAVIGATOR}")
        )
        .as_deref(),
        Some("Navigator")
    );
    assert_eq!(
        aria_label(cx, window, &nav("probe")).as_deref(),
        Some("Not set")
    );
    assert_eq!(
        aria_label(cx, window, &format!("{}-key", nav("probe"))).as_deref(),
        Some("Bind")
    );

    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    assert_eq!(
        aria_label(cx, window, &nav("show")),
        Some(keycap("secondary-shift-y"))
    );
    assert_eq!(
        aria_label(cx, window, &format!("{}-key", nav("show"))).as_deref(),
        Some("Change")
    );
    assert!(present(cx, window, &format!("{}-remove", nav("show"))));

    // New Thread keeps its key. Clash wanted it, so its row says what took it, and so does New Thread's.
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-clash"));
    assert_eq!(
        aria_label(cx, window, &nav("clash")).as_deref(),
        Some("Not set")
    );
    assert_eq!(
        nav_note(cx, window, "clash"),
        Some(format!(
            "{} is already used by New thread.",
            keycap(&chord("NewThread"))
        ))
    );
    click(cx, window, "setting-row-NewThread");
    let taken = aria_label(cx, window, "setting-row-NewThread-note").unwrap_or_default();
    assert!(
        taken.contains("Clash in Navigator also wants this key."),
        "{taken}"
    );
    assert!(
        taken.contains("Chord in Navigator also wants this key."),
        "{taken}"
    );
}

#[gpui_kit::test]
fn a_plugin_command_binds_in_settings_and_its_key_runs_it(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    let row = format!("setting-row-plugin-{NAVIGATOR}-probe");
    click(cx, window, &row);
    click(cx, window, &format!("{}-key", nav("probe")));
    assert_eq!(
        aria_label(cx, window, &nav("probe")).as_deref(),
        Some("Press a key")
    );

    // Escape stops without saving, and Settings stays open.
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert_eq!(
        aria_label(cx, window, &nav("probe")).as_deref(),
        Some("Not set")
    );
    assert!(shows(cx, window, "settings-page"));

    // A key Show already has is not saved, and the row says who has it.
    click(cx, window, &format!("{}-key", nav("probe")));
    cx.simulate_keystrokes(window, "secondary-shift-y");
    wait_until(cx, |cx| nav_note(cx, window, "probe").is_some());
    assert_eq!(
        nav_note(cx, window, "probe"),
        Some(format!(
            "{} is already used by Show in Navigator.",
            keycap("secondary-shift-y")
        ))
    );
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:probe")], Value::Null);

    click(cx, window, &format!("{}-key", nav("probe")));
    cx.simulate_keystrokes(window, "f8");
    wait_until(cx, |cx| {
        aria_label(cx, window, &nav("probe")) == Some(keycap("f8"))
    });
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:probe")], "f8");
    assert_eq!(nav_note(cx, window, "probe"), None);

    click(cx, window, "settings-close");
    cx.simulate_keystrokes(window, "f8");
    wait_until(cx, |cx| plugin_log(cx, NAVIGATOR).contains("probe refused"));
}

#[gpui_kit::test]
fn a_plugin_key_is_removed_and_its_rows_go_with_the_plugin(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    click(cx, window, &format!("{}-remove", nav("show")));
    wait_until(cx, |cx| {
        aria_label(cx, window, &nav("show")).as_deref() == Some("Not set")
    });
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:show")], Value::Null);

    // The removed key stays removed when the plugin comes back.
    let id = PluginId(NAVIGATOR.into());
    run_op(cx, Op::Plugin(PluginOp::Disable { id: id.clone() }));
    assert!(!present(
        cx,
        window,
        &format!("settings-group-shortcuts-plugin-{NAVIGATOR}")
    ));
    assert!(!present(cx, window, &nav("show")));
    assert!(plugin_keys(cx).get(format!("{NAVIGATOR}:show")).is_none());
    run_op(cx, Op::Plugin(PluginOp::Enable { id }));
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    assert_eq!(
        aria_label(cx, window, &nav("show")).as_deref(),
        Some("Not set")
    );
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:show")], Value::Null);
}

#[gpui_kit::test]
fn change_on_a_review_shortcut_saves_it_under_its_keymap_name(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "setting-row-NextChange");
    click(cx, window, "shortcut-NextChange-key");
    cx.simulate_keystrokes(window, "f9");
    wait_until(cx, |cx| {
        aria_label(cx, window, "shortcut-NextChange") == Some(keycap("f9"))
    });
    let get = cx.update(|cx| Workspace::dispatch(Op::Keymap(KeymapOp::Get), cx));
    let OpResult::Setting(keys) = cx.foreground_executor().block_test(get).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(keys["shared"]["NextHunk"], "f9");
    assert_eq!(aria_label(cx, window, "setting-row-NextChange-note"), None);
}

#[gpui_kit::test]
fn the_key_button_keeps_focus_while_it_waits_and_after(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    let button = format!("{}-key", nav("probe"));
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-probe"));
    let mut reached = false;
    for _ in 0..200 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of(cx, window, &button) == Some(true) {
            reached = true;
            break;
        }
    }
    assert!(reached, "Tab reaches Bind");
    press_key(cx, window, "enter");
    assert_eq!(aria_label(cx, window, &button).as_deref(), Some("Cancel"));
    assert_eq!(focused_of(cx, window, &button), Some(true));
    cx.simulate_keystrokes(window, "f8");
    wait_until(cx, |cx| {
        aria_label(cx, window, &button).as_deref() == Some("Change")
    });
    assert_eq!(focused_of(cx, window, &button), Some(true));
}

const ZZ: &str = "dev.zz.tool";
const AA: &str = "dev.aa.tool";

fn install_tool(data: &Path, id: &str, key: Option<&str>, enabled: bool) {
    let root = data.join("plugins").join(id);
    std::fs::create_dir_all(&root).unwrap();
    let mut command = json!({ "id": "run", "title": "Run" });
    if let Some(key) = key {
        command["key"] = json!(key);
    }
    let manifest = json!({
        "id": id, "name": id, "version": "1.0.0", "kagan": ">=0.1.0", "entry": "main.js",
        "commands": [command]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        "import { View } from 'gpui-kit';\nexport default class Plugin extends View { render() { return ''; } }\n",
    )
    .unwrap();
    kg_store::Store::open(data.join("kagan.sqlite3"))
        .unwrap()
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled,
            approved: json!({ "powers": {}, "downloads": {}, "folders": {} }),
            error: None,
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_saved_key_wins_over_a_plugin_installed_later(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_tool(data, ZZ, None, true);
        install_tool(data, AA, Some("f8"), false);
    });
    run_op(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: format!("{ZZ}:run"),
            key: "f8".into(),
            reset: false,
        }),
    );
    // aa sorts first, and its manifest asks for the same key.
    run_op(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(AA.into()),
        }),
    );
    click(cx, window, "settings-gear");
    let zz = format!("shortcut-plugin-{ZZ}-run");
    let aa = format!("shortcut-plugin-{AA}-run");
    click(cx, window, &format!("setting-row-plugin-{ZZ}-run"));
    assert_eq!(aria_label(cx, window, &zz), Some(keycap("f8")));
    click(cx, window, &format!("setting-row-plugin-{AA}-run"));
    assert_eq!(aria_label(cx, window, &aa).as_deref(), Some("Not set"));
    assert_eq!(
        aria_label(cx, window, &format!("setting-row-plugin-{AA}-run-note")),
        Some(format!("{} is already used by Run in {ZZ}.", keycap("f8")))
    );
}
