use super::chord;
use crate::kagan::screen_read;
use kg_core::Workspace;
use std::path::Path;

use gpui_kit::{TestAppContext, px, size};
use kg_domain::{KeymapOp, Op, OpResult, Plugin, PluginId, PluginOp, ProjectId};

use super::{
    add_project, click, grid_text, install_navigator, open_surface, open_with_plugins_then,
    plugin_log, run_op, run_shell, shows, wait_for_screen, wait_until,
};

#[gpui_kit::test]
fn a_plugin_key_command_opens_a_thread_and_a_panel_and_runs(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    let first = run_shell(cx);
    let _second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(first.clone())
    });
    open_surface(cx, &PluginId(NAVIGATOR.into()), "notes");
    wait_until(cx, |cx| {
        shows(cx, window, "panel-dev.kagan.navigator-notes-body")
    });
    click(cx, window, &format!("thread-{}", first.0));

    // The key is bound in the thread context, so it runs while a thread is on screen.
    cx.simulate_keystrokes(
        window,
        if cfg!(target_os = "macos") {
            "cmd-shift-y"
        } else {
            "ctrl-shift-y"
        },
    );
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx))
            .is_some_and(|shown| shown != first)
    });
    assert!(plugin_log(cx, "dev.kagan.navigator").contains("show ran"));
}

#[gpui_kit::test]
fn a_plugin_command_gets_the_answer_when_its_panel_is_missing(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let host = cx.update(|cx| kg_plugins::PluginHost::global(cx));
    let plugin = PluginId("dev.kagan.navigator".into());
    let task = cx.update(|cx| {
        kg_plugins::PluginHost::dispatch_command(&host, &plugin, "probe", None, None, cx)
    });
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
    assert!(
        plugin_log(cx, "dev.kagan.navigator").contains("probe refused"),
        "the plugin never got an answer"
    );
}

pub(in crate::tests) const PASTER_SOURCE: &str = r#"import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log, pasteIntoThread } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then(
          (value) => reply(job.id, value),
          (error) => fail(job.id, String(error)),
        );
      }
    });
  }
  render() { return ""; }
}
async function handle(job) {
  if (job.kind !== "command") throw new Error(`Unknown job ${job.kind}`);
  if (job.command === "paste") {
    await pasteIntoThread(job.threadId, "pasted-from-plugin");
    log("info", "paste ran");
    return null;
  }
  throw new Error(`Unknown command ${job.command}`);
}
"#;

pub(in crate::tests) fn install_paster(data: &Path) {
    let root = data.join("plugins").join("paster");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.paster",
        "name": "Paster",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Pastes into a thread.",
        "commands": [
            { "id": "paste", "title": "Paste", "when": "thread" }
        ]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), PASTER_SOURCE).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.paster".into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({
                "powers": {}, "downloads": {}, "folders": {}
            }),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

#[gpui_kit::test]
fn paste_into_thread_puts_text_in_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_paster);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(id.clone())
    });
    let host = cx.update(|cx| kg_plugins::PluginHost::global(cx));
    let plugin = PluginId("dev.kagan.paster".into());
    let task = cx.update(|cx| {
        kg_plugins::PluginHost::dispatch_command(
            &host,
            &plugin,
            "paste",
            Some(id.clone()),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains("pasted-from-plugin")
    });
    assert!(plugin_log(cx, "dev.kagan.paster").contains("paste ran"));
}

pub(in crate::tests) const NAVIGATOR: &str = "dev.kagan.navigator";

/// The shortcut for `keys` on this platform: `shift-j` is cmd-shift-j on macOS, ctrl-shift-j elsewhere.
pub(in crate::tests) fn secondary(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{keys}")
    } else {
        format!("ctrl-{keys}")
    }
}

