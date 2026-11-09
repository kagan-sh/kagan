use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpResult, PluginId, PluginOp, ThreadOp};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::manifest::{self, Grant};

use super::{call as ask_tool, installed_at, open, read_manifest, run, thread};

const CALLER: &str = "dev.test.caller";
const CALLEE: &str = "dev.test.callee";

const CALLEE_MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.name === "lookup") reply(job.id, { word: job.input.word, caller: job.caller });
        if (job.name === "broken") fail(job.id, "no luck");
        if (job.name === "size") reply(job.id, job.input.word.length);
        if (job.name === "echo_size") reply(job.id, "y".repeat(job.input.n));
      }
    });
  }
  render() { return "callee"; }
}
"#;

const SECRET_CALLEE_MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
import { settings } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        const token = settings().token;
        if (job.name === "token") reply(job.id, { token });
        if (job.name === "leak") fail(job.id, `401 for ${token}`);
      }
    });
  }
  render() { return "callee"; }
}
"#;

const CALLER_MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import { call } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        ask(job).then(
          text => reply(job.id, { text, isError: false }),
          error => reply(job.id, { text: String(error), isError: true }),
        );
      }
    });
  }
  render() { return "caller"; }
}
async function ask(job) {
  return JSON.stringify(await call(job.input.plugin, job.input.name, job.input.input));
}
"#;

const BOOT_CALLER_MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import { call } from "kagan/host";
const ready = call("dev.test.callee", "lookup", { word: "boot" }).then(
  value => JSON.stringify(value),
  error => String(error),
);
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        reply(job.id, { text: await ready, isError: false });
      }
    });
  }
  render() { return "caller"; }
}
"#;

fn job_plugin(id: &str, extra: Value, script: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut manifest = json!({
        "id": id,
        "name": id,
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
    });
    if let Value::Object(fields) = extra {
        manifest.as_object_mut().unwrap().extend(fields);
    }
    std::fs::create_dir_all(dir.path().join("schemas")).unwrap();
    std::fs::write(
        dir.path().join("schemas/any.json"),
        json!({ "type": "object" }).to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join(manifest::FILE), manifest.to_string()).unwrap();
    std::fs::write(dir.path().join("main.js"), script).unwrap();
    dir
}

fn asking(target: &str) -> Value {
    json!({
        "tools": [{ "name": "ask", "description": "Ask", "input": "schemas/any.json", "scope": "all" }],
        "powers": { "calls": [target] }
    })
}

fn exporting(names: &[&str]) -> Value {
    json!({ "exports": names })
}

fn ask_result(cx: &mut TestAppContext, input: Value) -> (String, bool) {
    match ask_tool(cx, "ask", input).unwrap() {
        OpResult::ToolReply { text, is_error } => (text, is_error),
        other => panic!("unexpected {other:?}"),
    }
}

#[gpui_kit::test]
fn a_plugin_gets_the_reply_of_an_export_it_may_call(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["lookup"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "lookup", "input": { "word": "hi" } }),
    );
    assert!(!is_error, "{text}");
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({ "word": "hi", "caller": CALLER })
    );
}

#[gpui_kit::test]
fn a_call_to_a_name_the_callee_does_not_export_is_refused(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["lookup"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "missing", "input": {} }),
    );
    assert!(is_error);
    assert!(text.contains("does not export missing"), "{text}");
}

#[gpui_kit::test]
fn a_call_to_a_plugin_the_caller_never_asked_for_is_refused(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["lookup"]), CALLEE_MAIN);
    let asker = job_plugin(
        CALLER,
        json!({ "tools": asking(CALLEE)["tools"].clone() }),
        CALLER_MAIN,
    );
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "lookup", "input": {} }),
    );
    assert!(is_error);
    assert!(
        text.contains(&format!("{CALLER} is not approved to call {CALLEE}")),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_caller_whose_approval_left_out_the_call_does_not_start(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["lookup"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    let mut record = installed_at(asker.path());
    record.enabled = false;
    let mut approved: Grant = serde_json::from_value(record.approved.clone()).unwrap();
    assert!(
        approved.powers.calls.contains(&PluginId(CALLEE.into())),
        "the fixture asks for the call, so only approval can refuse it"
    );
    approved.powers.calls.clear();
    record.approved = serde_json::to_value(&approved).unwrap();
    open(cx, &[record, installed_at(target.path())]);
    let error = run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(CALLER.into()),
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains(&format!(
            "needs approval for: asks the {CALLEE} plugin to do work for it"
        )),
        "{error}"
    );
}

#[test]
fn a_call_shows_on_the_install_screen_and_needs_approval() {
    let plugin = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    let manifest = read_manifest(plugin.path()).unwrap();
    let asked = Grant::of(&manifest, plugin.path());
    assert!(
        asked
            .lines()
            .contains(&format!("asks the {CALLEE} plugin to do work for it")),
        "{:?}",
        asked.lines()
    );
    assert!(!asked.within(&Grant::default()));
    assert!(asked.intersect(&Grant::default()).powers.calls.is_empty());
}

#[gpui_kit::test]
fn a_call_to_a_disabled_plugin_is_refused(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["lookup"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    let mut disabled = installed_at(target.path());
    disabled.enabled = false;
    open(cx, &[installed_at(asker.path()), disabled]);
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "lookup", "input": {} }),
    );
    assert!(is_error);
    assert!(
        text.contains(&format!("plugin {CALLEE} is not enabled")),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_failed_export_rejects_the_call_with_its_message(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["broken"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "broken", "input": {} }),
    );
    assert!(is_error);
    assert!(text.contains("no luck"), "{text}");
}

