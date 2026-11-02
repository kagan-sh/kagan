use crate::kagan::screen_read;
use std::path::Path;

use crate::kagan::SurfaceCommit;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Focusable as _, InputEvent as _, TestAppContext,
    Window, px, size,
};
use kg_core::Workspace;
use kg_domain::{
    Appearance, Op, OpError, OpResult, Plugin, PluginId, PluginOp, PreferenceOp, SettingsOp,
    SurfaceItem, SurfaceOp, SurfacePlacement, ThemeOp, ThreadId, ThreadOp, ThreadStatus, UiOp,
};
use serde_json::json;

use super::kagan::{
    chord, click, focused_of, install_overlay, label, open, open_with_plugins_then, plugin_log,
    present, press_key, repo_thread, run_shell, selected_of, shows, thread_record, wait_until,
};
use super::sidebars::focus_row;

const ALPHA: &str = "dev.test.alpha";
const BETA: &str = "dev.test.beta";
const BROKEN: &str = "dev.test.broken";

#[gpui_kit::test]
fn rail_switches_between_tool_and_threads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    click(cx, window, &tool);
    assert!(
        item(&surfaces(cx), ALPHA, "tool").open,
        "{:?}",
        surfaces(cx)
    );
    assert!(
        item(&surfaces(cx), ALPHA, "tool").visible,
        "{:?}",
        surfaces(cx)
    );
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert!(shows(cx, window, "project-prj_kagan"));
    click(cx, window, "threads-home");
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert!(!shows(cx, window, "alpha-tool"));
    open_surface(cx, ALPHA, "tool").unwrap();
    run(cx, Op::Surface(SurfaceOp::Threads)).unwrap();
    assert!(shows(cx, window, "project-prj_kagan"));
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn rail_has_the_expected_width_and_plugin_settings_opens(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    assert!((width_of(cx, window, "sidebar-rail") - 48.).abs() < 0.5);
    assert!((width_of(cx, window, &format!("surface-item-{ALPHA}-tool")) - 24.).abs() < 0.5);
    run(cx, Op::Settings(SettingsOp::Plugins)).unwrap();
    wait_until(cx, |cx| shows(cx, window, "setting-row-install"));
}

#[gpui_kit::test]
fn the_rail_has_one_icon_per_plugin_above_the_plugin_buttons(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_many);
    wide(cx, window);
    let tool = "surface-item-dev.test.many-t0";
    assert!(shows(cx, window, tool));
    assert!(!shows(cx, window, "surface-item-dev.test.many-t1"));
    let tool_y = origin_y(cx, window, tool);
    assert!(origin_y(cx, window, "plugins-manage") > tool_y);
    assert!(origin_y(cx, window, "plugins-add") > origin_y(cx, window, "plugins-manage"));
    assert_eq!(label(cx, window, "plugins-manage"), "Manage plugins");
    assert_eq!(label(cx, window, "plugins-add"), "New plugin…");
}

#[gpui_kit::test]
fn clicking_the_open_rail_tool_returns_to_threads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    click(cx, window, &tool);
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert_eq!(selected_of(cx, window, &tool), Some(true));
    click(cx, window, &tool);
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert!(!shows(cx, window, "alpha-tool"));
    assert_eq!(selected_of(cx, window, &tool), Some(false));
}

#[gpui_kit::test]
fn a_narrow_window_shows_the_sidebar_over_the_center(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    assert!(!shows(cx, window, "project-prj_kagan"));
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "project-prj_kagan"));
    assert_eq!(label(cx, window, "left-sidebar-toggle"), "Hide sidebar");
    assert!(shows(cx, window, "threads-home"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "threads-home"));
}

#[gpui_kit::test]
fn at_528_the_rail_stays_and_show_sidebar_draws_the_list_over_the_center(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    let viewport = cx
        .update_window(window, |_, window, _| {
            f32::from(window.viewport_size().width)
        })
        .unwrap();
    assert!((viewport - 528.).abs() < 0.5, "viewport {viewport}");
    assert!(!shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "sidebar-rail"));
    assert!((origin_x(cx, window, "sidebar-rail")).abs() < 0.5);
    assert!((width_of(cx, window, "sidebar-rail") - 48.).abs() < 0.5);

    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert_eq!(label(cx, window, "left-sidebar-toggle"), "Hide sidebar");
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "sidebar-rail"));
    assert!((origin_x(cx, window, "sidebar-rail")).abs() < 0.5);
    let panel_x = origin_x(cx, window, "left-overlay-panel");
    let panel_width = width_of(cx, window, "left-overlay-panel");
    assert!((panel_x - 48.).abs() < 0.5, "panel at {panel_x}");
    assert!(
        panel_x + panel_width <= 528.5,
        "panel {panel_x}+{panel_width}"
    );
}

#[gpui_kit::test]
fn the_rail_has_a_page_icon_that_opens_and_closes_the_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_many);
    wide(cx, window);
    let page = "surface-item-dev.test.many-page";
    assert!(shows(cx, window, page));
    assert_eq!(label(cx, window, page), "Board");
    let tool_y = origin_y(cx, window, "surface-item-dev.test.many-t0");
    let page_y = origin_y(cx, window, page);
    assert!(page_y > tool_y);
    assert!(origin_y(cx, window, "plugins-manage") > page_y);

    click(cx, window, page);
    wait_until(cx, |cx| shows(cx, window, "surface-title"));
    assert_eq!(selected_of(cx, window, page), Some(true));
    click(cx, window, page);
    wait_until(cx, |cx| !shows(cx, window, "surface-title"));
    assert_eq!(selected_of(cx, window, page), Some(false));
}

#[gpui_kit::test]
fn a_rail_tool_at_a_narrow_width_opens_over_the_center(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    click(cx, window, &format!("surface-item-{ALPHA}-tool"));
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert!(shows(cx, window, "left-overlay-panel"));
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);
    assert_eq!(focused_of(cx, window, "left-overlay-panel"), Some(true));

    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert!(width_of(cx, window, "center") >= 480. - 0.5);
    assert!(!shows(cx, window, "surface-title"));
}

#[gpui_kit::test]
fn a_row_menu_at_a_narrow_width_opens_in_the_overlay(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    narrow(cx, window);
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, &format!("thread-actions-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "popup-menu"));
    assert!(shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "popup-menu"));
}

#[gpui_kit::test]
fn rail_tool_is_reachable_by_keyboard(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    tab_to(cx, window, &tool);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
}

const TINT: &str = "dev.test.tint";

const TINT_OVERLAY: &str = r##"import { View, div } from "gpui-kit";
import { log } from "kagan/host";
export default class Plugin extends View {
  render() { return ""; }
}
export class Cover extends View {
  init() { log("info", "mounted"); }
  render() { return div().size_full().bg("#ff00ff"); }
}
"##;

const THROWING_OVERLAY: &str = r#"import { View } from "gpui-kit";
export default class Plugin extends View {
  render() { return ""; }
}
export class Cover extends View {
  render() { throw new Error("boom in the overlay"); }
}
"#;

fn bounds_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .unwrap_or_else(|| panic!("{name} is not drawn"))
            .bounds()
    })
    .unwrap()
}

fn dialog(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    use gpui_kit::component::WindowExt as _;
    cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))
        .unwrap()
}

