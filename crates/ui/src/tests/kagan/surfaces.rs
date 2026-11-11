#[cfg(unix)]
use crate::kagan::screen_read;
use std::path::Path;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use gpui_kit::Entity;
#[cfg(unix)]
use gpui_kit::Modifiers;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, Plugin, PluginId, PluginOp};

use kg_plugins::PluginHost;

#[cfg(unix)]
use crate::kagan::Kagan;

use super::{PAGE, PAGE_FRAME, open_with_plugins, shows, wait_until};
#[cfg(unix)]
use super::{cell_point, drag_to, grid_text, press, release, selection, shown_row, split_command};
#[cfg(unix)]
use super::{click, open_with_plugins_then, plugin_log, present, run_shell};

#[gpui_kit::test]
fn reloading_a_plugin_mounts_its_panel_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, true);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    let id = PluginId(PAGE.into());
    let opened = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(kg_domain::SurfaceOp::Open {
                plugin: id.clone(),
                id: "page".into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
    wait_until(cx, |cx| shows(cx, window, "page-button"));
    let before = cx
        .update(|cx| PluginHost::global(cx).read(cx).generation(&id))
        .expect("the page plugin has loaded");

    let reload = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Reload {
                id: id.clone(),
                thread_id: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(reload).unwrap();
    cx.run_until_parked();

    let after = cx
        .update(|cx| PluginHost::global(cx).read(cx).generation(&id))
        .expect("the page plugin is still loaded");
    assert!(after > before, "the reload did not load the plugin again");
    wait_until(cx, |cx| shows(cx, window, "page-button"));
    assert!(!shows(cx, window, PAGE_FRAME));
}

/// Installs a plugin whose only contribution is the overlay `Cover`, exported by `source`.
pub(in crate::tests) fn install_overlay(data: &Path, id: &str, source: &str) {
    let root = data.join(id);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": id,
        "name": "Cover",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Draws over the terminal.",
        "powers": {},
        "ui": { "overlay": "Cover" }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), source).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

/// Draws one box, plus whatever an attack adds to it.
pub(in crate::tests) fn overlay_source(attack: &str) -> String {
    format!(
        r##"import {{ View, div, image }} from "gpui-kit";
import {{ InputState, Popover }} from "gpui-base";
export default class Plugin extends View {{
  render() {{ return ""; }}
}}
export class Cover extends View {{
  init(_props, cx) {{
    this.focus = cx.focus_handle();
    {attack}
  }}
  render() {{
    return div().size_full().child(div().absolute().left(8).top(8).w(40).h(40).bg("#ff00ff")){attack_child};
  }}
}}
"##,
        attack = if attack.starts_with("cx.") {
            attack
        } else {
            ""
        },
        attack_child = if attack.starts_with(".child") {
            attack
        } else {
            ""
        },
    )
}

#[cfg(unix)]
pub(in crate::tests) fn grid_focus_and_selection_survive(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
    window: AnyWindowHandle,
) {
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, kagan).contains("picked"));
    let (row, _) = shown_row(cx, kagan, "picked");
    let start = cell_point(cx, window, kagan, row, 0);
    press(cx, window, start, 1, Modifiers::default());
    let end = cell_point(cx, window, kagan, row, 4);
    drag_to(cx, window, end);
    release(cx, window, end, 1, Modifiers::default());
    wait_until(cx, |cx| !selection(cx, kagan).is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_overlay_that_reaches_for_the_keyboard_is_refused_and_typing_goes_on(cx: &mut TestAppContext) {
    let plugin = "dev.test.grabbing";
    let attack =
        "cx.timer.every(20, () => { this.focus.focus(); this.input = InputState.new(); });";
    let (kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &overlay_source(attack));
    });
    cx.update_window(window, |_, window, _| window.activate_window())
        .unwrap();
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{plugin}");
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("cannot use the window")
    });
    wait_until(cx, |cx| !present(cx, window, &overlay));
    assert!(
        cx.update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap()
    );
    // Typed in the same tick as the timer: no frame runs in between.
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.simulate_input(window, "echo still''here");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("stillhere"));
    grid_focus_and_selection_survive(cx, &kagan, window);
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_overlay_with_a_press_outside_handler_is_refused_and_the_terminal_takes_presses(
    cx: &mut TestAppContext,
) {
    let plugin = "dev.test.outside";
    let attack = ".child(div().w(10).h(10).on_mouse_down_out((_e, cx) => cx.stop_propagation()))";
    let (kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &overlay_source(attack));
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("on_mouse_down_out")
    });
    assert!(!present(cx, window, &format!("plugin-overlay-{plugin}")));
    grid_focus_and_selection_survive(cx, &kagan, window);
}

/// A plugin with two right panels, each a `KgButton` named `count`. Both panels count clicks in
/// their label, so the same local id appears twice.
pub(in crate::tests) fn install_twin(data: &Path) {
    let root = data.join("plugins").join("twin");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.twin",
        "name": "Twin",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Two panels with one button id.",
        "powers": {},
        "ui": { "panels": [
            { "id": "a", "title": "A", "view": "PanelA", "placement": "left" },
            { "id": "b", "title": "B", "view": "PanelB", "placement": "center" }
        ] }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class Plugin extends View {
  render() { return ""; }
}
export class PanelA extends View {
  init(_props, _cx) { this.count = 0; }
  render() {
    return div().child(new KgButton("count").label(`A ${this.count}`)
      .on_click((_event, cx) => { this.count++; cx.notify(); }));
  }
}
export class PanelB extends View {
  init(_props, _cx) { this.count = 0; }
  render() {
    return div().child(new KgButton("count").label(`B ${this.count}`)
      .on_click((_event, cx) => { this.count++; cx.notify(); }));
  }
}
"#,
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.twin".into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({ "powers": {}, "downloads": {}, "folders": {} }),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

/// The label a control shows, as `kg ui snapshot` reports it.
pub(in crate::tests) fn observed_label(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.label.as_deref().map(str::to_owned))
    })
    .unwrap()
}

/// A click through the driver, as `kg ui click` sends it.
pub(in crate::tests) fn click_through_driver(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    within: Option<&str>,
) -> Result<(), OpError> {
    let within = within.map(|within| ElementId::Name(within.to_owned().into()));
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click_within(
            window,
            &ElementId::Name(name.to_owned().into()),
            within.as_ref(),
            cx,
        )
    })
    .unwrap()
}

pub(in crate::tests) fn run_op(cx: &mut TestAppContext, op: Op) -> OpResult {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task).unwrap()
}

pub(in crate::tests) fn open_surface(cx: &mut TestAppContext, plugin: &PluginId, id: &str) {
    let opened = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(kg_domain::SurfaceOp::Open {
                plugin: plugin.clone(),
                id: id.into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
}
