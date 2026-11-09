use std::path::Path;
use std::rc::Rc;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{Op, OpResult, PluginId, PluginOp, ThreadOp};
use serde_json::{Value, json};

use super::{answer_standalone, installed_at, open, plugin_op, thread, write_plugin};

const STATUS: &str = "dev.test.status";
const UNDECLARED: &str = "dev.test.undeclared";

const SCRIPT: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import { setStatus } from "kagan/host";

export default class Plugin extends View {
  init(_props, cx) {
    __INIT__
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind !== "tool" || job.name !== "status") { reply(job.id, null); continue; }
        const input = job.input || {};
        try {
          if (input.op === "hide") { setStatus(null); reply(job.id, { text: "ok" }); }
          else if (input.op === "same") { setStatus("42%", { tone: "busy" }); reply(job.id, { text: "ok" }); }
          else if (input.op === "value") {
            try { setStatus(input.text, input.options); reply(job.id, { text: "ok" }); }
            catch (error) { reply(job.id, { text: String(error) }); }
          } else if (input.op === "refusals") {
            const cases = [
              ["empty", () => setStatus("", {})],
              ["space", () => setStatus("   ", {})],
              ["long", () => setStatus("x".repeat(121), {})],
              ["control", () => setStatus("a\nb", {})],
              ["bidi", () => setStatus("a\u202Eb", {})],
              ["tone", () => setStatus("ok", { tone: "loud" })],
              ["option", () => setStatus("ok", { top: "x" })],
              ["missing", () => setStatus()],
            ];
            const errors = {};
            for (const [name, run] of cases) {
              try { run(); errors[name] = "accepted"; } catch (error) { errors[name] = String(error); }
            }
            reply(job.id, { text: JSON.stringify(errors) });
          } else { reply(job.id, { text: "unknown" }); }
        } catch (error) { reply(job.id, { text: String(error) }); }
      }
    });
  }
}
"#;

fn status_plugin(root: &Path, id: &str, title: Option<&str>, initial: &str) {
    let mut manifest = json!({
        "id": id,
        "name": "Status",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Show a line in the activity strip.",
        "powers": {},
        "tools": [{
            "name": "status",
            "description": "Change the status line",
            "input": "schemas/any.json",
            "scope": "all"
        }]
    });
    if let Some(title) = title {
        manifest["ui"] = json!({ "status": { "title": title } });
    }
    write_plugin(root, &manifest);
    std::fs::write(root.join("main.js"), SCRIPT.replace("__INIT__", initial)).unwrap();
}

fn items(cx: &mut TestAppContext) -> Vec<crate::StatusItem> {
    cx.read(|cx| {
        crate::PluginStrip::global(cx)
            .map(|strip| strip.read(cx).items().cloned().collect())
            .unwrap_or_default()
    })
}

fn call(cx: &mut TestAppContext, input: Value) -> String {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "status".into(),
                input,
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::ToolReply { text, .. } => text,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
fn a_declared_status_line_appears_after_boot_without_a_panel(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);

    let [item] = items(cx).try_into().unwrap();
    assert_eq!(item.id.0, STATUS);
    assert_eq!(item.title, "Usage");
    assert_eq!(item.text, "42%");
    assert_eq!(item.tone, crate::StatusTone::Busy);
}

#[gpui_kit::test]
fn set_status_null_clears_the_item(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    assert_eq!(items(cx).len(), 1);

    assert_eq!(call(cx, json!({ "op": "hide" })), "ok");
    assert!(items(cx).is_empty());
}

