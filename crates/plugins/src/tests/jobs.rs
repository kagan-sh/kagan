use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, Plugin, PluginId, PluginOp, ProjectId, ThreadOp};
use serde_json::json;

use crate::PluginHostGlobal;
use crate::ask::{INSTRUCT_TIMEOUT, TOOL_TIMEOUT};
use crate::manifest::Grant;

use super::{
    CLI_PROBE, COMMAND_PROBE, ID, THREAD, answered, ask_instruct, call, cli_plugin, hanging_calls,
    installed, installed_at, instruct_plugin, instruct_plugin_id, loaded, open, plugin_dir,
    probe_plugin, read_manifest, records, run, sample_manifest, thread, tool_names, write_plugin,
};

#[gpui_kit::test]
fn tools_follow_scope_and_thread_metadata(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);

    let (instructions, names) = tool_names(cx);
    let instructions = instructions.unwrap();
    assert!(instructions.starts_with("Plugin "));
    assert!(instructions.contains("Echo things back."));
    assert_eq!(names, ["echo", "broken", "hang", "peek", "skill_load"]);

    run(
        cx,
        Op::Thread(ThreadOp::SetMetadata {
            id: thread(),
            plugin_id: PluginId(ID.into()),
            value: json!({ "master": true }),
        }),
    )
    .unwrap();
    assert_eq!(
        tool_names(cx).1,
        ["echo", "broken", "hang", "secret", "peek", "skill_load"]
    );
    assert!(!loaded(cx), "listing tools runs no plugin code");
}

#[gpui_kit::test]
fn a_tool_name_two_plugins_share_is_a_note_naming_both(cx: &mut TestAppContext) {
    let first = plugin_dir();
    let second = tempfile::tempdir().unwrap();
    let mut other = sample_manifest();
    other["id"] = json!("dev.test.echo.two");
    other["name"] = json!("Echo Two");
    write_plugin(second.path(), &other);
    open(cx, &[installed(first.path()), installed_at(second.path())]);
    let notes = cx.read(|cx| {
        cx.global::<PluginHostGlobal>()
            .0
            .read(cx)
            .status()
            .into_iter()
            .filter_map(|status| status.note.map(|note| (status.id.0, note)))
            .collect::<Vec<_>>()
    });
    let [(id, note)] = notes.as_slice() else {
        panic!("{notes:?}");
    };
    assert_eq!(id, "dev.test.echo.two");
    assert!(note.contains(ID), "{note}");
    assert!(note.contains("is also declared by plugin"), "{note}");
}

#[gpui_kit::test]
fn calls_are_checked_before_the_plugin_loads_then_answered_by_its_jobs(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);

    let error = call(cx, "echo", json!({ "text": 5 })).unwrap_err();
    assert!(
        matches!(&error, OpError::Invalid(message) if message.contains("does not match its schema")),
        "{error}"
    );
    assert!(!loaded(cx));
    assert!(matches!(
        call(cx, "secret", json!({})),
        Err(OpError::NotFound(_))
    ));

    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );
    assert!(loaded(cx));
    assert_eq!(
        call(cx, "broken", json!({})).unwrap(),
        OpResult::ToolReply {
            text: "no luck".into(),
            is_error: true
        }
    );
}

#[gpui_kit::test]
fn unloading_fails_open_jobs_and_makes_tools_unavailable(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);

    let pending = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "hang".into(),
                input: json!({}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    assert!(loaded(cx));

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let error = cx.foreground_executor().block_test(pending).unwrap_err();
    assert!(error.to_string().contains("unloaded"), "{error}");
    assert!(matches!(
        call(cx, "echo", json!({ "text": "hi" })),
        Err(OpError::NotFound(_))
    ));
    assert_eq!(tool_names(cx).1, ["skill_load"]);
    assert!(!records(cx)[0].enabled);

    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    assert_eq!(
        call(cx, "echo", json!({ "text": "back" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} back true"),
            is_error: false
        }
    );
}

#[gpui_kit::test]
fn a_job_without_an_answer_times_out(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);

    let pending = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "hang".into(),
                input: json!({}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(TOOL_TIMEOUT + Duration::from_secs(1));
    cx.run_until_parked();
    let error = cx.foreground_executor().block_test(pending).unwrap_err();
    assert!(error.to_string().contains("did not answer hang"), "{error}");
}

#[gpui_kit::test]
fn an_unloaded_plugin_reports_the_same_reason_to_tools_and_cli(cx: &mut TestAppContext) {
    let plugin = cli_plugin();
    open(cx, &[installed(plugin.path())]);
    let calls = hanging_calls(cx);
    cx.run_until_parked();
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let expected = format!("plugin {ID} was unloaded before it answered hang");
    for call in calls {
        let error = cx.foreground_executor().block_test(call).unwrap_err();
        assert_eq!(error.to_string(), expected);
    }
}

#[gpui_kit::test]
fn a_hung_plugin_reports_the_same_timeout_to_tools_and_cli(cx: &mut TestAppContext) {
    let plugin = cli_plugin();
    open(cx, &[installed(plugin.path())]);
    let calls = hanging_calls(cx);
    cx.run_until_parked();
    cx.executor()
        .advance_clock(TOOL_TIMEOUT + Duration::from_secs(1));
    cx.run_until_parked();
    let expected = format!(
        "plugin {ID} did not answer hang within {} s",
        TOOL_TIMEOUT.as_secs()
    );
    for call in calls {
        let error = cx.foreground_executor().block_test(call).unwrap_err();
        assert_eq!(error.to_string(), expected);
    }
}

const LEAKING_COMMAND: &str = r#"
import { View } from "gpui-kit";
import { next, fail } from "kagan/jobs";
import { settings } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        fail(job.id, `401 for ${settings().token}`);
      }
    });
  }
  render() { return "commands"; }
}
"#;