#[gpui_kit::test]
fn an_overlay_covers_the_center_and_kagan_draws_over_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) =
        open_with_plugins_then(cx, |data| install_overlay(data, TINT, TINT_OVERLAY));
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));
    let center = bounds_of(cx, window, "center");
    assert_eq!(bounds_of(cx, window, "plugin-overlays"), center);
    assert!(center.origin.x > px(0.), "{center:?}");

    let magenta: gpui_kit::Hsla = gpui_kit::rgb(0x00ff_00ff).into();
    let (quads, scaled) = cx
        .update_window(window, |_, window, _| {
            (window.painted_quads(), center.scale(window.scale_factor()))
        })
        .unwrap();
    let tint = quads
        .iter()
        .position(|quad| quad.background == magenta.into())
        .expect("the overlay is painted");
    assert_eq!(quads[tint].bounds, scaled);
    let grid = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("terminal-grid".into()))
                .expect("the terminal grid")
                .bounds()
                .scale(window.scale_factor())
        })
        .unwrap();
    let terminal: Vec<_> = quads
        .iter()
        .enumerate()
        .filter(|(_, quad)| quad.bounds.intersects(&grid) && quad.background != magenta.into())
        .map(|(index, _)| index)
        .collect();
    assert!(!terminal.is_empty(), "the terminal paints nothing");
    assert!(
        terminal.iter().all(|index| *index < tint),
        "the terminal is painted over the overlay"
    );

    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    click(cx, window, "quit-cancel");
    cx.run_until_parked();
    assert!(
        !dialog(cx, window),
        "the overlay took the click meant for the dialog"
    );

    let other = run_shell(cx);
    narrow(cx, window);
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, &format!("thread-{}", other.0));
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(other));
    assert!(shows(cx, window, &overlay));
}

#[gpui_kit::test]
fn disable_reload_and_removal_take_the_overlay_away(cx: &mut TestAppContext) {
    let (_kagan, window, data) =
        open_with_plugins_then(cx, |data| install_overlay(data, TINT, TINT_OVERLAY));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));
    let plugin = PluginId(TINT.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: plugin.clone() })).unwrap();
    cx.run_until_parked();
    assert!(!present(cx, window, &overlay));
    run(cx, Op::Plugin(PluginOp::Enable { id: plugin.clone() })).unwrap();
    wait_until(cx, |cx| shows(cx, window, &overlay));

    let log = data.path().join("plugins").join(TINT).join("logs.jsonl");
    let mounts = || std::fs::read_to_string(&log).map_or(0, |text| text.matches("mounted").count());
    let before = mounts();
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: plugin.clone(),
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| mounts() > before && shows(cx, window, &overlay));

    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: plugin,
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
            .id()
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!present(cx, window, &overlay));
    assert!(!present(cx, window, "plugin-overlays"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_overlay_that_throws_draws_nothing_and_the_terminal_still_types(cx: &mut TestAppContext) {
    let plugin = "dev.test.throwing";
    let (kagan, window, _data) =
        open_with_plugins_then(cx, |data| install_overlay(data, plugin, THROWING_OVERLAY));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{plugin}");
    wait_until(cx, |cx| !present(cx, window, &overlay));
    cx.simulate_input(window, "echo still''here");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| {
        super::kagan::grid_text(cx, &kagan).contains("stillhere")
    });
    assert!(!present(cx, window, &overlay));
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("boom in the overlay")
    });
}

fn install_example_overlay(data: &Path) {
    let root = dunce::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/overlay"),
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.overlay".into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": {}}),
            error: None,
        })
        .unwrap();
}

/// The example's bubble: a 96 point square box, painted.
fn bubbles(cx: &mut TestAppContext, window: AnyWindowHandle) -> usize {
    cx.update_window(window, |_, window, _| {
        let side = 96. * window.scale_factor();
        window
            .painted_quads()
            .iter()
            .filter(|quad| {
                (quad.bounds.size.width.0 - side).abs() < 1.
                    && (quad.bounds.size.height.0 - side).abs() < 1.
            })
            .count()
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_example_overlay_draws_and_its_setting_hides_it(cx: &mut TestAppContext) {
    let plugin = "dev.kagan.overlay";
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_example_overlay);
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| bubbles(cx, window) == 1);
    assert!(shows(cx, window, &format!("plugin-overlay-{plugin}")));
    run(
        cx,
        Op::Plugin(PluginOp::Config {
            id: PluginId(plugin.into()),
            op: kg_domain::ConfigOp::Set {
                key: "shown".into(),
                value: Some(json!(false)),
            },
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| bubbles(cx, window) == 0);
    let log = plugin_log(cx, plugin);
    assert!(
        !log.contains("crashed") && !log.contains("paint-only"),
        "{log}"
    );
}

fn ask(cx: &mut TestAppContext, id: &ThreadId) {
    let asked = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Keep going?".into(),
                options: ["yes", "no"]
                    .map(|option| kg_domain::InteractionOption {
                        id: option.into(),
                        label: option.into(),
                    })
                    .into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(asked).unwrap();
}

#[gpui_kit::test]
fn an_overlay_hides_while_a_question_waits_and_never_draws_over_settings(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, TINT, TINT_OVERLAY);
        install_overlay(data, "dev.test.other", TINT_OVERLAY);
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));

    run(cx, Op::Settings(SettingsOp::Plugins)).unwrap();
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
    assert!(!present(cx, window, "plugin-overlays"));
    run(cx, Op::Surface(SurfaceOp::Threads)).unwrap();
    wait_until(cx, |cx| shows(cx, window, &overlay));

    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: PluginId("dev.test.other".into()),
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    wait_until(cx, |cx| !present(cx, window, "plugin-overlays"));
    run(cx, Op::Settings(SettingsOp::Plugins)).unwrap();
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
    assert!(!present(cx, window, "plugin-overlays"));
    drop(remove);
}

#[gpui_kit::test]
fn an_overlay_hides_while_its_thread_asks(cx: &mut TestAppContext) {
    let (_kagan, window, _data) =
        open_with_plugins_then(cx, |data| install_overlay(data, TINT, TINT_OVERLAY));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));
    ask(cx, &id);
    wait_until(cx, |cx| !present(cx, window, &overlay));
}

fn image_refused(cx: &mut TestAppContext, plugin: &str, source: &str) {
    let attack = format!(".child(image({source:?}))");
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &super::kagan::overlay_source(&attack));
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("relative path inside the application's folder")
    });
    wait_until(cx, |cx| {
        !present(cx, window, &format!("plugin-overlay-{plugin}"))
    });
}

#[gpui_kit::test]
fn an_overlay_image_from_a_url_is_refused(cx: &mut TestAppContext) {
    image_refused(cx, "dev.test.url", "https://example.com/a.png?d=secret");
}

#[gpui_kit::test]
fn an_overlay_image_that_leaves_the_folder_is_refused(cx: &mut TestAppContext) {
    image_refused(cx, "dev.test.outside", "../../secret.png");
}

#[gpui_kit::test]
fn an_overlay_that_opens_a_popover_is_refused_and_dialogs_take_clicks(cx: &mut TestAppContext) {
    let plugin = "dev.test.popover";
    let attack = ".child(Popover.new(\"menu\").open(true).trigger(div()).content(div().w(600).h(600).child(\"Deny\")))";
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &super::kagan::overlay_source(attack));
    });
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| plugin_log(cx, plugin).contains("Popover"));
    assert!(!present(cx, window, &format!("plugin-overlay-{plugin}")));
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    click(cx, window, "quit-cancel");
    cx.run_until_parked();
    assert!(!dialog(cx, window));
}

fn install_many(data: &Path) {
    let mut panels = Vec::new();
    for index in 0..12 {
        panels.push(json!({
            "id": format!("t{index}"),
            "title": format!("Tool {index}"),
            "view": "ToolPanel",
            "placement": "left"
        }));
    }
    panels.push(json!({
        "id": "page",
        "title": "Board",
        "view": "PagePanel",
        "placement": "center"
    }));
    write_plugin(
        data,
        "many",
        "dev.test.many",
        &manifest("dev.test.many", "Many", json!(panels), false),
        &source(false),
    );
}

