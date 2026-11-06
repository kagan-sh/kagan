use std::process::Command;
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{
    EnvironmentSpec, EventKind, Op, OpError, OpResult, PluginId, ProjectId, Run, ThreadOp,
    ThreadStatus, Timestamp, TurnOutcome,
};
use serde_json::{Value, json};

use crate::source;

use super::{
    ID, IDLE_PLUGIN, QUIET_PLUGIN, THREAD, answer_standalone, catalog, check_plugin, event_plugin,
    event_script, install, installed_at, job_seen, open, plugin_loaded, plugin_op, run,
    sample_manifest, sleep_command, thread, write_plugin,
};

#[test]
fn shared_logs_escape_messages_and_resume_after_complete_lines() {
    let data = tempfile::tempdir().unwrap();
    let id = PluginId(ID.into());
    crate::log(data.path(), &id, crate::Level::Info, "first\nline").unwrap();
    let path = data.path().join("plugins").join(ID).join("logs.jsonl");
    let OpResult::Logs { text, after, .. } = crate::development::logs(&path, 0).unwrap() else {
        panic!("logs")
    };
    assert_eq!(text.lines().count(), 1);
    let line: Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(line["message"], "first\nline");
    crate::log(data.path(), &id, crate::Level::Error, "crash").unwrap();
    let OpResult::Logs {
        text, after: next, ..
    } = crate::development::logs(&path, after).unwrap()
    else {
        panic!("logs")
    };
    assert!(text.contains("crash"));
    assert!(!text.contains("first"));
    assert!(next > after);
    assert!("unknown".parse::<crate::Level>().is_err());
}

#[test]
fn scaffold_and_types_match_the_running_shell() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("my-plugin");
    crate::development::new("my-plugin", Some(root.clone()), &catalog()).unwrap();
    check_plugin(&root).unwrap();
    let host = std::fs::read_to_string(root.join("kagan.d.ts")).unwrap();
    assert!(host.contains("declare module \"kagan/jobs\""));
    assert!(host.contains("export function next(): Promise<Job>"));
    assert!(host.contains("export function setStatus"));
    assert!(host.contains("declare module \"kagan/process\""));
    assert!(!host.contains("kagan/downloads"));
    assert!(host.contains("overlay?: { width: number; height: number }"));
    let shell = std::fs::read_to_string(root.join("gpui-kit.d.ts")).unwrap();
    assert!(shell.contains("View"));
    for call in [
        "on_drag<",
        "on_drag_move<",
        "on_drop<",
        "interface DropEvent",
    ] {
        assert!(shell.contains(call), "{call} is declared");
    }
    assert!(crate::development::new("my-plugin", Some(root), &catalog()).is_err());
    assert!(crate::development::new("../escape", None, &catalog()).is_err());
}

#[test]
fn a_name_the_disk_cannot_hold_is_refused_plainly() {
    let directory = tempfile::tempdir().unwrap();
    let name = "a".repeat(300);
    let error =
        crate::development::new(&name, Some(directory.path().join(&name)), &catalog()).unwrap_err();
    assert_eq!(error.to_string(), "the plugin name is too long");
}

#[test]
fn logs_rotate_and_reset_the_cursor() {
    let data = tempfile::tempdir().unwrap();
    let id = PluginId(ID.into());
    let directory = data.path().join("plugins").join(ID);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("logs.jsonl");
    let previous = directory.join("logs.1.jsonl");
    std::fs::write(&previous, "older").unwrap();
    std::fs::File::create(&path)
        .unwrap()
        .set_len(crate::log::LIMIT)
        .unwrap();
    crate::log(data.path(), &id, crate::Level::Info, "fresh").unwrap();
    assert_eq!(
        std::fs::metadata(&previous).unwrap().len(),
        crate::log::LIMIT
    );
    let OpResult::Logs { text, after, reset } =
        crate::development::logs(&path, crate::log::LIMIT).unwrap()
    else {
        panic!("logs")
    };
    assert!(reset);
    assert!(text.contains("fresh"));
    assert_eq!(after, std::fs::metadata(path).unwrap().len());
    assert!(
        crate::log(
            data.path(),
            &id,
            crate::Level::Info,
            &"x".repeat(usize::try_from(crate::log::LIMIT).unwrap())
        )
        .is_err()
    );
}