#[gpui_kit::test]
fn a_failed_command_hides_the_plugin_secrets(cx: &mut TestAppContext) {
    const SECRET: &str = "lin_api_7f3c9d1e";
    let plugin = probe_plugin(&[], LEAKING_COMMAND);
    let path = plugin.path().join(crate::manifest::FILE);
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["settings"] = json!({ "token": { "type": "secret", "title": "Token" } });
    std::fs::write(path, value.to_string()).unwrap();
    open(cx, &[installed(plugin.path())]);
    run(
        cx,
        Op::Plugin(PluginOp::Config {
            id: PluginId(ID.into()),
            op: kg_domain::ConfigOp::Set {
                key: "token".into(),
                value: Some(json!(SECRET)),
            },
            thread_id: None,
        }),
    )
    .unwrap();
    let error = run(
        cx,
        Op::Plugin(PluginOp::Command {
            id: PluginId(ID.into()),
            command: "ping".into(),
            thread_id: Some(thread()),
            project_id: None,
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("401 for [secret]"), "{error}");
    assert!(!error.contains(SECRET), "{error}");
}

#[gpui_kit::test]
fn commands_run_as_jobs_and_need_a_thread_when_they_are_asked_to(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&[], COMMAND_PROBE);
    open(cx, &[installed(plugin.path())]);
    let command = |thread_id| {
        Op::Plugin(PluginOp::Command {
            id: PluginId(ID.into()),
            command: "ping".into(),
            thread_id,
            project_id: None,
        })
    };
    assert!(run(cx, command(None)).is_err());
    assert_eq!(run(cx, command(Some(thread()))).unwrap(), OpResult::Done);
}

#[gpui_kit::test]
fn a_declared_cli_command_runs_as_a_cli_job(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&[], CLI_PROBE);
    let mut value = sample_manifest();
    value["cli"] = json!([{ "name": "probe", "commands": [{ "name": "say" }] }]);
    write_plugin(plugin.path(), &value);
    std::fs::write(plugin.path().join("main.js"), CLI_PROBE).unwrap();
    open(cx, &[installed(plugin.path())]);
    let cli = |name: &str, command: &str| {
        Op::Plugin(PluginOp::Cli {
            name: name.into(),
            command: command.into(),
            argv: vec!["a".into(), "b".into()],
            cwd: "/work".into(),
            thread_id: None,
            project_id: None,
        })
    };
    assert_eq!(
        answered(cx, cli("probe", "say")).unwrap(),
        OpResult::Cli {
            exit_code: 3,
            stdout: "say:a,b".into(),
            stderr: "/work".into()
        }
    );
    assert!(matches!(
        answered(cx, cli("probe", "other")),
        Err(OpError::NotFound(_))
    ));
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    assert!(matches!(
        answered(cx, cli("probe", "say")),
        Err(OpError::NotFound(_))
    ));
}