fn install_pair(data: &Path) {
    write_plugin(
        data,
        "alpha",
        ALPHA,
        &manifest(
            ALPHA,
            "Alpha",
            json!([
                {"id": "tool", "title": "Alpha tool", "view": "ToolPanel", "placement": "left"},
                {"id": "page", "title": "Alpha page", "view": "PagePanel", "placement": "center"},
                {"id": "side", "title": "Alpha side", "view": "SidePanel", "placement": "right"}
            ]),
            true,
        ),
        &source(true),
    );
    write_plugin(
        data,
        "beta",
        BETA,
        &manifest(
            BETA,
            "Beta",
            json!([
                {"id": "tool", "title": "Beta tool", "view": "ToolPanel", "placement": "left"},
                {"id": "page", "title": "Beta page", "view": "PagePanel", "placement": "center"}
            ]),
            false,
        ),
        &source(false),
    );
}

fn install_broken(data: &Path) {
    write_plugin(
        data,
        "broken",
        BROKEN,
        &manifest(
            BROKEN,
            "Broken",
            json!([
                {"id": "page", "title": "Broken page", "view": "PagePanel", "placement": "center"}
            ]),
            false,
        ),
        BROKEN_SOURCE,
    );
}

fn manifest(id: &str, name: &str, panels: serde_json::Value, command: bool) -> serde_json::Value {
    let mut value = json!({
        "id": id,
        "name": name,
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A surface fixture.",
        "powers": {},
        "ui": {}
    });
    value["ui"]["panels"] = panels;
    if command {
        value["commands"] = json!([{ "id": "open", "title": "Open", "when": "always" }]);
    }
    value
}

fn source(command: bool) -> String {
    let handle = if command {
        r#"async function handle(job) {
  if (job.kind === "command" && job.command === "open") {
    await openPanel("tool", { mark: "from-host" });
    return null;
  }
  throw new Error("unknown");
}"#
    } else {
        r#"async function handle(job) {
  throw new Error("unknown " + job.kind);
}"#
    };
    format!(
        r#"// @ts-check
import {{ View, div }} from "gpui-kit";
import {{ Input, InputState, Popover }} from "gpui-base";
import {{ KgButton }} from "gpui-component";
import {{ context, onContextChanged, openPanel }} from "kagan/host";
import {{ fail, next, reply }} from "kagan/jobs";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        handle(job).then((value) => reply(job.id, value), (error) => fail(job.id, String(error)));
      }}
    }});
  }}
  render() {{ return ""; }}
}}
{tool}
{page}
{side}
{handle}
"#,
        tool = panel_class("ToolPanel", "alpha-tool", "alpha-ready", false),
        page = panel_class("PagePanel", "alpha-page", "page-ready", true),
        side = panel_class("SidePanel", "alpha-side", "side-ready", false),
        handle = handle,
    )
    .replace(
        "alpha-tool",
        if command { "alpha-tool" } else { "beta-tool" },
    )
    .replace(
        "alpha-ready",
        if command { "alpha-ready" } else { "beta-ready" },
    )
    .replace(
        "alpha-page",
        if command { "alpha-page" } else { "beta-page" },
    )
    .replace(
        "page-ready",
        if command {
            "page-ready"
        } else {
            "beta-page-ready"
        },
    )
    .replace(
        "alpha-side",
        if command { "alpha-side" } else { "beta-side" },
    )
}

fn panel_class(name: &str, button: &str, word: &str, page: bool) -> String {
    let field = if page {
        r#"this.pop = false;
    this.field = InputState.new({ value: "" });
    this.typed = "";
    this.field.on("change", (_event, cx) => {
      this.typed = this.field.value();
      cx.notify();
    });"#
    } else {
        ""
    };
    let extra = if page {
        format!(
            r#".child(Input.new(this.field)).child(new KgButton("{button}-typed").label(this.typed || "empty")).child(Popover.new("page-pop").open(this.pop).on_open_change((open, cx) => {{ this.pop = open; cx.notify(); }}).trigger(new KgButton("{button}-pop-open").label("Open pop")).content(new KgButton("{button}-pop").label("pop")))"#
        )
    } else {
        String::new()
    };
    format!(
        r#"export class {name} extends View {{
  init(_props, cx) {{
    this.word = "{word}";
    this.count = 0;
    {field}
    cx.spawn(async () => {{
      const read = (value) => {{
        if (value && value.props && value.props.mark) this.word = String(value.props.mark);
      }};
      read(context());
      cx.notify();
      for (;;) {{
        read(await onContextChanged());
        cx.notify();
      }}
    }});
  }}
  render() {{
    return div()
      {extra}
      .child(new KgButton("{button}").label(this.word))
      .child(new KgButton("{button}-count").label(String(this.count)).on_click((_event, cx) => {{
        this.count++;
        cx.notify();
      }}));
  }}
}}"#
    )
}

const BROKEN_SOURCE: &str = r#"// @ts-check
import { View } from "gpui-kit";
export default class Plugin extends View {}
export class PagePanel extends View {
  render() { throw new Error("boom in render"); }
}
"#;

fn write_plugin(data: &Path, folder: &str, id: &str, manifest: &serde_json::Value, entry: &str) {
    let root = data.join(folder);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), entry).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": {}}),
            error: None,
        })
        .unwrap();
}

fn wide(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
}

fn run(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task)
}

fn surfaces(cx: &mut TestAppContext) -> Vec<SurfaceItem> {
    match run(cx, Op::Surface(SurfaceOp::List)).unwrap() {
        OpResult::Surfaces(items) => items,
        other => panic!("{other:?}"),
    }
}

fn open_surface(cx: &mut TestAppContext, plugin: &str, id: &str) -> Result<OpResult, OpError> {
    run(
        cx,
        Op::Surface(SurfaceOp::Open {
            plugin: PluginId(plugin.into()),
            id: id.into(),
        }),
    )
}

fn close_surface(cx: &mut TestAppContext, plugin: &str, id: &str) -> Result<OpResult, OpError> {
    run(
        cx,
        Op::Surface(SurfaceOp::Close {
            plugin: PluginId(plugin.into()),
            id: id.into(),
        }),
    )
}

fn item<'a>(items: &'a [SurfaceItem], plugin: &str, id: &str) -> &'a SurfaceItem {
    items
        .iter()
        .find(|item| item.plugin.0 == plugin && item.id == id)
        .unwrap_or_else(|| panic!("{plugin} {id} missing from {items:?}"))
}

fn width_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(0., |element| f32::from(element.bounds().size.width))
    })
    .unwrap()
}

fn height_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(0., |element| f32::from(element.bounds().size.height))
    })
    .unwrap()
}

fn body(plugin: &str, id: &str) -> String {
    format!("panel-{plugin}-{id}-body")
}

fn box_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> (f32, f32, f32, f32) {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into())).map_or(
            (0., 0., 0., 0.),
            |element| {
                let bounds = element.bounds();
                (
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                )
            },
        )
    })
    .unwrap()
}

#[cfg(unix)]
#[gpui_kit::test]
fn add_plugin_starts_a_thread_and_pastes_without_enter(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(
        cx,
        data.path(),
        "printf 'Ready> '\nread line\nprintf 'SUBMITTED:%s' \"$line\"\nsleep 30",
    );
    assert_eq!(label(cx, window, "plugins-add"), "New plugin…");
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| {
        super::kagan::grid_text(cx, &kagan).contains("Build a kagan plugin that")
    });
    let text = super::kagan::grid_text(cx, &kagan);
    assert!(!text.contains("SUBMITTED:"));
    let id = cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap();
    let thread = thread_record(cx, &id);
    assert_eq!(thread.title, "New plugin");
    assert_eq!(thread.project_id.0, "personal");
}

