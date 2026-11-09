use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{
    InteractionOption, Op, OpError, OpResult, Plugin, PluginId, PluginOp, SurfaceOp,
    UI_PERSON_ONLY, UiOp,
};
use kg_plugins::PluginHost;
use serde_json::json;

use crate::tests::kagan::{
    PAGE, PAGE_FRAME, click, focused_of, install_page, open, open_with_plugins_then, present,
    press_key, shows, value_of, wait_until,
};

const BROKEN: &str = "panel-dev.test.broken-hello";

const HELLO: &str = "// @ts-check
import { View, div } from \"gpui-kit\";
import { KgButton } from \"gpui-component\";
export default class Plugin extends View {}
export class HelloPanel extends View {
  init() { this.count = 0; }
  render() {
    return div().child(
      new KgButton(\"count\")
        .label(`Count ${this.count}`)
        .on_click((_event, cx) => { this.count++; cx.notify(); }),
    );
  }
}
";

const BROKEN_HELLO: &str = "// @ts-check
import { View, div } from \"gpui-kit\";
export default class Plugin extends View {}
export class HelloPanel extends View {
  render() {
    throw new Error(\"boom in render\");
  }
}
";

fn install_panel(data: &Path, folder: &str, id: &str, entry: &str) {
    let root = data.join(folder);
    std::fs::create_dir(&root).unwrap();
    let manifest = json!({
        "id": id,
        "name": folder,
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A test panel.",
        "powers": {},
        "ui": {"panels": [{"id": "hello", "title": "Hello", "view": "HelloPanel", "placement": "left"}]}
    });
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
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

fn install_frame(data: &Path) {
    install_panel(data, "frame", "dev.test.frame", HELLO);
}

fn install_broken(data: &Path) {
    install_panel(data, "broken", "dev.test.broken", BROKEN_HELLO);
}

fn open_tool(cx: &mut TestAppContext, plugin: &str) {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(SurfaceOp::Open {
                plugin: PluginId(plugin.into()),
                id: "hello".into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

fn show_questions(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    if !shows(cx, window, "needs-you") {
        click(cx, window, "right-sidebar-toggle");
    }
}

fn run_op(cx: &mut TestAppContext, op: Op) -> OpResult {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task).unwrap()
}

fn plugin_log(cx: &mut TestAppContext, id: &str) -> String {
    match run_op(
        cx,
        Op::Plugin(PluginOp::Logs {
            id: PluginId(id.into()),
            follow: false,
            after: None,
        }),
    ) {
        OpResult::Logs { text, .. } => text,
        other => panic!("{other:?}"),
    }
}

/// The height the window drew for a named element, in points.
fn height_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(0., |element| f32::from(element.bounds.size.height))
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_plugin_panel_sits_below_the_threads_and_its_button_works(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_frame);
    cx.run_until_parked();
    open_tool(cx, "dev.test.frame");
    wait_until(cx, |cx| shows(cx, window, "count"));

    let height = height_of(cx, window, "plugin-tool-body");
    assert!(height > 200., "the panel is {height} points tall");
    click(cx, window, "count");
    cx.run_until_parked();
    assert!(
        shows(cx, window, "count"),
        "the button is still on screen after a click"
    );
}

#[gpui_kit::test]
fn a_panel_that_fails_to_draw_says_so_and_its_failure_reaches_status_and_the_log(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");

    let reload = format!("{BROKEN}-reload");
    wait_until(cx, |cx| shows(cx, window, &reload));
    for action in ["logs", "disable", "details"] {
        assert!(shows(cx, window, &format!("{BROKEN}-{action}")), "{action}");
    }

    // The watchdog samples every half second and writes the failure once.
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.test.broken").contains("boom in render")
    });
    let status = run_op(
        cx,
        Op::Status {
            thread_id: None,
            project_id: None,
            environment_id: None,
        },
    );
    let OpResult::Status { warnings, .. } = status else {
        panic!("status returned no warnings");
    };
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("boom in render")),
        "{warnings:?}"
    );