#[test]
fn bounded_programs_capture_output_and_stop_on_timeout() {
    let mut command = Command::new("git");
    command.arg("--version");
    let (_keep, never) = async_channel::bounded::<()>(1);
    let output = crate::source::run(command, Duration::from_secs(5), &never).unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("git version")
    );
    let started = std::time::Instant::now();
    let error = crate::source::run(sleep_command(), Duration::from_millis(30), &never).unwrap_err();
    assert!(
        matches!(error, crate::source::RunError::Timeout),
        "{error:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn a_stopped_git_step_ends_at_once_with_a_plain_message() {
    let (stop, stopped) = async_channel::bounded::<()>(1);
    let started = std::time::Instant::now();
    let later = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        drop(stop);
    });
    let error = source::run_git(sleep_command(), Duration::from_secs(30), &stopped).unwrap_err();
    later.join().unwrap();
    assert_eq!(error, OpError::Failed("the install was stopped".into()));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_git_step_past_its_limit_says_so() {
    let (_keep, never) = async_channel::bounded::<()>(1);
    let error = source::run_git(sleep_command(), Duration::from_millis(30), &never).unwrap_err();
    assert_eq!(
        error,
        OpError::Failed("git took longer than 2 minutes; check the host and try again".into())
    );
}

#[gpui_kit::test]
fn a_thread_idle_event_reaches_only_a_subscribed_plugin(cx: &mut TestAppContext) {
    let idle = event_plugin(IDLE_PLUGIN, &["thread.idle"]);
    let quiet = event_plugin(QUIET_PLUGIN, &["thread.archived"]);
    open(cx, &[installed_at(idle.path()), installed_at(quiet.path())]);
    assert!(!plugin_loaded(cx, IDLE_PLUGIN));
    assert!(!plugin_loaded(cx, QUIET_PLUGIN));

    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let idle_seen = data
        .join("plugins")
        .join(IDLE_PLUGIN)
        .join("data")
        .join("seen.json");
    let quiet_seen = data
        .join("plugins")
        .join(QUIET_PLUGIN)
        .join("data")
        .join("seen.json");
    std::fs::write(idle.path().join("main.js"), event_script(&idle_seen)).unwrap();
    std::fs::write(quiet.path().join("main.js"), event_script(&quiet_seen)).unwrap();

    let mut store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .set_thread_status(&thread(), ThreadStatus::Active, Timestamp(1))
        .unwrap();
    let persisted = store
        .append_events(&thread(), Timestamp(2), [EventKind::Idle])
        .unwrap();
    drop(store);
    assert!(persisted.iter().any(|event| event.kind == EventKind::Idle));
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |_, cx| {
            for event in persisted {
                cx.emit(kg_core::WorkspaceEvent::Event {
                    event,
                    project_id: ProjectId("prj_test".into()),
                });
            }
        });
    });

    for _ in 0..2000 {
        cx.run_until_parked();
        if idle_seen.is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let seen: Value = serde_json::from_str(&std::fs::read_to_string(&idle_seen).unwrap()).unwrap();
    assert_eq!(
        seen,
        json!({
            "kind": "event",
            "type": "idle",
            "threadId": THREAD,
            "projectId": "prj_test",
            "data": null
        })
    );
    assert!(plugin_loaded(cx, IDLE_PLUGIN));
    assert!(!plugin_loaded(cx, QUIET_PLUGIN));
    assert!(!quiet_seen.exists());
}