#[cfg(unix)]
#[gpui_kit::test]
fn add_plugin_offers_copy_when_output_never_settles(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    let (kagan, window, data) = open(cx);
    plugin_agent(
        cx,
        data.path(),
        "i=0; while :; do printf '%s\\n' \"$i\"; i=$((i + 1)); sleep 0.1; done",
    );
    click(cx, window, "plugins-add");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !shows(cx, window, "plugin-prompt-notice") {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "no copy notice");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!super::kagan::grid_text(cx, &kagan).contains("Build a kagan plugin that"));
}

#[cfg(unix)]
fn plugin_agent(cx: &mut TestAppContext, data: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = data.join("agent-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("claude");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    use_path(cx, &bin);
}

/// Finds programs in `bin` only, so the agents on this machine do not matter.
fn use_path(cx: &mut TestAppContext, bin: &Path) {
    use_environment(cx, bin, Path::new("/bin/sh"));
}

fn use_environment(cx: &mut TestAppContext, bin: &Path, shell: &Path) {
    let (bin, shell) = (bin.to_path_buf(), shell.to_path_buf());
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(&bin, &shell);
        });
    });
}

#[cfg(unix)]
fn wait_for_plugin(
    cx: &mut TestAppContext,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) -> ThreadId {
    wait_until(cx, |cx| {
        super::kagan::grid_text(cx, kagan).contains("Ready>")
    });
    cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap()
}

#[cfg(unix)]
fn pump_for(cx: &mut TestAppContext, duration: std::time::Duration) {
    let until = std::time::Instant::now() + duration;
    while std::time::Instant::now() < until {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn screen_text(cx: &mut TestAppContext, id: &ThreadId) -> String {
    let grid = cx
        .update(|cx| Workspace::global(cx).read(cx).grid(id, None))
        .unwrap();
    grid.lines.iter().flatten().map(|cell| cell.c).collect()
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_ignores_a_second_click_while_starting(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    cx.update_window(window, |_, window, cx| {
        let id = ElementId::Name("plugins-add".into());
        crate::ui_dispatch::click(window, &id, cx).unwrap();
        crate::ui_dispatch::click(window, &id, cx).unwrap();
    })
    .unwrap();
    wait_for_plugin(cx, &kagan);
    pump_for(cx, std::time::Duration::from_secs(2));
    let count = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread_records(cx)
            .iter()
            .filter(|t| t.title == "New plugin")
            .count()
    });
    assert_eq!(count, 1);
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_starts_the_agent_not_the_last_program(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    super::kagan::run_program(cx, "/bin/sh");
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    click(cx, window, "plugins-add");
    let id = wait_for_plugin(cx, &kagan);
    assert_eq!(thread_record(cx, &id).provider, "claude");
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_says_so_when_no_agent_is_installed(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let empty = data.path().join("no-agents");
    std::fs::create_dir_all(&empty).unwrap();
    use_path(cx, &empty);
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| {
        label(cx, window, "note").contains("No agent is installed")
    });
    let started = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread_records(cx)
            .iter()
            .filter(|t| t.title == "New plugin")
            .count()
    });
    assert_eq!(started, 0);
}

#[gpui_kit::test]
fn selecting_a_project_targets_it_for_a_new_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    wide(cx, window);
    click(cx, window, "project-prj_kagan");
    wait_until(cx, |cx| {
        label(cx, window, "start-prompt").starts_with("New thread in")
    });
    assert!(!label(cx, window, "start-prompt").contains("Personal"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_on_a_project_starts_a_thread_there(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    wide(cx, window);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    tab_to(cx, window, "project-prj_kagan");
    press_key(cx, window, "enter");
    wait_until(cx, |cx| {
        let records = cx.update(|cx| Workspace::global(cx).read(cx).thread_records(cx));
        records
            .iter()
            .any(|t| t.project_id.0 == "prj_kagan" && t.provider == "claude")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn typing_cancels_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    run(cx, Op::Plugin(PluginOp::Start)).unwrap();
    let id = wait_for_plugin(cx, &kagan);
    cx.simulate_keystrokes(window, "x");
    pump_for(cx, std::time::Duration::from_secs(2));
    let text = screen_text(cx, &id);
    assert!(text.contains('x'));
    assert!(!text.contains("Build a kagan plugin"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn switching_threads_cancels_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    let other = super::kagan::run_program(cx, "/bin/sh");
    click(cx, window, "plugins-add");
    let id = wait_for_plugin(cx, &kagan);
    click(cx, window, &format!("thread-{}", other.0));
    pump_for(cx, std::time::Duration::from_secs(2));
    assert!(!screen_text(cx, &id).contains("Build a kagan plugin"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn closing_the_window_cancels_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    click(cx, window, "plugins-add");
    let id = wait_for_plugin(cx, &kagan);
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    drop(kagan);
    pump_for(cx, std::time::Duration::from_secs(2));
    assert!(!screen_text(cx, &id).contains("Build a kagan plugin"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_paused_splash_can_receive_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(
        cx,
        data.path(),
        "printf 'Ready> '; sleep 8; printf 'SECOND'; read line; sleep 30",
    );
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| {
        super::kagan::grid_text(cx, &kagan).contains("Build a kagan plugin that")
    });
    assert!(!super::kagan::grid_text(cx, &kagan).contains("SECOND"));
}

#[gpui_kit::test]
fn list_open_and_close_reach_every_placement(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.run_until_parked();

    let listed = surfaces(cx);
    assert_eq!(
        item(&listed, ALPHA, "tool").placement,
        SurfacePlacement::Left
    );
    assert_eq!(
        item(&listed, ALPHA, "page").placement,
        SurfacePlacement::Center
    );
    assert_eq!(
        item(&listed, ALPHA, "side").placement,
        SurfacePlacement::Right
    );
    assert_eq!(item(&listed, BETA, "tool").id, "tool");
    assert!(!item(&listed, ALPHA, "tool").open);
    let tool = format!("surface-item-{ALPHA}-tool");
    let page = format!("surface-item-{ALPHA}-page");
    assert!(shows(cx, window, &tool));
    assert!(shows(cx, window, &page));
    assert!(shows(cx, window, "plugins-manage"));
    assert!(!shows(cx, window, "plugins"));
    assert!(!shows(cx, window, &format!("surface-item-{ALPHA}-side")));
    assert!(shows(cx, window, "project-prj_kagan"));
    assert_eq!(label(cx, window, &tool), "Alpha tool");

    click(cx, window, &tool);
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert_eq!(selected_of(cx, window, &tool), Some(true));
    let opened = surfaces(cx);
    let tool_open = item(&opened, ALPHA, "tool").visible || item(&opened, BETA, "tool").visible;
    assert!(tool_open, "{opened:?}");

    let which = if item(&opened, ALPHA, "tool").open {
        ALPHA
    } else {
        BETA
    };
    close_surface(cx, which, "tool").unwrap();
    assert!(!shows(cx, window, "alpha-tool"));
    assert!(!shows(cx, window, "beta-tool"));

    open_and_close_the_page(cx, window);
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    assert!(shows(cx, window, "right-tab-bar"));
    assert!(item(&surfaces(cx), ALPHA, "side").visible);
    close_surface(cx, ALPHA, "side").unwrap();
    assert!(!shows(cx, window, "alpha-side"));

    let _ = kagan;
}

fn open_and_close_the_page(cx: &mut TestAppContext, window: AnyWindowHandle) {
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    assert!(shows(cx, window, "surface-title"));
    assert_eq!(label(cx, window, "surface-close"), "Close");
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
    assert!(!shows(cx, window, "thread-title"));

    press_key(cx, window, "escape");
    assert!(!shows(cx, window, "surface-page"));
    open_surface(cx, ALPHA, "page").unwrap();
    press_key(cx, window, "space");
    assert!(!shows(cx, window, "surface-page"));
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-close"));
    click(cx, window, "surface-close");
    assert!(!shows(cx, window, "surface-page"));
}

#[gpui_kit::test]
fn a_narrow_window_still_opens_a_page_after_a_right_panel(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    let right = saved(cx, "main.right-sidebar");

    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, &body(ALPHA, "side")));
    assert!(shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), right);

    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert!(width_of(cx, window, "center") >= 480. - 0.5);
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
}

#[gpui_kit::test]
fn settings_and_a_page_do_not_show_together(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );

    click(cx, window, "settings-gear");
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    assert!(!shows(cx, window, "surface-page"));
    assert!(cx.update(|cx| kagan.read(cx).shown_thread(cx)).is_none());

    open_surface(cx, ALPHA, "page").unwrap();
    assert!(shows(cx, window, "surface-page"));
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn a_page_keeps_the_running_terminal_and_a_thread_switch_leaves_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let first = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(first.clone())
    });
    let view = cx.update(|cx| screen_read::terminal_view(kagan.read(cx)));
    run(
        cx,
        Op::Thread(ThreadOp::Input {
            id: first.clone(),
            text: "printf SURFACEMARK\\n".into(),
            terminal: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| screen(cx, &first).contains("SURFACEMARK"));

    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert!(!shows(cx, window, "thread-title"));
    run(
        cx,
        Op::Thread(ThreadOp::Input {
            id: first.clone(),
            text: "printf PAGEUP\\n".into(),
            terminal: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| screen(cx, &first).contains("PAGEUP"));
    assert_eq!(thread_record(cx, &first).status, ThreadStatus::Active);
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );

    close_surface(cx, ALPHA, "page").unwrap();
    assert!(shows(cx, window, "thread-title"));
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );
    assert!(screen(cx, &first).contains("PAGEUP"));

    open_surface(cx, ALPHA, "page").unwrap();
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", second.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(second.clone())
    });
    assert!(!shows(cx, window, "surface-page"));
    assert_eq!(thread_record(cx, &first).status, ThreadStatus::Active);
    assert!(screen(cx, &first).contains("SURFACEMARK"));
    assert!(!item(&surfaces(cx), ALPHA, "page").open);
}

#[gpui_kit::test]
fn open_panel_props_stay_on_the_plugin_that_sent_them(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let command = run(
        cx,
        Op::Plugin(PluginOp::Command {
            id: PluginId(ALPHA.into()),
            command: "open".into(),
            thread_id: None,
            project_id: None,
        }),
    );
    command.unwrap();
    wait_until(cx, |cx| label(cx, window, "alpha-tool") == "from-host");
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);

    open_surface(cx, BETA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "beta-tool"));
    assert_eq!(label(cx, window, "beta-tool"), "beta-ready");
    assert!(!shows(cx, window, "alpha-tool"));
}

#[gpui_kit::test]
fn reload_disable_and_removal_drop_one_plugin_and_leave_the_other(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));

    let entry = data.path().join("alpha").join("main.js");
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
    assert_eq!(item(&surfaces(cx), BETA, "page").id, "page");

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ALPHA.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), BETA, "page").open);
    open_surface(cx, BETA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "beta-page"));

    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: PluginId(BETA.into()),
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
            .id()
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "surface-page"));
    assert!(surfaces(cx).iter().all(|item| item.plugin.0 != BETA));
    let _ = kagan;
}