    // Details open the stack trace behind the card.
    click(cx, window, &format!("{BROKEN}-details"));
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("{BROKEN}-details")));
}

#[gpui_kit::test]
fn manage_plugins_opens_the_installed_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_frame);
    cx.run_until_parked();
    click(cx, window, "plugins-manage");
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    assert!(shows(cx, window, "settings-group-installed"));
    assert!(shows(cx, window, "plugin-install-open"));
}

/// The bounds `kg ui` reports for an element: x, y, width and height, in points.
fn bounds_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<[f32; 4]> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into())).map(|element| {
            let bounds = element.bounds;
            [
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            ]
        })
    })
    .unwrap()
}

/// Whether `kg ui` reports an element as disabled.
pub(crate) fn disabled_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.disabled)
    })
    .unwrap()
}

/// Whether `inner` lies inside `outer`, edge to edge.
fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
    inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[0] + inner[2] <= outer[0] + outer[2] + 0.5
        && inner[1] + inner[3] <= outer[1] + outer[3] + 0.5
}

#[gpui_kit::test]
fn a_page_button_sits_inside_its_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_page);
    cx.run_until_parked();
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(SurfaceOp::Open {
                plugin: PluginId(PAGE.into()),
                id: "page".into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| shows(cx, window, "page-button"));

    let frame = bounds_of(cx, window, "surface-page").expect("the page is drawn");
    let button = bounds_of(cx, window, "page-button").expect("the button is drawn");
    assert!(button[2] > 0. && button[3] > 0.);
    assert!(
        inside(button, frame),
        "button {button:?} is outside the page {frame:?}"
    );
    assert!(!shows(cx, window, PAGE_FRAME));
}

#[gpui_kit::test]
fn every_recovery_control_stays_inside_the_narrowest_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    set_preference(cx, "main.left-width", json!(240.));
    cx.simulate_window_resize(window, size(px(1200.), px(800.)));
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-details")));
    click(cx, window, &format!("{BROKEN}-details"));
    cx.run_until_parked();

    let frame = bounds_of(cx, window, "sidebar-pane").expect("the sidebar is drawn");
    for action in ["reload", "logs", "disable", "details"] {
        let button = bounds_of(cx, window, &format!("{BROKEN}-{action}"))
            .unwrap_or_else(|| panic!("{action} is drawn"));
        assert!(
            inside(button, frame),
            "{action} {button:?} is outside {frame:?}"
        );
    }
}

#[gpui_kit::test]
fn the_error_cards_logs_button_shows_that_plugins_log(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-logs")));
    // The watchdog writes the render failure to the log within a second.
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.test.broken").contains("boom in render")
    });

    click(cx, window, &format!("{BROKEN}-logs"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-group-installed"));
    wait_until(cx, |cx| {
        value_of(cx, window, "plugin-log-dev.test.broken")
            .is_some_and(|text| text.contains("boom in render"))
    });
}

fn approvals_waiting(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).approvals().count())
}

fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    cx.foreground_executor().block_test(task)
}

#[gpui_kit::test]
fn space_answers_an_approval_as_a_person_and_synthetic_space_is_refused(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    }];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let name = cx.update(|cx| {
        let id = Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id
            .0
            .clone();
        format!("answer-{id}-yes")
    });

    // kg ui input is not a person: Space on the focused answer is refused.
    let mut refused = false;
    for _ in 0..40 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if !(shows(cx, window, &name) && focused_of_name(cx, window, &name)) {
            continue;
        }
        let keyed = ui(
            cx,
            UiOp::Key {
                keystroke: "space".into(),
                window: 0,
            },
        );
        refused = matches!(keyed, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY);
        break;
    }
    assert!(refused, "synthetic Space reached the approval");
    assert_eq!(approvals_waiting(cx), 1);

    // A person's Space on the focused answer answers it.
    cx.simulate_keystrokes(window, "space");
    cx.run_until_parked();
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
    assert_eq!(approvals_waiting(cx), 0);
}

