use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Appearance, Op, Plugin, PluginId, PluginOp, ThemeOp};
use serde_json::{Value, json};

use super::{aria_label, install_plugins, run_op};
use crate::tests::kagan::{click, open, open_with_plugins_then, present, shows};

const PREFS: &str = "dev.test.prefs";
const VIEW: &str = "dev.test.view";
const OTHER: &str = "dev.test.other";
const OLD: &str = "dev.test.old";

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

pub(in crate::tests) fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

pub(in crate::tests) fn write_git_plugin(dir: &std::path::Path, clipboard: bool) {
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

pub(in crate::tests) fn press(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(window, &ElementId::Name(name.to_owned().into()), cx)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    })
    .unwrap();
}

pub(in crate::tests) fn choose_folder(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    path: &Path,
) {
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

pub(in crate::tests) fn host_setting(cx: &mut TestAppContext, key: &str) -> Value {
    cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .settings(&PluginId(PREFS.into()))
            .into_iter()
            .find(|(name, _, _)| name == key)
            .map_or(Value::Null, |(_, _, value)| value)
    })
}

pub(in crate::tests) fn approval_count(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).approvals().count())
}

pub(in crate::tests) fn until_saved(cx: &mut TestAppContext, key: &str, want: &Value) {
    for _ in 0..200 {
        if &host_setting(cx, key) == want {
            return;
        }
        assert!(cx.background_executor.tick(), "{key} never reached {want}");
    }
    panic!("{key} stayed {}", host_setting(cx, key));
}

pub(in crate::tests) fn canonical(path: &Path) -> String {
    dunce::canonicalize(path).unwrap().display().to_string()
}

pub(in crate::tests) fn approve_waiting(cx: &mut TestAppContext) {
    let id = waiting(cx).expect("an approval is waiting");
    let answer = cx.update(|cx| Workspace::answer_standalone(id, "approve".into(), cx));
    cx.foreground_executor().block_test(answer).unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn waiting(
    cx: &mut TestAppContext,
) -> Option<kg_domain::PendingInteractionId> {
    cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .map(|approval| approval.id.clone())
    })
}

pub(in crate::tests) fn bound_keycap(cx: &mut TestAppContext, action: &str) -> String {
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

pub(in crate::tests) fn set_theme(cx: &mut TestAppContext, theme: &str) {
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

pub(in crate::tests) fn row_top(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(f32::MAX, |element| f32::from(element.bounds.origin.y))
    })
    .unwrap()
}

pub(in crate::tests) fn install_ordered(data: &Path) {
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

pub(in crate::tests) const TIDE: &str = r##"{
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