#[gpui_kit::test]
fn a_page_that_fails_to_draw_offers_recovery_and_close(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let view = cx.update(|cx| screen_read::terminal_view(kagan.read(cx)));
    open_surface(cx, BROKEN, "page").unwrap();
    let failed = format!("panel-{BROKEN}-page");
    wait_until(cx, |cx| shows(cx, window, &format!("{failed}-reload")));
    assert!(shows(cx, window, &format!("{failed}-logs")));
    assert!(shows(cx, window, &format!("{failed}-disable")));
    assert!(shows(cx, window, "surface-close"));
    click(cx, window, "surface-close");
    assert!(!shows(cx, window, "surface-page"));
    assert!(shows(cx, window, "thread-title"));
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );
}

#[gpui_kit::test]
fn light_and_dark_keep_the_same_page_box(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    set_mode(cx, Appearance::Dark);
    let dark = (
        width_of(cx, window, "surface-page"),
        height_of(cx, window, "surface-title"),
    );
    set_mode(cx, Appearance::Light);
    let light = (
        width_of(cx, window, "surface-page"),
        height_of(cx, window, "surface-title"),
    );
    assert!((dark.0 - light.0).abs() < 0.5, "width {dark:?} {light:?}");
    assert!((dark.1 - light.1).abs() < 0.5, "title {dark:?} {light:?}");
    assert!(dark.0 > 0. && dark.1 > 0.);
}

fn origin_x(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .unwrap_or_else(|| panic!("{name} is not drawn"))
            .bounds()
            .origin
            .x
            .as_f32()
    })
    .unwrap()
}

fn origin_y(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .unwrap_or_else(|| panic!("{name} is not drawn"))
            .bounds()
            .origin
            .y
            .as_f32()
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_page_body_lines_up_with_its_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    let page = origin_x(cx, window, "surface-page");
    let title = origin_x(cx, window, "surface-title");
    let marker = origin_x(cx, window, &body(ALPHA, "page"));
    let content = origin_x(cx, window, "alpha-page");
    assert!(
        (content - title).abs() < 1.5,
        "content {content} title {title} page {page}"
    );
    assert!((marker - page).abs() < 1., "body {marker} page {page}");
    assert!(height_of(cx, window, &body(ALPHA, "page")) > 0.);
    let before = origin_y(cx, window, "alpha-page");
    run(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: "alpha-page".into(),
            within: None,
            dy: -200.,
            window: 0,
        }),
    )
    .unwrap();
    assert!(
        (origin_y(cx, window, "alpha-page") - before).abs() < 0.5,
        "a full page scrolled"
    );
}

#[gpui_kit::test]
fn an_open_draws_a_right_panel_tab(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(1120.), px(720.)));
    cx.run_until_parked();
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    assert!(item(&surfaces(cx), ALPHA, "side").visible);
    assert!(shows(cx, window, "right-tab-bar"));
    assert!(shows(cx, window, "right-tab-add"));
    assert!(shows(cx, window, &format!("right-tab-panel-{ALPHA}-side")));
    assert!(shows(cx, window, "needs-you-header"));
    assert!(!present(cx, window, "right-tab-close-needs-you"));

    click(cx, window, &format!("right-tab-close-panel-{ALPHA}-side"));
    wait_until(cx, |cx| !shows(cx, window, &body(ALPHA, "side")));
    assert!(!item(&surfaces(cx), ALPHA, "side").open);
    assert_eq!(focused_of(cx, window, "right-tab-add"), Some(true));

    click(cx, window, "right-tab-add");
    click(cx, window, &format!("right-add-{ALPHA}-side"));
    wait_until(cx, |cx| shows(cx, window, &body(ALPHA, "side")));
    assert_eq!(
        selected_of(cx, window, &format!("right-tab-panel-{ALPHA}-side")),
        Some(true)
    );
}

const WEB: &str = "dev.test.webview";

const WEB_SOURCE: &str = r#"// @ts-check
import { View } from "gpui-kit";
import { WebView } from "gpui-component";
export default class Plugin extends View {
  render() { return ""; }
}
export class Page extends View {
  render() {
    return new WebView("editor").url("http://127.0.0.1:1/");
  }
}
export class Fetch extends View {
  render() {
    return new WebView("fetch").url("https://files.example/draw");
  }
}
export class Off extends View {
  render() {
    return new WebView("off").url("https://example.com/");
  }
}
"#;