#[gpui_kit::test]
fn a_call_that_is_never_answered_times_out(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["hang"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let pending = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "ask".into(),
                input: json!({ "plugin": CALLEE, "name": "hang", "input": {} }),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(crate::ask::CALL_TIMEOUT + Duration::from_secs(1));
    cx.run_until_parked();
    match cx.foreground_executor().block_test(pending).unwrap() {
        OpResult::ToolReply { text, is_error } => {
            assert!(is_error);
            assert!(text.contains("did not answer hang within 30 s"), "{text}");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[gpui_kit::test]
fn a_call_made_while_the_caller_loads_fails_the_load_without_a_panic(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["lookup"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), BOOT_CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let error = ask_tool(cx, "ask", json!({})).unwrap_err();
    assert!(error.to_string().contains("failed to load"), "{error}");
    assert!(
        error.to_string().contains("no kagan app context"),
        "{error}"
    );
}

#[test]
fn a_plugin_cannot_list_itself_in_calls() {
    let plugin = job_plugin(CALLER, asking(CALLER), CALLER_MAIN);
    let error = read_manifest(plugin.path()).unwrap_err();
    assert!(error.to_string().contains("cannot call itself"), "{error}");
}

#[test]
fn exports_are_unique_nonempty_names() {
    for names in [&["lookup", "lookup"][..], &[""][..]] {
        let plugin = job_plugin(CALLEE, exporting(names), CALLEE_MAIN);
        let error = read_manifest(plugin.path()).unwrap_err().to_string();
        assert!(
            error.contains("exports must be unique, nonempty names"),
            "{error}"
        );
    }
}

#[gpui_kit::test]
fn a_call_reply_hides_the_callee_secrets(cx: &mut TestAppContext) {
    const SECRET: &str = "lin_api_7f3c9d1e";
    let target = job_plugin(
        CALLEE,
        json!({
            "exports": ["token", "leak"],
            "settings": { "token": { "type": "secret", "title": "Token" } }
        }),
        SECRET_CALLEE_MAIN,
    );
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    run(
        cx,
        Op::Plugin(PluginOp::Config {
            id: PluginId(CALLEE.into()),
            op: ConfigOp::Set {
                key: "token".into(),
                value: Some(json!(SECRET)),
            },
            thread_id: None,
        }),
    )
    .unwrap();
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "token", "input": {} }),
    );
    assert!(!is_error, "{text}");
    assert_eq!(text, json!({ "token": "[secret]" }).to_string());
    assert!(!text.contains(SECRET), "{text}");
    let (text, is_error) = ask_result(cx, json!({ "plugin": CALLEE, "name": "leak", "input": {} }));
    assert!(is_error);
    assert!(text.contains("401 for [secret]"), "{text}");
    assert!(!text.contains(SECRET), "{text}");
}

#[gpui_kit::test]
fn a_call_input_is_capped_at_64_kib(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["size"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    // `{"word":""}` is 11 bytes, so this input is exactly the cap.
    let at = "x".repeat(crate::call::CALL_CAP - 11);
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "size", "input": { "word": at } }),
    );
    assert!(!is_error, "{text}");
    assert_eq!(text, at.len().to_string());
    let over = "x".repeat(crate::call::CALL_CAP - 10);
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "size", "input": { "word": over } }),
    );
    assert!(is_error);
    assert!(
        text.contains(&format!("the input to {CALLEE} is larger than 64 KiB")),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_call_reply_is_capped_at_64_kib(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["echo_size"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    // A JSON string spends two bytes on its quotes, so this reply is exactly the cap.
    let at = crate::call::CALL_CAP - 2;
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "echo_size", "input": { "n": at } }),
    );
    assert!(!is_error, "{text}");
    assert_eq!(text.len(), at + 2);
    let over = crate::call::CALL_CAP - 1;
    let (text, is_error) = ask_result(
        cx,
        json!({ "plugin": CALLEE, "name": "echo_size", "input": { "n": over } }),
    );
    assert!(is_error);
    assert!(
        text.contains(&format!("the reply from {CALLEE} is larger than 64 KiB")),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_call_to_a_plugin_unloaded_before_it_answers_ends_unloaded(cx: &mut TestAppContext) {
    let target = job_plugin(CALLEE, exporting(&["hang"]), CALLEE_MAIN);
    let asker = job_plugin(CALLER, asking(CALLEE), CALLER_MAIN);
    open(
        cx,
        &[installed_at(asker.path()), installed_at(target.path())],
    );
    let pending = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "ask".into(),
                input: json!({ "plugin": CALLEE, "name": "hang", "input": {} }),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(CALLEE.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
    match cx.foreground_executor().block_test(pending).unwrap() {
        OpResult::ToolReply { text, is_error } => {
            assert!(is_error);
            assert!(
                text.contains(&format!(
                    "plugin {CALLEE} was unloaded before it answered hang"
                )),
                "{text}"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}
