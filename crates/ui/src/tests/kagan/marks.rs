use super::chord;
use kg_core::Workspace;
use std::path::Path;
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, TestAppContext};
use kg_domain::{InteractionOption, Op, OpResult, Plugin, PluginId, PluginOp, ThreadOp};

use super::{
    click, label, open, pending, run_op, run_program, run_shell, shows, value_of, wait_until,
};

#[cfg(unix)]
#[gpui_kit::test]
fn a_thread_marks_busy_then_done_then_idle_once_read(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (_kagan, window, _data) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    let program = folder.path().join("agent");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf '\\033]2;\\342\\227\\220 work\\007'; sleep 1; printf '\\033]2;\\342\\234\\263 work\\007'; sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let thread = run_program(cx, &program.display().to_string());
    let other = run_shell(cx);
    let mark = format!("thread-mark-{}", thread.0);
    click(cx, window, &format!("thread-{}", thread.0));
    wait_for_mark(cx, window, &mark, "busy");
    click(cx, window, &format!("thread-{}", other.0));
    wait_for_mark(cx, window, &mark, "done");
    let asking = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: thread.clone(),
                title: "Keep going?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
            }),
            cx,
        )
    });
    wait_for_mark(cx, window, &mark, "needs you");
    click(cx, window, &format!("thread-{}", thread.0));
    wait_for_mark(cx, window, &mark, "needs you");
    let question = pending(cx, &thread).into_iter().next().expect("question");
    click(cx, window, &format!("answer-{}-yes", question.id.0));
    cx.foreground_executor().block_test(asking).unwrap();
    wait_for_mark(cx, window, &mark, "idle");
}

#[gpui_kit::test]
fn a_pending_question_stays_needs_you_after_the_thread_is_opened(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    let other = run_shell(cx);
    let row = format!("thread-{}", id.0);
    let mark = format!("thread-mark-{}", id.0);
    wait_until(cx, |cx| shows(cx, window, &row));
    let asking = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Keep going?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
            }),
            cx,
        )
    });
    wait_for_mark(cx, window, &mark, "needs you");
    click(cx, window, &row);
    click(cx, window, &format!("thread-{}", other.0));
    assert_eq!(label(cx, window, &mark), "needs you");
    assert_eq!(value_of(cx, window, &row).as_deref(), Some("needs you"));
    let question = pending(cx, &id).into_iter().next().expect("question");
    click(cx, window, &row);
    click(cx, window, &format!("answer-{}-yes", question.id.0));
    cx.foreground_executor().block_test(asking).unwrap();
    wait_for_mark(cx, window, &mark, "idle");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_shown_thread_that_goes_idle_is_never_needs_you(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (_kagan, window, _data) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    let program = folder.path().join("agent");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf '\\033]2;\\342\\227\\220 work\\007'; sleep 1; printf '\\033]2;\\342\\234\\263 work\\007'; sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let thread = run_program(cx, &program.display().to_string());
    let mark = format!("thread-mark-{}", thread.0);
    click(cx, window, &format!("thread-{}", thread.0));
    let mut seen = wait_for_mark(cx, window, &mark, "busy");
    seen.extend(wait_for_mark(cx, window, &mark, "idle"));
    assert!(
        !seen.iter().any(|word| word == "needs you"),
        "a shown thread went idle without needing the person: {seen:?}"
    );
}

pub(in crate::tests) fn wait_for_mark(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    word: &str,
) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let text = label(cx, window, name);
        if seen.last() != Some(&text) {
            seen.push(text.clone());
        }
        if text == word {
            return seen;
        }
        assert!(Instant::now() < deadline, "{name} never read {word:?}");
        std::thread::yield_now();
    }
}

/// A plugin with a key command that opens another thread and a panel, and a probe command that
/// asks for a panel that does not exist and logs the answer it gets back.
pub(in crate::tests) const NAVIGATOR_SOURCE: &str = r#"import { View, div } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log, openPanel, openThread } from "kagan/host";
import { list } from "kagan/threads";
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
export class NotesPanel extends View {
  render() { return div().child("Notes"); }
}
async function handle(job) {
  if (job.kind !== "command") throw new Error(`Unknown job ${job.kind}`);
  if (job.command === "show") {
    const other = (await list({ limit: 20 })).find((thread) => thread.id !== job.threadId);
    if (!other) throw new Error("no other thread");
    await openThread(other.id);
    await openPanel("notes", { from: job.threadId });
    log("info", "show ran");
    return null;
  }
  if (job.command === "panel") {
    try {
      await openPanel("notes", {});
      log("info", "panel ran");
    } catch (error) {
      log("info", `panel refused: ${String(error)}`);
    }
    return null;
  }
  if (job.command === "project") {
    log("info", "project ran");
    return null;
  }
  if (job.command === "clash") {
    log("info", "clash ran");
    return null;
  }
  if (job.command === "chord") {
    log("info", "chord ran");
    return null;
  }
  if (job.command === "prefixed") {
    log("info", "prefixed ran");
    return null;
  }
  if (job.command === "probe") {
    try {
      await openPanel("missing", {});
      log("info", "probe opened");
    } catch (error) {
      log("info", `probe refused: ${String(error)}`);
    }
    return null;
  }
  throw new Error(`Unknown command ${job.command}`);
}
"#;

pub(in crate::tests) fn install_navigator(data: &Path) {
    let root = data.join("plugins").join("navigator");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.navigator",
        "name": "Navigator",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Opens threads and panels.",
        "powers": { "areas": ["threads"] },
        "commands": [
            { "id": "show", "title": "Show", "key": "secondary-shift-y", "when": "thread" },
            { "id": "probe", "title": "Probe", "when": "always" },
            { "id": "panel", "title": "Panel", "when": "always" },
            { "id": "project", "title": "Project", "key": "secondary-shift-j", "when": "project" },
            { "id": "clash", "title": "Clash", "key": chord("NewThread"), "when": "always" },
            { "id": "chord", "title": "Chord", "key": format!("{} x", chord("NewThread")), "when": "always" },
            { "id": "prefixed", "title": "Prefixed", "key": "secondary-shift-k z", "when": "always" }
        ],
        "ui": { "panels": [
            { "id": "notes", "title": "Notes", "view": "NotesPanel", "placement": "left" }
        ] }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), NAVIGATOR_SOURCE).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.navigator".into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({
                "powers": { "areas": ["threads"] }, "downloads": {}, "folders": {}
            }),
            error: None,
        })
        .unwrap();
}

/// The plugin's log, as its own output reports it.
pub(in crate::tests) fn plugin_log(cx: &mut TestAppContext, id: &str) -> String {
    let logs = run_op(
        cx,
        Op::Plugin(PluginOp::Logs {
            id: PluginId(id.into()),
            follow: false,
            after: None,
        }),
    );
    match logs {
        OpResult::Logs { text, .. } => text,
        other => panic!("{other:?}"),
    }
}