fn install_webview(data: &Path) {
    let mut spec = manifest(
        WEB,
        "Web view",
        json!([
            {"id": "page", "title": "Editor", "view": "Page", "placement": "right"},
            {"id": "fetch", "title": "Fetch", "view": "Fetch", "placement": "right"},
            {"id": "off", "title": "Off", "view": "Off", "placement": "right"}
        ]),
        false,
    );
    spec["powers"]["fetch"] = json!([{ "host": "files.example", "methods": ["GET"] }]);
    write_plugin(data, "webview", WEB, &spec, WEB_SOURCE);
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .set_plugin_approved(
            &PluginId(WEB.into()),
            &json!({
                "powers": { "fetch": [{ "host": "files.example", "methods": ["GET"] }] }
            }),
        )
        .unwrap();
}

fn page_place(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &str,
) -> (Option<bool>, String) {
    let note = label(cx, window, &format!("webview-{id}-note"));
    let native = cx
        .update_window(window, |_, _, cx| crate::components::native_shown(cx, id))
        .unwrap();
    (native, note)
}

#[gpui_kit::test]
fn a_plugin_right_panel_mounts_a_web_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_webview);
    wide(cx, window);
    open_surface(cx, WEB, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "webview-editor"));
    cx.run_until_parked();
    let page = box_of(cx, window, "webview-editor");
    let side = box_of(cx, window, "right-sidebar");
    let panel = box_of(cx, window, &body(WEB, "page"));
    assert!(
        (page.0 - panel.0).abs() < 1.
            && (page.1 - panel.1).abs() < 1.
            && (page.2 - panel.2).abs() < 1.
            && (page.3 - panel.3).abs() < 1.,
        "page {page:?} panel {panel:?}"
    );
    assert!(
        (page.0 - side.0).abs() < 1. && (page.2 - side.2).abs() < 1.,
        "page {page:?} side {side:?}"
    );
    assert!(page.1 > side.1, "page {page:?} side {side:?}");
    assert!(
        ((page.1 + page.3) - (side.1 + side.3)).abs() < 1.,
        "page {page:?} side {side:?}"
    );
    let (native, note) = page_place(cx, window, "editor");
    assert!(
        native == Some(true) || note.contains("could not open"),
        "a 127.0.0.1 page did not place: native {native:?} note {note:?}"
    );

    open_surface(cx, WEB, "fetch").unwrap();
    wait_until(cx, |cx| shows(cx, window, "webview-fetch"));
    cx.run_until_parked();
    let (native, note) = page_place(cx, window, "fetch");
    assert!(
        native == Some(true) || note.contains("could not open"),
        "a powers.fetch host did not place: native {native:?} note {note:?}"
    );

    open_surface(cx, WEB, "off").unwrap();
    wait_until(cx, |cx| shows(cx, window, "webview-off-note"));
    assert!(!shows(cx, window, "webview-editor"));
    assert_eq!(
        label(cx, window, "webview-off-note"),
        crate::components::REFUSED_HOST
    );
    let hidden = cx
        .update_window(window, |_, _, cx| {
            crate::components::native_shown(cx, "editor")
        })
        .unwrap();
    assert_ne!(hidden, Some(true));

    click(cx, window, &format!("right-tab-panel-{WEB}-page"));
    wait_until(cx, |cx| shows(cx, window, "webview-editor"));
    cx.run_until_parked();
    let back = cx
        .update_window(window, |_, _, cx| {
            crate::components::native_shown(cx, "editor")
        })
        .unwrap();
    if back.is_some() {
        assert_eq!(back, Some(true));
    }
}

#[gpui_kit::test]
fn full_screen_gives_the_right_sidebar_the_window_and_gives_it_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    let right = width_of(cx, window, "right-sidebar");

    click(cx, window, "right-full");
    assert!(!shows(cx, window, "center"));
    assert!(width_of(cx, window, "right-sidebar") > right);
    assert_eq!(selected_of(cx, window, "right-full"), Some(true));

    click(cx, window, "right-full");
    assert!(shows(cx, window, "center"));
    assert!((width_of(cx, window, "right-sidebar") - right).abs() < 0.5);

    click(cx, window, "right-full");
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert_eq!(selected_of(cx, window, "right-full"), Some(false));
}

const BLANK: &str = "dev.test.blank";

const BLANK_SOURCE: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
export default class Plugin extends View {
  render() { return ""; }
}
export class ToolPanel extends View {
  render() { return div(); }
}
export class SidePanel extends View {
  render() { return div(); }
}
export class PagePanel extends View {
  render() { return div(); }
}
"#;

const FILL: &str = "dev.test.fill";

const FILL_SOURCE: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
import { context, onContextChanged } from "kagan/host";
export default class Plugin extends View {
  render() { return ""; }
}
export class SidePanel extends View {
  init(_props, cx) {
    this.word = "";
    const apply = (value) => {
      const mark = value && value.props && value.props.mark;
      this.word = mark ? String(mark) : "";
    };
    apply(context());
    cx.spawn(async () => {
      for (;;) {
        apply(await onContextChanged());
        cx.notify();
      }
    });
  }
  render() {
    if (!this.word) return div();
    return div().child(new KgButton("fill-side").label(this.word));
  }
}
"#;

fn install_fill(data: &Path) {
    write_plugin(
        data,
        "fill",
        FILL,
        &manifest(
            FILL,
            "Fill",
            json!([
                {"id": "side", "title": "Fill side", "view": "SidePanel", "placement": "left"}
            ]),
            false,
        ),
        FILL_SOURCE,
    );
}

fn install_blank(data: &Path) {
    write_plugin(
        data,
        "blank",
        BLANK,
        &manifest(
            BLANK,
            "Blank",
            json!([
                {"id": "tool", "title": "Blank tool", "view": "ToolPanel", "placement": "left"},
                {"id": "side", "title": "Blank side", "view": "SidePanel", "placement": "right"},
                {"id": "page", "title": "Blank page", "view": "PagePanel", "placement": "center"}
            ]),
            false,
        ),
        BLANK_SOURCE,
    );
}

fn saved(cx: &mut TestAppContext, key: &str) -> serde_json::Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: key.into() }), cx)
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn tab_to(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) {
    for _ in 0..80 {
        if focused_of(cx, window, id) == Some(true) {
            return;
        }
        cx.update_window(window, |_, window, cx| window.focus_next(cx))
            .unwrap();
        cx.run_until_parked();
    }
    panic!("tab did not reach {id}");
}

#[gpui_kit::test]
fn a_failed_open_keeps_the_saved_layout(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_blank);
    wide(cx, window);
    click(cx, window, "left-sidebar-toggle");
    assert!(!shows(cx, window, "project-prj_kagan"));
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
    let right = saved(cx, "main.right-sidebar");

    let missed = open_surface(cx, BLANK, "tool").unwrap_err();
    assert!(missed.to_string().contains("not on screen"), "{missed}");
    cx.run_until_parked();
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
    assert_eq!(saved(cx, "main.right-sidebar"), right);
    assert!(!shows(cx, window, "project-prj_kagan"));
    assert!(!item(&surfaces(cx), BLANK, "tool").open);

    open_surface(cx, BLANK, "side").unwrap();
    wait_until(cx, |cx| item(&surfaces(cx), BLANK, "side").open);
    assert!(item(&surfaces(cx), BLANK, "side").visible);
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
}