fn focused_of_name(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.focused)
            == Some(true)
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_install_form_is_disabled_while_an_install_runs_and_follows_the_host(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |_| {});
    cx.run_until_parked();
    let folder = data.path().join("waiting");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(
        folder.join("kagan.json"),
        json!({
            "id": "dev.test.waiting", "name": "Waiting", "version": "1.0.0",
            "kagan": ">=0.1.0", "entry": "main.js", "instructions": "Waits.", "powers": {"clipboard": {"write": true}}
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(folder.join("main.js"), "export default class Plugin {}\n").unwrap();
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-install-open");
    cx.run_until_parked();

    let install = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source: folder.display().to_string(),
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    assert!(!shows(cx, window, "plugin-install"));
    assert!(!shows(cx, window, "plugin-source"));
    click(cx, window, "setting-row-install-waiting");
    assert!(shows(cx, window, "setting-row-install-waiting"));

    // The window learns the install ended from the host alone: no other click is needed.
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval")
            .id
            .clone()
    });
    let declined = cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx));
    cx.foreground_executor().block_test(declined).unwrap();
    cx.foreground_executor().block_test(install).unwrap_err();
    cx.run_until_parked();
    assert_eq!(disabled_of(cx, window, "plugin-install"), Some(false));
    assert_eq!(disabled_of(cx, window, "plugin-source"), Some(false));
}

#[gpui_kit::test]
fn a_focused_plugin_row_answers_its_keys_through_its_actions(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    // The watchdog writes the render failure to the log within a second.
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.test.broken").contains("boom in render")
    });
    click(cx, window, "settings-gear");

    let row = "plugin-row-dev.test.broken";
    let mut focused = false;
    for _ in 0..100 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of_name(cx, window, row) {
            focused = true;
            break;
        }
    }
    assert!(focused, "the plugin row takes focus");

    // Enter is TogglePluginDetails for the row; L is ShowPluginLogs.
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(
        present(cx, window, "plugin-logs-dev.test.broken"),
        "details opened"
    );
    cx.simulate_keystrokes(window, "l");
    cx.run_until_parked();
    wait_until(cx, |cx| {
        value_of(cx, window, "plugin-log-dev.test.broken")
            .is_some_and(|text| text.contains("boom in render"))
    });
}

fn set_preference(cx: &mut TestAppContext, key: &str, value: serde_json::Value) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(kg_domain::PreferenceOp::Set {
                key: key.into(),
                value,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_panel_saved_as_collapsed_by_an_older_kagan_still_shows_its_body(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_setting("panel.needs-you.collapsed", &json!(true))
            .unwrap();
    });
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    show_questions(cx, window);
    wait_until(cx, |cx| shows(cx, window, "needs-you-body"));
    assert!(!shows(cx, window, "panel-toggle-needs-you"));
}

const NAVIGATOR_PANEL: &str = "panel-dev.kagan.navigator-notes";
#[gpui_kit::test]
fn a_panel_a_plugin_opens_stays_open_when_a_preference_changes(cx: &mut TestAppContext) {
    use crate::tests::kagan::{install_navigator, run_navigator};
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    run_navigator(cx, "panel");
    wait_until(cx, |cx| shows(cx, window, "plugin-tool-body"));
    set_preference(cx, "main.left-width", json!(300.));
    cx.run_until_parked();
    assert!(shows(cx, window, "plugin-tool-body"));
    assert!(!shows(cx, window, NAVIGATOR_PANEL));
}

fn plugin_enabled(cx: &mut TestAppContext, id: &str) -> bool {
    cx.update(|cx| {
        PluginHost::try_global(cx)
            .expect("plugin host")
            .read(cx)
            .status()
            .iter()
            .find(|status| status.id.0 == id)
            .expect("plugin listed")
            .enabled
    })
}