/// Runs one of the navigator's commands the way a call from outside a key does.
pub(in crate::tests) fn run_navigator(cx: &mut TestAppContext, command: &str) {
    let host = cx.update(|cx| kg_plugins::PluginHost::global(cx));
    let plugin = PluginId(NAVIGATOR.into());
    let task = cx.update(|cx| {
        kg_plugins::PluginHost::dispatch_command(&host, &plugin, command, None, None, cx)
    });
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_plugin_key_that_a_window_shortcut_takes_is_not_bound(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    // The plugin's clash key is New Thread's default, so New Thread runs and the plugin is told why.
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    let log = plugin_log(cx, NAVIGATOR);
    assert!(!log.contains("clash ran"), "{log}");
    assert!(
        log.contains(&format!(
            "command clash is not bound to {}",
            kg_core::parse_key(&chord("NewThread"))
                .unwrap()
                .unparse(kg_core::host_os())
        )),
        "{log}"
    );
}

/// Windows reads a run-time key through the keyboard layout, unlike the window's own keys.
#[cfg(windows)]
#[gpui_kit::test]
fn on_windows_a_plugin_key_that_bigger_takes_is_not_bound(cx: &mut TestAppContext) {
    let plugin = "dev.kagan.grow";
    let (_kagan, _window, _data) = open_with_plugins_then(cx, |data| {
        let root = data.join("grow");
        std::fs::create_dir_all(&root).unwrap();
        let manifest = serde_json::json!({
            "id": plugin, "name": "Grow", "version": "1.0.0", "kagan": ">=0.1.0",
            "entry": "main.js", "instructions": "Clashes with Bigger.", "powers": {},
            "commands": [{ "id": "grow", "title": "Grow", "key": chord("Bigger"), "when": "always" }]
        });
        std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
        std::fs::write(
            root.join("main.js"),
            "import { View } from \"gpui-kit\";\nexport default class Plugin extends View { render() { return \"\"; } }\n",
        )
        .unwrap();
        kg_store::Store::open(data.join("kagan.sqlite3"))
            .unwrap()
            .insert_plugin(&Plugin {
                id: PluginId(plugin.into()),
                source: root.display().to_string(),
                root,
                commit: None,
                enabled: true,
                approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
                skills: Vec::new(),
                error: None,
            })
            .unwrap();
    });
    cx.run_until_parked();
    let log = plugin_log(cx, plugin);
    assert!(log.contains("command grow is not bound to"), "{log}");
    assert!(log.contains("takes that key"), "{log}");
}

#[gpui_kit::test]
fn plugin_keys_follow_a_reload_and_a_changed_window_shortcut(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let thread = run_shell(cx);
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(NAVIGATOR.into()),
            thread_id: Some(thread),
        }),
    );
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert!(
        !plugin_log(cx, NAVIGATOR).contains("clash ran"),
        "reload kept the window's key"
    );

    // New Thread moves onto the project key, and frees its old key for the plugin.
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Keymap(KeymapOp::Set {
                action: "NewThread".into(),
                key: secondary("shift-j"),
                reset: false,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &secondary("shift-j"));
    cx.run_until_parked();
    let log = plugin_log(cx, NAVIGATOR);
    assert!(!log.contains("project ran"), "{log}");
    assert!(
        log.contains(&format!(
            "command project is not bound to {}",
            secondary("shift-j")
        )),
        "{log}"
    );
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );

    // The key New Thread gave up is the plugin's now.
    cx.simulate_keystrokes(window, &chord("NewThread"));
    wait_until(cx, |cx| plugin_log(cx, NAVIGATOR).contains("clash ran"));
}

#[gpui_kit::test]
fn a_plugin_panel_opens_from_a_closed_sidebar_without_any_key(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    if cx.update(|cx| kagan.read(cx).left_open()) {
        click(cx, window, "left-sidebar-toggle");
    }
    wait_until(cx, |cx| !cx.update(|cx| kagan.read(cx).left_open()));
    assert!(!shows(cx, window, "panel-dev.kagan.navigator-notes-body"));

    run_navigator(cx, "panel");
    wait_until(cx, |cx| {
        shows(cx, window, "panel-dev.kagan.navigator-notes-body")
    });
    assert!(cx.update(|cx| kagan.read(cx).left_open()));
    assert!(plugin_log(cx, NAVIGATOR).contains("panel ran"));
}