#[gpui_kit::test]
fn unloading_disabling_and_removing_clear_the_item(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    let id = PluginId(STATUS.into());

    cx.update(|cx| {
        crate::PluginHost::global(cx).update(cx, |host, cx| host.unload(&id, cx));
    });
    assert!(items(cx).is_empty(), "unload left the line");

    let disabled = plugin_op(cx, PluginOp::Disable { id: id.clone() });
    cx.foreground_executor().block_test(disabled).unwrap();
    assert!(items(cx).is_empty());

    let enabled = plugin_op(cx, PluginOp::Enable { id: id.clone() });
    cx.foreground_executor().block_test(enabled).unwrap();
    assert_eq!(items(cx).len(), 1, "enable did not start the status line");

    let removed = plugin_op(
        cx,
        PluginOp::Remove {
            id,
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removed).unwrap();
    assert!(items(cx).is_empty(), "remove left the line");
}

#[gpui_kit::test]
fn a_reload_replaces_the_item(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    assert_eq!(items(cx)[0].text, "42%");

    std::fs::write(
        plugin.path().join("main.js"),
        SCRIPT.replace("__INIT__", r#"setStatus("7%", { tone: "needs" });"#),
    )
    .unwrap();
    let reloaded = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(STATUS.into()),
            thread_id: None,
        },
    );
    cx.foreground_executor().block_test(reloaded).unwrap();

    let [item] = items(cx).try_into().unwrap();
    assert_eq!(item.text, "7%");
    assert_eq!(item.tone, crate::StatusTone::Needs);
}

#[gpui_kit::test]
fn a_plugin_without_the_manifest_key_is_refused_by_name(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(plugin.path(), UNDECLARED, None, "");
    open(cx, &[installed_at(plugin.path())]);

    let error = call(cx, json!({ "op": "value", "text": "x" }));
    assert!(
        error.contains("plugin dev.test.undeclared has no status item; declare ui.status"),
        "{error}"
    );
    assert!(items(cx).is_empty());
}

#[gpui_kit::test]
fn a_refused_value_names_its_limit(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);

    let errors: std::collections::BTreeMap<String, String> =
        serde_json::from_str(&call(cx, json!({ "op": "refusals" }))).unwrap();
    assert!(
        errors["empty"].contains("use null"),
        "{:?}",
        errors["empty"]
    );
    assert!(
        errors["space"].contains("use null"),
        "{:?}",
        errors["space"]
    );
    assert!(errors["long"].contains("120"), "{:?}", errors["long"]);
    assert!(
        errors["control"].contains("plain line"),
        "{:?}",
        errors["control"]
    );
    assert!(
        errors["bidi"].contains("plain line"),
        "{:?}",
        errors["bidi"]
    );
    assert!(
        errors["tone"].contains("quiet, busy, needs or failed"),
        "{:?}",
        errors["tone"]
    );
    assert!(
        errors["option"].contains("only tone"),
        "{:?}",
        errors["option"]
    );
    assert!(
        errors["missing"].contains("argument 1 is missing"),
        "{:?}",
        errors["missing"]
    );
}

#[gpui_kit::test]
fn the_same_value_does_not_notify(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);

    let strip = cx.read(|cx| crate::PluginStrip::global(cx).unwrap());
    let seen = Rc::new(std::cell::Cell::new(0));
    let changes = seen.clone();
    let _subscription =
        cx.update(|cx| cx.observe(&strip, move |_, _| changes.set(changes.get() + 1)));
    assert_eq!(call(cx, json!({ "op": "same" })), "ok");
    cx.run_until_parked();
    assert_eq!(seen.get(), 0, "the same status notified");
}

#[gpui_kit::test]
fn a_load_that_fails_after_set_status_leaves_no_stale_line(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("stale"); throw new Error("boom");"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    assert!(items(cx).is_empty(), "a failed load kept its line");
}

#[gpui_kit::test]
fn a_crashed_load_clears_the_line(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    assert_eq!(items(cx).len(), 1);

    let id = PluginId(STATUS.into());
    cx.update(|cx| {
        crate::PluginHost::global(cx).update(cx, |host, cx| {
            host.active
                .get_mut(&id)
                .and_then(|active| active.loaded.as_mut())
                .expect("the status plugin loaded")
                .watchdog
                .error = Some("crashed".into());
            host.sample(cx);
        });
    });
    assert!(items(cx).is_empty(), "a crashed load kept its line");
}