/// Opens the broken panel and moves focus from the card by `steps` Tabs: 0 is the card, 1 is
/// Reload, 2 is Logs, 3 is Disable, 4 is Details.
fn focus_past_the_card(cx: &mut TestAppContext, window: AnyWindowHandle, steps: usize) {
    let card = format!("{BROKEN}-failed");
    let mut focused = false;
    for _ in 0..40 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of_name(cx, window, &card) {
            focused = true;
            break;
        }
    }
    assert!(focused, "the card takes focus");
    for _ in 0..steps {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    if steps > 0 {
        assert!(!focused_of_name(cx, window, &card), "a button has focus");
    }
}

#[gpui_kit::test]
fn space_on_the_card_is_the_row_key_that_disables_the_plugin(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-failed")));

    focus_past_the_card(cx, window, 0);
    press_key(cx, window, "space");
    wait_until(cx, |cx| !plugin_enabled(cx, "dev.test.broken"));
}

#[gpui_kit::test]
fn enter_and_space_on_a_tab_select_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = crate::tests::kagan::run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    show_questions(cx, window);
    wait_until(cx, |cx| shows(cx, window, "needs-you-body"));
    assert!(shows(cx, window, "changes-header"));
    // Leave the terminal; Tab stays inside it otherwise.
    click(cx, window, "needs-you-header");
    assert_eq!(focused_of(cx, window, "needs-you-header"), Some(true));

    for (header, key, body) in [
        ("changes-header", "enter", "changes-body"),
        ("needs-you-header", "space", "needs-you-body"),
    ] {
        let mut focused = false;
        for _ in 0..40 {
            if focused_of_name(cx, window, header) {
                focused = true;
                break;
            }
            cx.simulate_keystrokes(window, "tab");
            cx.run_until_parked();
        }
        assert!(focused, "{header} takes focus");
        press_key(cx, window, key);
        assert!(shows(cx, window, body), "{key} selects {header}");
    }
    assert!(!shows(cx, window, "changes-body"));
}
#[gpui_kit::test]
fn enter_on_reload_reloads_the_plugin(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-failed")));
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.test.broken").contains("boom in render")
    });

    focus_past_the_card(cx, window, 1);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.test.broken").contains("reloaded")
    });
    assert!(plugin_enabled(cx, "dev.test.broken"));
}

#[gpui_kit::test]
fn enter_on_logs_shows_the_plugins_log(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-failed")));
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.test.broken").contains("boom in render")
    });

    focus_past_the_card(cx, window, 2);
    press_key(cx, window, "enter");
    assert!(
        shows(cx, window, "settings-group-installed"),
        "Enter did not open the installed page"
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "plugin-log-dev.test.broken")
            .is_some_and(|text| text.contains("boom in render"))
    });
    assert!(plugin_enabled(cx, "dev.test.broken"));
}

#[gpui_kit::test]
fn enter_and_space_on_details_show_and_hide_the_error(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-failed")));

    let error = format!("{BROKEN}-error");
    focus_past_the_card(cx, window, 4);
    assert!(!shows(cx, window, &error));
    press_key(cx, window, "enter");
    wait_until(cx, |cx| {
        value_of(cx, window, &error).is_some_and(|text| text.contains("boom in render"))
    });
    press_key(cx, window, "space");
    assert!(!shows(cx, window, &error), "Space did not hide the error");
    assert!(plugin_enabled(cx, "dev.test.broken"));
}

#[gpui_kit::test]
fn enter_on_disable_turns_the_plugin_off(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    wait_until(cx, |cx| shows(cx, window, &format!("{BROKEN}-failed")));

    focus_past_the_card(cx, window, 3);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| !plugin_enabled(cx, "dev.test.broken"));
}

#[gpui_kit::test]
fn x_on_a_plugin_row_asks_before_removing_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    cx.run_until_parked();
    open_tool(cx, "dev.test.broken");
    click(cx, window, "settings-gear");

    let row = "plugin-row-dev.test.broken";
    let mut focused = false;
    for _ in 0..100 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of_name(cx, window, row) {
            focused = true;
            break;
        }
    }
    assert!(focused, "the plugin row takes focus");
    assert_eq!(approvals_waiting(cx), 0);

    cx.simulate_keystrokes(window, "x");
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    assert!(plugin_enabled(cx, "dev.test.broken"));
}