#[gpui_kit::test]
fn turn_events_reach_a_subscribed_plugin(cx: &mut TestAppContext) {
    let plugin = event_plugin(
        "dev.test.turns",
        &["thread.turn.started", "thread.turn.ended"],
    );
    open(cx, &[installed_at(plugin.path())]);
    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let dir = data.join("plugins").join("dev.test.turns").join("data");
    let started_path = dir.join("started.json");
    let ended_path = dir.join("ended.json");
    let started_js = serde_json::to_string(&started_path.display().to_string()).unwrap();
    let ended_js = serde_json::to_string(&ended_path.display().to_string()).unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            r#"
import {{ View }} from "gpui-kit";
import {{ next, reply }} from "kagan/jobs";
import {{ writeFile }} from "fs/promises";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        if (job.kind !== "event") continue;
        const path = job.type === "turn.started" ? {started_js} : {ended_js};
        await writeFile(path, JSON.stringify({{
          kind: job.kind, type: job.type, threadId: job.threadId, projectId: job.projectId, data: job.data
        }}));
        reply(job.id, null);
      }}
    }});
  }}
  render() {{ return "plugin"; }}
}}
"#
        ),
    )
    .unwrap();
    let mut store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    let persisted = store
        .append_events(
            &thread(),
            Timestamp(2),
            [
                EventKind::TurnStarted,
                EventKind::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    error: None,
                },
            ],
        )
        .unwrap();
    drop(store);
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |_, cx| {
            for event in persisted {
                cx.emit(kg_core::WorkspaceEvent::Event {
                    event,
                    project_id: ProjectId("prj_test".into()),
                });
            }
        });
    });
    let started = job_seen(cx, &started_path, "turn.started");
    assert_eq!(started["data"], Value::Null, "{started}");
    let ended = job_seen(cx, &ended_path, "turn.ended");
    assert_eq!(
        ended["data"],
        json!({ "outcome": "completed", "error": null }),
        "{ended}"
    );
}

#[gpui_kit::test]
fn created_and_deleted_events_carry_the_project(cx: &mut TestAppContext) {
    let plugin = event_plugin("dev.test.life", &["thread.created", "thread.deleted"]);
    open(cx, &[installed_at(plugin.path())]);
    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let seen = data
        .join("plugins")
        .join("dev.test.life")
        .join("data")
        .join("seen.json");
    std::fs::write(plugin.path().join("main.js"), event_script(&seen)).unwrap();

    let OpResult::Thread(forked) = run(
        cx,
        Op::Thread(ThreadOp::Run(Run {
            project_id: ProjectId("prj_test".into()),
            program: if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into(),
            title: None,
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
    )
    .unwrap() else {
        panic!("run")
    };
    assert_eq!(
        job_seen(cx, &seen, "created"),
        json!({
            "kind": "event",
            "type": "created",
            "threadId": forked.id,
            "projectId": "prj_test",
            "data": null
        })
    );

    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: forked.id.clone(),
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(deleting).unwrap();
    assert_eq!(
        job_seen(cx, &seen, "deleted"),
        json!({
            "kind": "event",
            "type": "deleted",
            "threadId": forked.id,
            "projectId": "prj_test",
            "data": null
        })
    );
}

#[gpui_kit::test]
fn removing_a_project_delivers_deleted_events_to_plugins(cx: &mut TestAppContext) {
    let plugin = event_plugin("dev.test.removal", &["thread.deleted"]);
    open(cx, &[installed_at(plugin.path())]);
    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let seen = data
        .join("plugins")
        .join("dev.test.removal")
        .join("data")
        .join("seen.json");
    std::fs::write(plugin.path().join("main.js"), event_script(&seen)).unwrap();
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Remove {
                id: ProjectId("prj_test".into()),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    assert_eq!(
        job_seen(cx, &seen, "deleted"),
        json!({
            "kind": "event", "type": "deleted", "threadId": THREAD,
            "projectId": "prj_test", "data": null
        })
    );
}

#[gpui_kit::test]
fn a_plugin_without_powers_says_what_it_cannot_do(cx: &mut TestAppContext) {
    let plugin = tempfile::tempdir().unwrap();
    let mut value = sample_manifest();
    value["powers"] = json!({});
    value["settings"] = json!({});
    write_plugin(plugin.path(), &value);
    open(cx, &[]);
    let install_task = plugin_op(cx, install(plugin.path().display().to_string(), None, None));
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .unwrap()
    });
    assert!(
        approval.title.contains(
            "It asks for no extra powers. It cannot run programs or read your other files."
        ),
        "{}",
        approval.title
    );
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(install_task).is_err());
}