#[gpui_kit::test]
fn the_secret_redaction_runs_before_the_line_is_shown(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(
        plugin.path(),
        STATUS,
        Some("Usage"),
        r#"setStatus("42%", { tone: "busy" });"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    let id = PluginId(STATUS.into());
    let secrets = cx.read(|cx| crate::PluginHost::global(cx).read(cx).secrets.clone());
    secrets.set(&id, "token", "hunter2").unwrap();

    assert_eq!(call(cx, json!({ "op": "value", "text": "hunter2" })), "ok");
    assert_eq!(items(cx)[0].text, "[secret]");
}

#[gpui_kit::test]
fn five_plugins_keep_their_id_order(cx: &mut TestAppContext) {
    let plugins: Vec<tempfile::TempDir> = (1..=5)
        .map(|number| {
            let plugin = tempfile::tempdir().unwrap();
            status_plugin(
                plugin.path(),
                &format!("dev.test.status{number}"),
                Some("Usage"),
                r#"setStatus("42%", { tone: "busy" });"#,
            );
            plugin
        })
        .collect();
    let installed: Vec<_> = plugins
        .iter()
        .map(|plugin| installed_at(plugin.path()))
        .collect();
    open(cx, &installed);

    let ids: Vec<String> = items(cx).into_iter().map(|item| item.id.0).collect();
    assert_eq!(
        ids,
        [
            "dev.test.status1",
            "dev.test.status2",
            "dev.test.status3",
            "dev.test.status4",
            "dev.test.status5",
        ]
    );
}

#[gpui_kit::test]
fn a_declared_item_that_is_never_set_draws_nothing(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(plugin.path(), STATUS, Some("Usage"), "");
    open(cx, &[installed_at(plugin.path())]);
    assert!(items(cx).is_empty());
}

#[test]
fn every_tone_name_parses() {
    use crate::StatusTone::{Busy, Failed, Needs, Quiet};
    let parse = crate::strip::StatusTone::parse;
    assert_eq!(parse("quiet"), Some(Quiet));
    assert_eq!(parse("busy"), Some(Busy));
    assert_eq!(parse("needs"), Some(Needs));
    assert_eq!(parse("failed"), Some(Failed));
    assert_eq!(parse("loud"), None);
}

#[gpui_kit::test]
fn a_tooltip_reaches_the_item_and_a_bad_one_is_refused(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(plugin.path(), STATUS, Some("Usage"), "");
    open(cx, &[installed_at(plugin.path())]);

    call(
        cx,
        json!({ "op": "value", "text": "42%", "options": { "tone": "busy", "tooltip": "Tokens used" } }),
    );
    assert_eq!(items(cx)[0].tooltip.as_deref(), Some("Tokens used"));

    let refused = call(
        cx,
        json!({ "op": "value", "text": "42%", "options": { "tooltip": "a\nb" } }),
    );
    assert!(refused.contains("plain line"), "{refused}");
    assert_eq!(items(cx)[0].tooltip.as_deref(), Some("Tokens used"));
}

#[gpui_kit::test]
fn a_declared_status_command_reaches_the_item(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    status_plugin(plugin.path(), STATUS, None, "");
    write_plugin(
        plugin.path(),
        &json!({
            "id": STATUS,
            "name": "Status",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "Show a line in the activity strip.",
            "powers": {},
            "commands": [{ "id": "usage.refresh", "title": "Refresh usage" }],
            "ui": { "status": { "title": "Usage", "command": "usage.refresh" } },
            "tools": [{
                "name": "status",
                "description": "Change the status line",
                "input": "schemas/any.json",
                "scope": "all"
            }]
        }),
    );
    std::fs::write(
        plugin.path().join("main.js"),
        SCRIPT.replace("__INIT__", ""),
    )
    .unwrap();
    open(cx, &[installed_at(plugin.path())]);

    call(cx, json!({ "op": "value", "text": "42%", "options": {} }));
    assert_eq!(items(cx)[0].command.as_deref(), Some("usage.refresh"));
}