#[gpui_kit::test]
fn opening_the_same_surface_again_keeps_its_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-tool-count"));
    click(cx, window, "alpha-tool-count");
    assert_eq!(label(cx, window, "alpha-tool-count"), "1");

    open_surface(cx, ALPHA, "tool").unwrap();
    assert_eq!(label(cx, window, "alpha-tool-count"), "1");
    assert_eq!(label(cx, window, "alpha-tool"), "alpha-ready");

    run(
        cx,
        Op::Plugin(PluginOp::Command {
            id: PluginId(ALPHA.into()),
            command: "open".into(),
            thread_id: None,
            project_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| label(cx, window, "alpha-tool") == "from-host");
    assert_eq!(label(cx, window, "alpha-tool-count"), "1");
}

#[gpui_kit::test]
fn enter_and_space_open_a_plugin_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    tab_to(cx, window, &tool);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    close_surface(cx, ALPHA, "tool").unwrap();
    tab_to(cx, window, &tool);
    press_key(cx, window, "space");
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    close_surface(cx, ALPHA, "tool").unwrap();
}

#[gpui_kit::test]
fn a_plugin_input_and_button_keep_the_page_open(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));

    cx.update_window(window, |_, window, cx| window.focus_next(cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_input(window, "hi");
    cx.run_until_parked();
    wait_until(cx, |cx| label(cx, window, "alpha-page-typed") == "hi");
    press_key(cx, window, "space");
    press_key(cx, window, "enter");
    assert!(shows(cx, window, "surface-page"));
    assert!(label(cx, window, "alpha-page-typed").contains("hi"));

    click(cx, window, "alpha-page-pop-open");
    wait_until(cx, |cx| shows(cx, window, "alpha-page-pop"));
    press_key(cx, window, "escape");
    assert!(shows(cx, window, "surface-page"));
    assert!(!shows(cx, window, "alpha-page-pop"));

    tab_to(cx, window, "alpha-page-count");
    press_key(cx, window, "enter");
    press_key(cx, window, "space");
    assert_eq!(label(cx, window, "alpha-page-count"), "2");
    assert!(shows(cx, window, "surface-page"));
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
}

fn install_pair_and_blank(data: &Path) {
    install_pair(data);
    install_blank(data);
}

fn titles(cx: &mut TestAppContext, kagan: &gpui_kit::Entity<crate::kagan::Kagan>) -> Vec<String> {
    cx.update(|cx| kagan.read(cx).panel_titles())
}

fn terminal_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_view(kagan.read(cx)).is_some_and(|view| view.read(cx).focused(window))
    })
    .unwrap()
}

fn host_opens(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    opens: Vec<(&str, &str, serde_json::Value)>,
) -> Vec<async_channel::Receiver<Result<(), String>>> {
    cx.update_window(window, |_, _, cx| {
        let ui = kg_plugins::HostUi::global(cx).expect("host");
        let mut replies = Vec::new();
        for (plugin, panel, props) in opens {
            let (reply, incoming) = async_channel::bounded(1);
            ui.update(cx, |_, cx| {
                cx.emit(kg_plugins::HostUiEvent::OpenPanel {
                    plugin: PluginId(plugin.into()),
                    panel: panel.into(),
                    props,
                    reply,
                });
            });
            replies.push(incoming);
        }
        replies
    })
    .unwrap()
}

fn replied(receiver: &async_channel::Receiver<Result<(), String>>) -> Result<(), String> {
    receiver
        .try_recv()
        .unwrap_or_else(|error| panic!("no reply: {error}"))
}

fn show_thread(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
}

fn focus_page_input(cx: &mut TestAppContext, window: AnyWindowHandle) {
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));
    cx.update_window(window, |_, window, cx| window.focus_next(cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_input(window, "hi");
    cx.run_until_parked();
    wait_until(cx, |cx| label(cx, window, "alpha-page-typed") == "hi");
}

#[gpui_kit::test]
fn overlapping_host_opens_keep_the_first_request(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair_and_blank);
    wide(cx, window);

    let replies = host_opens(
        cx,
        window,
        vec![
            (ALPHA, "tool", json!({"mark": "one"})),
            (ALPHA, "page", json!({"mark": "two"})),
        ],
    );
    cx.run_until_parked();
    let first = replied(&replies[0]);
    assert!(first.is_ok(), "{first:?}");
    let refused = replied(&replies[1]).unwrap_err();
    assert!(refused.contains("already waiting"), "{refused}");
    wait_until(cx, |cx| label(cx, window, "alpha-tool") == "one");
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);
    assert!(!item(&surfaces(cx), ALPHA, "page").open);

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(BLANK.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
    assert!(!titles(cx, &kagan).iter().any(|title| title == "Blank side"));
    click(cx, window, "right-sidebar-toggle");
    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(BLANK.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
    assert!(!titles(cx, &kagan).iter().any(|title| title == "Blank side"));

    let replies = host_opens(
        cx,
        window,
        vec![
            (BLANK, "tool", json!({"mark": "gone"})),
            (ALPHA, "page", json!({"mark": "nope"})),
        ],
    );
    cx.run_until_parked();
    let missed = replied(&replies[0]).unwrap_err();
    assert!(missed.contains("not on screen"), "{missed}");
    let refused = replied(&replies[1]).unwrap_err();
    assert!(refused.contains("already waiting"), "{refused}");
    assert_eq!(label(cx, window, "alpha-tool"), "one");
    assert!(!item(&surfaces(cx), ALPHA, "page").open);
    assert!(!shows(cx, window, &body(BLANK, "tool")));
    assert!(!titles(cx, &kagan).iter().any(|title| title == "Blank side"));
}

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

fn set_plugin(cx: &mut TestAppContext, enabled: bool) {
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

fn reload_moves_focus_to_close(cx: &mut TestAppContext, window: AnyWindowHandle, data: &Path) {
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

fn disable_moves_focus_to_the_terminal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    focus_page_input(cx, window);
    set_plugin(cx, false);
    assert!(!shows(cx, window, "surface-page"));
    assert!(terminal_focused(cx, window, kagan));
}

fn disable_leaves_terminal_focus(
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

fn disable_moves_tool_focus_to_the_trigger(cx: &mut TestAppContext, window: AnyWindowHandle) {
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

fn removal_moves_focus_to_the_terminal(
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
            .id()
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "surface-page"));
    assert!(terminal_focused(cx, window, kagan));
}

fn screen(cx: &mut TestAppContext, id: &ThreadId) -> String {
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

fn set_mode(cx: &mut TestAppContext, mode: Appearance) {
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

fn read_list(
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

fn emit_open(
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

fn assert_replaced(reply: &async_channel::Receiver<Result<(), String>>) {
    let err = replied(reply).unwrap_err();
    assert!(err.contains("replaced"), "{err}");
}

fn tap_then_settings(
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

fn deliver_mark(cx: &mut TestAppContext, mark: Option<&str>) {
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

fn plugin_generation(cx: &mut TestAppContext, id: &str) -> Option<u64> {
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

fn disable_beta(cx: &mut TestAppContext) {
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(BETA.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
}

fn focus_tool_button(cx: &mut TestAppContext, window: AnyWindowHandle) {
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

fn close_keeps_the_tools_trigger(cx: &mut TestAppContext, window: AnyWindowHandle) {
    focus_tool_button(cx, window);
    close_surface(cx, ALPHA, "tool").unwrap();
    let tool = format!("surface-item-{ALPHA}-tool");
    assert!(shows(cx, window, &tool));
    assert_eq!(focused_of(cx, window, &tool), Some(true));
}

fn disable_moves_the_only_tool_to_the_terminal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    focus_tool_button(cx, window);
    set_plugin(cx, false);
    assert!(!present(cx, window, &format!("surface-item-{ALPHA}-tool")));
    assert!(terminal_focused(cx, window, kagan));
}

fn settings_keeps_its_focus_when_the_only_tool_goes(
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

fn a_hidden_sidebar_does_not_focus_tools(cx: &mut TestAppContext, window: AnyWindowHandle) {
    set_plugin(cx, true);
    focus_tool_button(cx, window);
    click(cx, window, "left-sidebar-toggle");
    let tool = format!("surface-item-{ALPHA}-tool");
    assert!(shows(cx, window, &tool));
    set_plugin(cx, false);
    assert_ne!(focused_of(cx, window, &tool), Some(true));
    click(cx, window, "left-sidebar-toggle");
}

fn terminal_focus_stays_when_the_only_tool_goes(
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

fn open_changed_file(cx: &mut TestAppContext, window: AnyWindowHandle, data: &Path) {
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

fn removal_of_the_only_tool_focuses_the_terminal(
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
            .id()
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!present(cx, window, &format!("surface-item-{ALPHA}-tool")));
    assert!(terminal_focused(cx, window, kagan));
}

#[gpui_kit::test]
fn leaving_settings_returns_to_the_row_that_opened_them(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    click(cx, window, "plugins-manage");
    assert!(shows(cx, window, "setting-row-install"));
    press_key(cx, window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(focused_of(cx, window, "plugins-manage"), Some(true));
}

#[gpui_kit::test]
fn closing_the_second_tool_returns_to_its_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, BETA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "beta-tool-count"));
    tab_to(cx, window, "beta-tool-count");
    close_surface(cx, BETA, "tool").unwrap();
    let beta = format!("surface-item-{BETA}-tool");
    assert_eq!(focused_of(cx, window, &beta), Some(true));
    assert!(!shows(cx, window, "beta-tool"));
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shows(cx, window, "beta-tool"));
    assert!(!shows(cx, window, "alpha-tool"));
}

fn narrow(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn cmd_b_and_focus_sidebar_open_the_narrow_overlay_and_escape_closes_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    let before = saved(cx, "main.left-sidebar");
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "left-overlay-panel"), Some(true));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(saved(cx, "main.left-sidebar"), before);

    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(saved(cx, "main.left-sidebar"), before);
}

#[gpui_kit::test]
fn escape_closes_the_overlay_before_the_form_under_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    cx.simulate_keystrokes(window, &chord("NewThread"));
    wait_until(cx, |cx| shows(cx, window, "start-prompt"));
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "start-prompt"));
}

#[gpui_kit::test]
fn clicking_the_dimmed_center_closes_the_narrow_overlay(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    let before = saved(cx, "main.left-sidebar");
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    let alpha = cx.update(|cx| crate::kagan::layout_read::left_scrim_alpha(kagan.read(cx), cx));
    assert!(alpha > 0. && alpha < 1., "scrim alpha {alpha}");
    // The panel covers the scrim's middle, so click the dimmed part to its right.
    let panel_right =
        origin_x(cx, window, "left-overlay-panel") + width_of(cx, window, "left-overlay-panel");
    let point = gpui_kit::point(px(panel_right + 60.), px(300.));
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click_at(window, point, cx);
    })
    .unwrap();
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(saved(cx, "main.left-sidebar"), before);
}

#[gpui_kit::test]
fn picking_a_thread_in_the_narrow_overlay_closes_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    narrow(cx, window);
    let before = saved(cx, "main.left-sidebar");
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
    assert_eq!(saved(cx, "main.left-sidebar"), before);
}

#[gpui_kit::test]
fn picking_a_project_in_the_narrow_overlay_shows_its_new_thread_form(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, "project-prj_kagan");
    wait_until(cx, |cx| shows(cx, window, "start-prompt"));
    assert!(!shows(cx, window, "left-overlay-panel"));
}