#[gpui_kit::test]
fn a_plugin_panel_leaves_settings_and_is_on_screen_when_its_call_returns(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));

    run_navigator(cx, "panel");
    assert!(
        !shows(cx, window, "settings-page"),
        "settings still covers the panel"
    );
    assert!(shows(cx, window, "panel-dev.kagan.navigator-notes-body"));
    assert!(plugin_log(cx, NAVIGATOR).contains("panel ran"));
}

#[gpui_kit::test]
fn the_project_key_is_off_while_settings_covers_the_window(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let folder = data.path().join("plain");
    std::fs::create_dir_all(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx))) == Some(project.id.clone())
    });
    let runs = |cx: &mut TestAppContext| plugin_log(cx, NAVIGATOR).matches("project ran").count();

    cx.simulate_keystrokes(window, &secondary("shift-j"));
    wait_until(cx, |cx| runs(cx) == 1);

    click(cx, window, "settings-gear");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &secondary("shift-j"));
    cx.run_until_parked();
    assert_eq!(
        runs(cx),
        1,
        "the project key ran while settings covered the window"
    );

    click(cx, window, "settings-close");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &secondary("shift-j"));
    wait_until(cx, |cx| runs(cx) == 2);
}

#[gpui_kit::test]
fn a_plugin_sequence_that_starts_with_a_window_shortcut_is_not_bound(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let log = plugin_log(cx, NAVIGATOR);
    assert!(log.contains("command chord is not bound to"), "{log}");

    // The first key of the refused sequence is New Thread, which runs on its own.
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    assert!(!plugin_log(cx, NAVIGATOR).contains("chord ran"));
}

#[gpui_kit::test]
fn a_saved_shortcut_that_starts_a_plugin_sequence_takes_it_on_change(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    assert!(!plugin_log(cx, NAVIGATOR).contains("command prefixed is not bound"));

    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Keymap(KeymapOp::Set {
                action: "NewThread".into(),
                key: secondary("shift-k"),
                reset: false,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
    assert!(plugin_log(cx, NAVIGATOR).contains("command prefixed is not bound to"));

    cx.simulate_keystrokes(window, &secondary("shift-k"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    assert!(!plugin_log(cx, NAVIGATOR).contains("prefixed ran"));
}

#[gpui_kit::test]
fn a_saved_plugin_key_is_refused_where_the_window_refuses_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let set = |cx: &mut TestAppContext, command: &str, key: &str| {
        run_op(
            cx,
            Op::Keymap(KeymapOp::Set {
                action: format!("{NAVIGATOR}:{command}"),
                key: key.into(),
                reset: false,
            }),
        )
    };
    // A key that starts with New Thread's key is refused.
    assert_eq!(
        set(cx, "panel", &format!("{} x", chord("NewThread"))),
        OpResult::Setting(serde_json::json!({ "ok": false, "conflict": "NewThread" }))
    );
    // A key that overlaps another plugin command is refused until that command's key is removed.
    assert_eq!(
        set(cx, "panel", "secondary-shift-k z"),
        OpResult::Setting(
            serde_json::json!({ "ok": false, "conflict": format!("{NAVIGATOR}:prefixed") })
        )
    );
    run_op(
        cx,
        Op::Keymap(KeymapOp::Remove {
            action: format!("{NAVIGATOR}:prefixed"),
        }),
    );
    assert_eq!(
        set(cx, "panel", "secondary-shift-k"),
        OpResult::Setting(serde_json::json!({ "ok": true }))
    );
    cx.run_until_parked();

    // The window follows the saved binding: the key runs panel, and prefixed lost its key.
    cx.simulate_keystrokes(window, &secondary("shift-k"));
    wait_until(cx, |cx| plugin_log(cx, NAVIGATOR).contains("\"panel ran\""));
    assert!(!plugin_log(cx, NAVIGATOR).contains("prefixed ran"));
}