#[gpui_kit::test]
fn a_cli_command_two_plugins_declare_is_refused(cx: &mut TestAppContext) {
    let first = probe_plugin(&[], CLI_PROBE);
    let second = probe_plugin(&[], CLI_PROBE);
    let mut value = sample_manifest();
    value["cli"] = json!([{ "name": "probe", "commands": [{ "name": "say" }] }]);
    write_plugin(first.path(), &value);
    value["id"] = json!("dev.test.other");
    write_plugin(second.path(), &value);
    for dir in [&first, &second] {
        std::fs::write(dir.path().join("main.js"), CLI_PROBE).unwrap();
    }
    let other = {
        let root = dunce::canonicalize(second.path()).unwrap();
        Plugin {
            id: PluginId("dev.test.other".into()),
            source: root.display().to_string(),
            approved: serde_json::to_value(Grant::of(&read_manifest(&root).unwrap(), &root))
                .unwrap(),
            root,
            commit: None,
            enabled: true,
            skills: Vec::new(),
            error: None,
        }
    };
    open(cx, &[installed(first.path()), other]);
    let error = answered(
        cx,
        Op::Plugin(PluginOp::Cli {
            name: "probe".into(),
            command: "say".into(),
            argv: vec![],
            cwd: "/work".into(),
            thread_id: None,
            project_id: None,
        }),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("more than one plugin"),
        "{error}"
    );
}

#[gpui_kit::test]
fn an_instruct_plugin_appends_text_for_a_start(cx: &mut TestAppContext) {
    let plugin = instruct_plugin(
        r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind === "instruct") {
          reply(job.id, { text: `persona for ${job.threadId} in ${job.projectId}` });
        }
      }
    });
  }
  render() { return "instruct"; }
}
"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    let text = ask_instruct(cx);
    assert_eq!(
        text,
        format!("Plugin dev.test.instruct:\npersona for {THREAD} in prj_test")
    );
}

#[gpui_kit::test]
fn an_instruct_timeout_contributes_nothing(cx: &mut TestAppContext) {
    let plugin = instruct_plugin(
        r#"
import { View } from "gpui-kit";
import { next } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        await next();
      }
    });
  }
  render() { return "instruct"; }
}
"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    let host = cx.update(|cx| cx.global::<PluginHostGlobal>().0.clone());
    let pending = cx.update(|cx| {
        cx.spawn(async move |cx| {
            crate::host::instruct(&host, &thread(), &ProjectId("prj_test".into()), cx).await
        })
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(INSTRUCT_TIMEOUT + Duration::from_millis(100));
    cx.run_until_parked();
    match cx.foreground_executor().block_test(pending).unwrap() {
        OpResult::Output(Some(text)) => assert!(text.is_empty(), "{text}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[gpui_kit::test]
fn instruct_text_is_capped(cx: &mut TestAppContext) {
    let plugin = instruct_plugin(
        r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind === "instruct") {
          reply(job.id, "x".repeat(5000));
        }
      }
    });
  }
  render() { return "instruct"; }
}
"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    let text = ask_instruct(cx);
    let prefix = "Plugin dev.test.instruct:\n";
    assert!(text.starts_with(prefix), "{text}");
    let body = &text[prefix.len()..];
    assert_eq!(body.len(), 4096);
    assert!(body.chars().all(|c| c == 'x'));
}

#[gpui_kit::test]
fn instruct_text_is_capped_in_bytes(cx: &mut TestAppContext) {
    let plugin = instruct_plugin(
        r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind === "instruct") {
          reply(job.id, "é".repeat(3000));
        }
      }
    });
  }
  render() { return "instruct"; }
}
"#,
    );
    open(cx, &[installed_at(plugin.path())]);
    let text = ask_instruct(cx);
    let prefix = "Plugin dev.test.instruct:\n";
    assert!(text.starts_with(prefix), "{text}");
    let body = &text[prefix.len()..];
    assert_eq!(body.len(), 4096);
    assert!(body.chars().all(|c| c == 'é'), "{body}");
}

#[gpui_kit::test]
fn instruct_joined_text_is_capped_across_plugins(cx: &mut TestAppContext) {
    let script = |fill: &str| {
        format!(
            r#"
import {{ View }} from "gpui-kit";
import {{ next, reply }} from "kagan/jobs";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        if (job.kind === "instruct") {{
          reply(job.id, "{fill}".repeat(5000));
        }}
      }}
    }});
  }}
  render() {{ return "instruct"; }}
}}
"#
        )
    };
    let plugins: Vec<_> = ["a", "b", "c", "d"]
        .into_iter()
        .map(|suffix| instruct_plugin_id(&format!("dev.test.instruct.{suffix}"), &script(suffix)))
        .collect();
    let installed: Vec<_> = plugins
        .iter()
        .map(|plugin| installed_at(plugin.path()))
        .collect();
    open(cx, &installed);
    let text = ask_instruct(cx);
    assert!(text.contains("Plugin dev.test.instruct.a:\n"));
    assert!(text.contains("Plugin dev.test.instruct.b:\n"));
    assert!(text.contains("Plugin dev.test.instruct.c:\n"));
    assert!(!text.contains("Plugin dev.test.instruct.d:\n"), "{text}");
    assert!(text.len() <= 16 * 1024, "{}", text.len());
}