#[gpui_kit::test]
fn the_project_fold_button_ends_before_the_new_thread_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    assert_chevron_clear(cx, window, "a wide window");
}

#[gpui_kit::test]
fn the_project_fold_button_ends_before_the_plus_in_a_narrow_list(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    super::kagan::set_preference(cx, "main.left-width", json!(200.));
    wide(cx, window);
    cx.run_until_parked();
    assert_chevron_clear(cx, window, "a 200 point list");
}

#[gpui_kit::test]
fn the_project_fold_button_ends_before_the_plus_in_the_narrow_overlay(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    narrow(cx, window);
    assert_chevron_clear(cx, window, "the narrow overlay");
}

fn assert_chevron_clear(cx: &mut TestAppContext, window: AnyWindowHandle, when: &str) {
    focus_row(cx, window, "project-prj_kagan");
    let chevron_end = origin_x(cx, window, "project-disclose-prj_kagan")
        + width_of(cx, window, "project-disclose-prj_kagan");
    let plus_start = origin_x(cx, window, "project-new-prj_kagan");
    assert!(
        chevron_end <= plus_start + 0.5,
        "in {when}: chevron ends at {chevron_end}, + starts at {plus_start}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_shows_the_real_discovery_error_and_can_try_again(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let empty = data.path().join("no-agents");
    std::fs::create_dir_all(&empty).unwrap();
    use_environment(cx, &empty, Path::new("/nonexistent/kagan-shell"));
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| label(cx, window, "note").contains("the shell"));
    assert!(!label(cx, window, "note").contains("No agent"));

    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    click(cx, window, "plugins-add");
    wait_for_plugin(cx, &kagan);
    let id = cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap();
    assert_eq!(thread_record(cx, &id).title, "New plugin");
}

/// Checks that a menu opened from `button` sits under it, with its right edge on the button's.
fn assert_under_button(cx: &mut TestAppContext, window: AnyWindowHandle, button: &str, menu: &str) {
    let (bx, by) = (origin_x(cx, window, button), origin_y(cx, window, button));
    let (bw, bh) = (width_of(cx, window, button), height_of(cx, window, button));
    let (mx, my) = (origin_x(cx, window, menu), origin_y(cx, window, menu));
    let mw = width_of(cx, window, menu);
    assert!(
        (my - (by + bh)).abs() < 8.,
        "menu top {my}, button bottom {}",
        by + bh
    );
    assert!(
        (mx + mw - (bx + bw)).abs() < 8.,
        "menu right {}, button right {}",
        mx + mw,
        bx + bw
    );
}

#[gpui_kit::test]
fn the_thread_menu_from_a_row_opens_under_the_row_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    click(cx, window, &format!("thread-{}", id.0));
    let button = format!("thread-actions-{}", id.0);
    click(cx, window, &button);
    wait_until(cx, |cx| shows(cx, window, "popup-menu"));
    assert_under_button(cx, window, &button, "popup-menu");
}

#[gpui_kit::test]
fn the_title_bar_menu_still_opens_under_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "thread-actions-trigger");
    wait_until(cx, |cx| shows(cx, window, "popup-menu"));
    assert_under_button(cx, window, "thread-actions-trigger", "popup-menu");
}

#[gpui_kit::test]
fn the_project_menu_opens_under_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_pair);
    let folder = data.path().join("demo");
    std::fs::create_dir_all(&folder).unwrap();
    let project = super::kagan::add_project(cx, &folder);
    wide(cx, window);
    cx.run_until_parked();
    let button = format!("project-actions-{}", project.id.0);
    focus_row(cx, window, &format!("project-{}", project.id.0));
    click(cx, window, &button);
    // The project menu is gpui-kit's dropdown, so its first item stands in for the menu.
    wait_until(cx, |cx| shows(cx, window, "project-remove"));
    let (bx, by) = (origin_x(cx, window, &button), origin_y(cx, window, &button));
    let bh = height_of(cx, window, &button);
    let (item_x, item_y) = (
        origin_x(cx, window, "project-remove"),
        origin_y(cx, window, "project-remove"),
    );
    assert!(
        (item_y - (by + bh)).abs() < 40.,
        "menu item top {item_y}, button bottom {}",
        by + bh
    );
    assert!(
        (item_x - bx).abs() < 240.,
        "menu item left {item_x}, button left {bx}"
    );
}
