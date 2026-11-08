mod call;
mod contributions;
mod files;
mod host;
mod index;
mod install;
mod jobs;
mod manifest;
mod page;
mod runtime;
mod schedule;
mod script;
mod secret;
mod settings;
mod status;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::PluginHostGlobal;
use crate::manifest::Grant;

use kg_domain::{
    ConfigOp, EventKind, IndexOp, Op, OpError, OpResult, Plugin, PluginId, PluginOp, Project,
    ProjectId, ProjectKind, Thread, ThreadId, ThreadOp, ThreadStatus, Timestamp, ToolOutcome,
    Visibility,
};

const THREAD: &str = "thr_1";

const ID: &str = "dev.test.echo";

const MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
import { pluginId, settings, context, onContextChanged } from "kagan/host";
import { mkdir, readFile } from "fs/promises";
import { join } from "path";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.name === "echo" && job.input.text === "context") {
          reply(job.id, { text: JSON.stringify(context()) });
        } else if (job.name === "echo" && job.input.text === "env") {
          reply(job.id, { text: JSON.stringify(Object.keys(process.env)) });
        } else if (job.name === "echo" && job.input.text === "data-dir") {
          Promise.resolve().then(async () => {
            if (!process.dataDir) throw new Error("process.dataDir is unset");
            await mkdir(join(process.dataDir, "proof"));
            reply(job.id, { text: process.dataDir });
          }).catch((error) => reply(job.id, { text: String(error), isError: true }));
        } else if (job.name === "echo" && job.input.text === "watch") {
          onContextChanged().then(value => reply(job.id, { text: JSON.stringify(value) }));
        } else if (job.name === "echo") {
          reply(job.id, { text: `${pluginId()} ${job.input.text} ${settings().sounds}` });
        } else if (job.name === "broken") {
          fail(job.id, "no luck");
        } else if (job.name === "peek") {
          Promise.resolve().then(() => readFile(job.input.path, "utf8")).then(
            (text) => reply(job.id, { text }),
            (error) => reply(job.id, { text: String(error), isError: true }),
          );
        } else if (job.name === "secret") {
          reply(job.id, { text: job.threadId, isError: true });
        }
      }
    });
  }
  render() {
    return "echo";
  }
}
"#;

fn sample_manifest() -> Value {
    json!({
        "id": ID,
        "name": "Echo",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Echo things back.",
        "powers": { "clipboard": { "write": true } },
        "settings": {
            "sounds": { "type": "boolean", "title": "Sounds", "default": true },
            "board": { "type": "project", "title": "Board" },
            "retries": { "type": "number", "title": "Retries", "default": 3 },
            "notes": { "type": "folder", "title": "Notes" }
        },
        "tools": [
            { "name": "echo", "description": "Echo text", "input": "schemas/echo.json", "scope": "all" },
            { "name": "broken", "description": "Always fails", "input": "schemas/any.json", "scope": "all" },
            { "name": "hang", "description": "Never answers", "input": "schemas/any.json", "scope": "all" },
            { "name": "secret", "description": "Tagged threads only", "input": "schemas/any.json", "scope": "tagged" },
            { "name": "peek", "description": "Read a file", "input": "schemas/any.json", "scope": "all" }
        ],
        "ui": { "panels": [] }
    })
}

fn write_plugin(dir: &Path, manifest: &Value) {
    std::fs::create_dir_all(dir.join("schemas")).unwrap();
    std::fs::write(dir.join(crate::manifest::FILE), manifest.to_string()).unwrap();
    std::fs::write(dir.join("main.js"), MAIN).unwrap();
    std::fs::write(
        dir.join("schemas/echo.json"),
        json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("schemas/any.json"),
        json!({ "type": "object" }).to_string(),
    )
    .unwrap();
}

fn plugin_dir() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(dir.path(), &sample_manifest());
    dir
}

fn installed(root: &Path) -> Plugin {
    let root = dunce::canonicalize(root).unwrap();
    Plugin {
        id: PluginId(ID.into()),
        source: root.display().to_string(),
        approved: serde_json::to_value(Grant::of(&read_manifest(&root).unwrap(), &root)).unwrap(),
        root,
        commit: None,
        enabled: true,
        error: None,
    }
}

struct Opened {
    _data: TempDir,
}

impl gpui_kit::Global for Opened {}

fn open(cx: &mut TestAppContext, plugins: &[Plugin]) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, plugins);
}

fn open_kept(cx: &mut TestAppContext, data: &Path, keep: TempDir, plugins: &[Plugin]) {
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    let project = Project {
        id: ProjectId("prj_test".into()),
        kind: ProjectKind::Standard,
        name: "Project".into(),
        path: dunce::canonicalize(data).unwrap(),
        git_remote_url: None,
        sort: 0,
    };
    store.put_project(&project).unwrap();
    store
        .put_thread(&Thread {
            id: ThreadId(THREAD.into()),
            project_id: project.id,
            environment_id: None,
            title: "Thread".into(),
            title_locked: false,
            provider: "claude-code".into(),
            status: ThreadStatus::Idle,
            visibility: Visibility::Visible,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            section_id: None,
            pinned: false,
            last_read_at: None,
            latest_attention_at: Timestamp(0),
            mark: kg_domain::ThreadMark::None,
            archived_at: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        })
        .unwrap();
    for plugin in plugins {
        store.insert_plugin(plugin).unwrap();
    }
    drop(store);
    let opened =
        cx.update(|cx| Workspace::open(data.to_path_buf(), std::env::current_exe().unwrap(), cx));
    cx.foreground_executor().block_test(opened).unwrap();
    let started = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::init(cx, gpui_shell::components::components().unwrap())
    });
    cx.foreground_executor().block_test(started).unwrap();
    cx.update(|cx| cx.set_global(Opened { _data: keep }));
}

fn run(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.run_until_parked();
    cx.foreground_executor().block_test(task)
}

fn thread() -> ThreadId {
    ThreadId(THREAD.into())
}

fn tool_names(cx: &mut TestAppContext) -> (Option<String>, Vec<String>) {
    let Ok(OpResult::Tools {
        instructions,
        tools,
    }) = run(cx, Op::Thread(ThreadOp::Tools { id: thread() }))
    else {
        panic!("tools")
    };
    (
        instructions,
        tools.into_iter().map(|tool| tool.name).collect(),
    )
}

fn call(cx: &mut TestAppContext, name: &str, input: Value) -> Result<OpResult, OpError> {
    run(
        cx,
        Op::Thread(ThreadOp::CallTool {
            id: thread(),
            name: name.into(),
            input,
        }),
    )
}

fn loaded(cx: &mut TestAppContext) -> bool {
    cx.read(|cx| {
        cx.global::<PluginHostGlobal>()
            .0
            .read(cx)
            .active
            .get(&PluginId(ID.into()))
            .is_some_and(|active| active.loaded.is_some())
    })
}

fn answer(cx: &mut TestAppContext, option: &str) -> String {
    let Ok(OpResult::Interactions(interactions)) =
        run(cx, Op::Thread(ThreadOp::Interactions { id: thread() }))
    else {
        panic!("interactions")
    };
    let [interaction] = interactions.as_slice() else {
        panic!("one interaction: {interactions:?}")
    };
    let answered =
        cx.update(|cx| Workspace::answer(&thread(), interaction.id.clone(), option.into(), cx));
    cx.run_until_parked();
    cx.foreground_executor().block_test(answered).unwrap();
    interaction.title.clone()
}

fn answer_standalone(cx: &mut TestAppContext, option: &str) {
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .unwrap()
    });
    let answered =
        cx.update(|cx| Workspace::answer_standalone(approval.id.clone(), option.into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
}

fn plugin_op(cx: &mut TestAppContext, op: PluginOp) -> gpui_kit::Task<Result<OpResult, OpError>> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Plugin(op), cx));
    cx.run_until_parked();
    task
}

fn install(source: String, thread_id: Option<ThreadId>, plugin: Option<&str>) -> PluginOp {
    PluginOp::Install {
        source,
        plugin: plugin.map(str::to_owned),
        thread_id,
    }
}

fn records(cx: &mut TestAppContext) -> Vec<Plugin> {
    let task = cx.read(|cx| Workspace::global(cx).read(cx).plugins());
    cx.foreground_executor().block_test(task).unwrap()
}

fn stored_value(cx: &mut TestAppContext, key: &str) -> Option<Value> {
    let task = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .plugin_value(PluginId(ID.into()), key.into())
    });
    cx.foreground_executor().block_test(task).unwrap()
}

fn activation_of(cx: &mut TestAppContext) -> Option<u64> {
    cx.update(|cx| {
        cx.global::<PluginHostGlobal>()
            .0
            .read(cx)
            .activation(&PluginId(ID.into()))
    })
}

fn hold_commits(cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, _| host.commit_hold = Some(hold));
    });
    release
}

fn hold_writes(cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, _| host.write_hold = Some(hold));
    });
    release
}

fn hold_activation(cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, _| host.activation_hold = Some(hold));
    });
    release
}

fn hold_lifecycle(cx: &mut TestAppContext) -> async_channel::Sender<()> {
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, _| host.lifecycle_hold = Some(hold));
    });
    release
}

fn home() -> std::path::PathBuf {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key).map(std::path::PathBuf::from).unwrap()
}

fn spare_data() -> &'static Path {
    use std::sync::OnceLock;
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| tempfile::tempdir().unwrap()).path()
}

fn read_manifest(root: &Path) -> Result<crate::manifest::Manifest, OpError> {
    crate::manifest::read(root, spare_data())
}

/// The catalog the plugin crate's own tests declare against: the gpui-kit components.
fn catalog() -> gpui_shell::FrozenComponentRegistry {
    gpui_shell::components::components().unwrap()
}

fn check_plugin(root: &Path) -> crate::Result<Vec<String>> {
    crate::development::check(root, spare_data(), &catalog())
}

fn unique(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        ".ms88-{label}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn cli_plugin() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut value = sample_manifest();
    value["cli"] = json!([{ "name": "probe", "commands": [{ "name": "hang" }] }]);
    write_plugin(dir.path(), &value);
    dir
}

fn hanging_calls(cx: &mut TestAppContext) -> [gpui_kit::Task<Result<OpResult, OpError>>; 2] {
    let tool = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "hang".into(),
                input: json!({}),
            }),
            cx,
        )
    });
    let cli = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Cli {
                name: "probe".into(),
                command: "hang".into(),
                argv: vec![],
                cwd: "/work".into(),
                thread_id: None,
                project_id: None,
            }),
            cx,
        )
    });
    [tool, cli]
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

const FILES_MAIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import { read, list } from "kagan/files";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.name !== "slice") continue;
        const done = (value) => reply(job.id, { text: JSON.stringify(value) });
        const failed = (error) => reply(job.id, { text: String(error), isError: true });
        if (job.input.list) list(job.input.path).then(done, failed);
        else read(job.input.path, { offset: job.input.offset, maxBytes: job.input.maxBytes }).then(done, failed);
      }
    });
  }
  render() { return "files"; }
}
"#;

fn files_plugin(dir: &Path, folder: &str) {
    let mut value = sample_manifest();
    value["powers"]["read"] = json!([folder]);
    value["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "slice", "description": "Read a range", "input": "schemas/any.json", "scope": "all" }));
    write_plugin(dir, &value);
    std::fs::write(dir.join("main.js"), FILES_MAIN).unwrap();
}

fn slice(cx: &mut TestAppContext, input: Value) -> OpResult {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "slice".into(),
                input,
            }),
            cx,
        )
    });
    let (sender, reply) = async_channel::bounded(1);
    cx.foreground_executor()
        .spawn(async move { sender.send(task.await).await })
        .detach();
    for _ in 0..2000 {
        cx.run_until_parked();
        if let Ok(result) = reply.try_recv() {
            return result.unwrap();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("slice did not answer")
}

struct Sweep(std::path::PathBuf);

impl Drop for Sweep {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn refused_read(cx: &mut TestAppContext, entry: &str) -> String {
    let plugin = plugin_dir();
    let mut value = sample_manifest();
    value["powers"]["read"] = json!([entry]);
    write_plugin(plugin.path(), &value);
    let installing = plugin_op(cx, install(plugin.path().display().to_string(), None, None));
    cx.foreground_executor()
        .block_test(installing)
        .unwrap_err()
        .to_string()
}

struct Unlink(std::path::PathBuf);

impl Drop for Unlink {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn point_at(link: &Path, target: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        // One argument would wrap the whole command in quotes, and cmd
        // then treats the paths as relative to the current drive.
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

fn queue_folder(
    cx: &mut TestAppContext,
) -> (
    async_channel::Sender<()>,
    gpui_kit::Task<Result<OpResult, OpError>>,
    tempfile::TempDir,
) {
    let notes = tempfile::tempdir().unwrap();
    let release = hold_writes(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    answer(cx, "approve");
    cx.run_until_parked();
    (release, setting, notes)
}

fn assert_folder_did_not_land(cx: &mut TestAppContext) {
    let [record] = records(cx).try_into().unwrap();
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert!(
        !approved.paths.contains_key("notes"),
        "the folder path must not land on the next activation"
    );
    assert_eq!(stored_value(cx, "notes"), None);
}

fn assert_folder_landed(cx: &mut TestAppContext, path: &std::path::Path) {
    let [record] = records(cx).try_into().unwrap();
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert_eq!(approved.paths.get("notes"), Some(&path.to_path_buf()));
    assert_eq!(
        stored_value(cx, "notes"),
        Some(json!(path.display().to_string()))
    );
}

fn save_notes_now(cx: &mut TestAppContext) -> (tempfile::TempDir, std::path::PathBuf) {
    let notes = tempfile::tempdir().unwrap();
    let path = dunce::canonicalize(notes.path()).unwrap();
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    answer(cx, "approve");
    cx.foreground_executor().block_test(setting).unwrap();
    (notes, path)
}

fn probe_plugin(areas: &[&str], script: &str) -> TempDir {
    let dir = plugin_dir();
    let mut value = sample_manifest();
    value["powers"]["areas"] = json!(areas);
    value["commands"] =
        json!([{ "id": "ping", "title": "Ping", "key": "ctrl-alt-p", "when": "thread" }]);
    value["tools"].as_array_mut().unwrap().push(json!({ "name": "probe", "description": "Probe host areas", "input": "schemas/any.json", "scope": "all" }));
    write_plugin(dir.path(), &value);
    std::fs::write(dir.path().join("main.js"), script).unwrap();
    dir
}

fn real_call(cx: &mut TestAppContext, name: &str) -> Result<OpResult, OpError> {
    answered(
        cx,
        Op::Thread(ThreadOp::CallTool {
            id: thread(),
            name: name.into(),
            input: json!({}),
        }),
    )
}

fn answered(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let receive = dispatched(cx, op);
    settled(cx, &receive, Duration::ZERO)
}

fn dispatched(
    cx: &mut TestAppContext,
    op: Op,
) -> async_channel::Receiver<Result<OpResult, OpError>> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let (sender, receive) = async_channel::bounded(1);
    cx.foreground_executor()
        .spawn(async move {
            let _ = sender.send(task.await).await;
        })
        .detach();
    receive
}

/// Waits for an answer, moving the test clock by `tick` each round so timers can fire.
fn settled(
    cx: &mut TestAppContext,
    receive: &async_channel::Receiver<Result<OpResult, OpError>>,
    tick: Duration,
) -> Result<OpResult, OpError> {
    for _ in 0..2000 {
        cx.run_until_parked();
        if let Ok(result) = receive.try_recv() {
            return result;
        }
        cx.executor().advance_clock(tick);
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no answer")
}

const AREAS_PROBE: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
import * as threads from "kagan/threads";
import * as database from "kagan/database";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          await threads.setMetadata(job.threadId, { own: true });
          await threads.setPinned(job.threadId, true);
          await threads.markRead(job.threadId);
          await database.migrate(["CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT NOT NULL)"]);
          await database.migrate(["CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT NOT NULL)"]);
          await database.run("INSERT INTO notes (text) VALUES (?)", ["kept"]);
          try {
            await database.transaction([
              { sql: "INSERT INTO notes (text) VALUES (?)", params: ["rolled back"] },
              { sql: "INSERT INTO notes (text) VALUES (?)", params: [null] }
            ]);
          } catch (_) {}
          const result = {
            thread: await threads.get(job.threadId),
            metadata: await threads.metadata(job.threadId),
            sidebar: threads.sidebar(),
            projects: await threads.projects(),
            events: await threads.events(job.threadId, { afterSeq: 0, limit: 1 }),
            rows: await database.query("SELECT * FROM notes", []),
          };
          reply(job.id, { text: JSON.stringify(result) });
        } catch (error) { fail(job.id, String(error)); }
      }
    });
  }
  render() { return "probe"; }
}
"#;

const RENAME_PROBE: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as threads from "kagan/threads";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          await threads.rename(job.threadId, "Agent");
          reply(job.id, { text: "renamed" });
        } catch (error) {
          reply(job.id, { text: String(error) });
        }
      }
    });
  }
  render() { return "probe"; }
}
"#;

const THREAD_OPS: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
import * as threads from "kagan/threads";
import { settings } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        const id = job.threadId;
        const spawn = (metadata, extra) => {
          const options = {
            projectId: "prj_test",
            program: "missing-program",
            environment: { kind: "project-default" },
            ...extra,
          };
          if (metadata) options.metadata = metadata;
          return threads.spawn(options);
        };
        const calls = [
          ["list", () => threads.list()],
          ["listFiltered", () => threads.list({ projectId: "prj_test", archived: false })],
          ["get", () => threads.get(id)],
          ["events", () => threads.events(id, { afterSeq: 0, limit: 1 })],
          ["settle", () => threads.wait(id, { timeoutMs: 1000 })],
          ["send", () => threads.send(id, "hello")],
          ["output", () => threads.output(id)],
          ["wait", () => threads.waitEvent(id, { type: "nope", afterSeq: 0, timeoutMs: 1 })],
          ["pin", () => threads.setPinned(id, false)],
          ["read", () => threads.markRead(id)],
          ["writeMeta", () => threads.setMetadata(id, { n: 1 })],
          ["meta", () => threads.metadata(id)],
          ["projects", () => threads.projects()],
          ["project", () => threads.project("prj_test")],
          ["environment", () => threads.environment("env_missing")],
          ["restore", () => threads.restoreEnvironment(id)],
          ["spawn", () => spawn()],
          ["spawnMeta", () => spawn({ tag: true })],
          ["spawnAgent", () => spawn(null, { agent: "worker" })],
          ["spawnProvider", () => spawn(null, { provider: "claude-code" })],
          ["settings", async () => settings()],
          ["ask", async () => {
            const interaction = await threads.ask(id, "Ship it?", [
              { id: "yes", label: "Yes" },
              { id: "no", label: "No" },
            ]);
            return { interaction };
          }],
          ["stop", () => threads.stop(id)],
          ["archive", () => threads.archive(id)],
          ["unarchive", () => threads.unarchive(id)],
          ["remove", () => threads.remove(id)],
        ];
        const steps = [];
        for (const [name, call] of calls) {
          try {
            steps.push({ name, ok: true, value: await call() });
          } catch (error) {
            steps.push({ name, ok: false, error: String(error) });
          }
        }
        reply(job.id, { text: JSON.stringify(steps) });
      }
    });
  }
  render() { return "threads"; }
}
"#;

fn call_and_approve(cx: &mut TestAppContext) -> Result<OpResult, OpError> {
    let receive = dispatched(
        cx,
        Op::Thread(ThreadOp::CallTool {
            id: thread(),
            name: "probe".into(),
            input: json!({}),
        }),
    );
    for _ in 0..2000 {
        cx.run_until_parked();
        if cx.read(|cx| Workspace::global(cx).read(cx).approvals().next().is_some()) {
            answer_standalone(cx, "approve");
        }
        if let Ok(result) = receive.try_recv() {
            return result;
        }
        cx.executor().advance_clock(Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no answer")
}

fn pick_worker(cx: &mut TestAppContext) {
    run(
        cx,
        Op::Plugin(PluginOp::Config {
            id: PluginId(ID.into()),
            thread_id: None,
            op: ConfigOp::Set {
                key: "worker".into(),
                value: Some(json!({ "provider": "picked-agent", "model": "picked-model" })),
            },
        }),
    )
    .unwrap();
}

fn hides_agent(value: &Value) {
    for hidden in ["provider", "model", "reasoningLevel", "permissionMode"] {
        assert!(
            value.get(hidden).is_none(),
            "a plugin reads {hidden}: {value}"
        );
    }
    assert!(!value.to_string().contains("claude-code"), "{value}");
}

fn sleep_command() -> Command {
    #[cfg(unix)]
    {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 10"]);
        command
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 10"]);
        command
    }
}

const COMMAND_PROBE: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind === "command" && job.command === "ping") reply(job.id, null);
        else fail(job.id, "wrong command");
      }
    });
  }
  render() { return "commands"; }
}
"#;

fn write_theme_files(themes: &Path) -> TempDir {
    std::fs::create_dir(themes).unwrap();
    std::fs::write(
        themes.join("midnight.json"),
        include_str!("../../../docs/themes/ocean.json"),
    )
    .unwrap();
    std::fs::write(
        themes.join("blank.json"),
        include_str!("../../../docs/themes/ocean.json").replace("\"Ocean\"", "\" \""),
    )
    .unwrap();
    std::fs::write(
        themes.join("odd.json"),
        include_str!("../../../docs/themes/ocean.json").replace("#f2f2f2", "f2f2f2"),
    )
    .unwrap();
    let ocean = include_str!("../../../docs/themes/ocean.json");
    let padded = format!("{ocean}{}", " ".repeat(64 * 1024));
    std::fs::write(themes.join("big.json"), padded).unwrap();
    let long = ocean.replace("\"Ocean\"", &format!("\"{}\"", "o".repeat(65)));
    std::fs::write(themes.join("long.json"), long).unwrap();
    std::fs::write(
        themes.join("lines.json"),
        ocean.replace("\"Ocean\"", "\"Oc\\nean\""),
    )
    .unwrap();
    std::fs::create_dir(themes.join("dir.json")).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("t.json"), ocean).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path().join("t.json"), themes.join("link.json"))
            .unwrap();
        let made = Command::new("mkfifo")
            .arg(themes.join("pipe.json"))
            .status()
            .unwrap();
        assert!(made.success());
    }
    outside
}

const CLI_PROBE: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        reply(job.id, { exitCode: 3, stdout: job.command + ":" + job.argv.join(","), stderr: job.cwd });
      }
    });
  }
  render() { return "probe"; }
}
"#;

const FORWARD: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as process from "kagan/process";


const modules = { process };
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          const { module, call, args } = job.input;
          const value = await modules[module][call](...args);
          reply(job.id, { text: JSON.stringify(value ?? null) });
        } catch (error) { reply(job.id, { text: String(error), isError: true }); }
      }
    });
  }
  render() { return "forward"; }
}
"#;

const THREADS_FORWARD: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as threads from "kagan/threads";


const modules = { threads };
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          const { module, call, args } = job.input;
          const value = await modules[module][call](...args);
          reply(job.id, { text: JSON.stringify(value ?? null) });
        } catch (error) { reply(job.id, { text: String(error), isError: true }); }
      }
    });
  }
  render() { return "forward"; }
}
"#;

/// A plugin that forwards one host call, so the call runs inside a real script scope.
fn forwarding(cx: &mut TestAppContext, powers: Value) -> (TempDir, Grant) {
    let plugin = probe_plugin(&["process"], FORWARD);
    let mut value = sample_manifest();
    value["powers"] = powers;
    value["powers"]["areas"] = json!(["process"]);
    value["tools"].as_array_mut().unwrap().push(json!({ "name": "probe", "description": "Forward a host call", "input": "schemas/any.json", "scope": "all" }));
    write_plugin(plugin.path(), &value);
    std::fs::write(plugin.path().join("main.js"), FORWARD).unwrap();
    open(cx, &[installed(plugin.path())]);
    let grant = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    (plugin, grant)
}

fn forwarded(module: &str, call: &str, args: &Value) -> Op {
    Op::Thread(ThreadOp::CallTool {
        id: thread(),
        name: "probe".into(),
        input: json!({ "module": module, "call": call, "args": args }),
    })
}

fn forward(
    cx: &mut TestAppContext,
    module: &str,
    call: &str,
    args: &Value,
) -> Result<Value, String> {
    let reply = answered(cx, forwarded(module, call, args));
    forwarded_reply(reply)
}

fn forwarded_reply(reply: Result<OpResult, OpError>) -> Result<Value, String> {
    let OpResult::ToolReply { text, is_error } = reply.unwrap() else {
        panic!("tool reply")
    };
    if is_error {
        Err(text)
    } else {
        Ok(serde_json::from_str(&text).unwrap())
    }
}

fn shell() -> (&'static str, Vec<&'static str>) {
    if cfg!(windows) {
        ("cmd", vec!["/C", "echo hello & echo error 1>&2"])
    } else {
        ("sh", vec!["-c", "printf 'hello\\n'; printf 'error\\n' >&2"])
    }
}

fn sleeping() -> Value {
    if cfg!(windows) {
        json!(["/C", "ping -n 30 127.0.0.1 >nul"])
    } else {
        json!(["-c", "sleep 30"])
    }
}

#[cfg(unix)]
fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn start_tree(
    cx: &mut TestAppContext,
    name: &str,
) -> (
    Vec<String>,
    async_channel::Receiver<Result<OpResult, OpError>>,
) {
    let marks = tempfile::tempdir().unwrap();
    let script = format!(
        "sleep 30 & echo $! > {0}/child; echo $$ > {0}/parent; wait",
        marks.path().display()
    );
    let receive = dispatched(
        cx,
        forwarded("process", "run", &json!([name, ["-c", script]])),
    );
    let mut pids = Vec::new();
    for _ in 0..2000 {
        cx.run_until_parked();
        pids = ["parent", "child"]
            .map(|file| std::fs::read_to_string(marks.path().join(file)).unwrap_or_default())
            .map(|text| text.trim().to_owned())
            .into();
        if pids.iter().all(|pid| !pid.is_empty()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(pids.iter().all(|pid| alive(pid)), "{pids:?}");
    (pids, receive)
}

fn installed_example(root: &Path) -> Plugin {
    let root = dunce::canonicalize(root).unwrap();
    let manifest = read_manifest(&root).unwrap();
    Plugin {
        id: manifest.id.clone(),
        source: root.display().to_string(),
        approved: serde_json::to_value(Grant::of(&manifest, &root)).unwrap(),
        root,
        commit: None,
        enabled: true,
        error: None,
    }
}

const IDLE_PLUGIN: &str = "dev.test.idle";

const QUIET_PLUGIN: &str = "dev.test.quiet";

fn event_plugin(id: &str, events: &[&str]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut manifest = json!({
        "id": id,
        "name": "Events",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "powers": { "areas": ["threads"] }
    });
    manifest["events"] = json!(events);
    std::fs::write(dir.path().join(crate::manifest::FILE), manifest.to_string()).unwrap();
    std::fs::write(
        dir.path().join("main.js"),
        "import { View } from \"gpui-kit\";\nexport default class Plugin extends View { render() { return \"plugin\"; } }\n",
    )
    .unwrap();
    dir
}

fn installed_at(root: &Path) -> Plugin {
    let root = dunce::canonicalize(root).unwrap();
    let manifest = read_manifest(&root).unwrap();
    Plugin {
        id: manifest.id.clone(),
        source: root.display().to_string(),
        approved: serde_json::to_value(Grant::of(&manifest, &root)).unwrap(),
        root,
        commit: None,
        enabled: true,
        error: None,
    }
}

fn event_script(seen: &Path) -> String {
    let path = serde_json::to_string(&seen.display().to_string()).unwrap();
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
        if (job.kind === "event") {{
          await writeFile({path}, JSON.stringify({{
            kind: job.kind, type: job.type, threadId: job.threadId, projectId: job.projectId, data: job.data
          }}));
          reply(job.id, null);
        }}
      }}
    }});
  }}
  render() {{ return "plugin"; }}
}}
"#
    )
}

fn plugin_loaded(cx: &mut TestAppContext, id: &str) -> bool {
    cx.read(|cx| {
        cx.global::<PluginHostGlobal>()
            .0
            .read(cx)
            .is_loaded(&PluginId(id.into()))
    })
}

#[gpui_kit::test]
fn tool_events_reach_a_subscribed_plugin(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let subscriber = event_plugin(
        "dev.test.tools",
        &["thread.tool.started", "thread.tool.ended"],
    );
    open(
        cx,
        &[installed(plugin.path()), installed_at(subscriber.path())],
    );
    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let dir = data.join("plugins").join("dev.test.tools").join("data");
    let started_path = dir.join("started.json");
    let ended_path = dir.join("ended.json");
    let started_js = serde_json::to_string(&started_path.display().to_string()).unwrap();
    let ended_js = serde_json::to_string(&ended_path.display().to_string()).unwrap();
    std::fs::write(
        subscriber.path().join("main.js"),
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
        const path = job.type === "tool.started" ? {started_js} : {ended_js};
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
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false,
        }
    );
    let started = job_seen(cx, &started_path, "tool.started");
    let ended = job_seen(cx, &ended_path, "tool.ended");
    assert_eq!(started["data"]["name"], "echo", "{started}");
    assert_eq!(ended["data"]["status"], "ok", "{ended}");
    assert_eq!(
        started["data"]["id"], ended["data"]["id"],
        "{started} {ended}"
    );
    assert_eq!(started["data"].as_object().unwrap().len(), 2, "{started}");
    assert_eq!(ended["data"].as_object().unwrap().len(), 2, "{ended}");
    assert!(
        started["data"]["id"].as_str().unwrap().starts_with("tool_"),
        "{started}"
    );
}

const WAIT_TOOL_ENDED: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as threads from "kagan/threads";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind !== "tool" || job.name !== "probe") continue;
        const event = await threads.waitEvent(job.threadId, {
          type: "thread.tool.ended",
          timeoutMs: 5000,
        });
        reply(job.id, { text: JSON.stringify(event), isError: false });
      }
    });
  }
  render() { return "plugin"; }
}
"#;

#[gpui_kit::test]
fn wait_event_returns_a_tool_ended_row(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads"], WAIT_TOOL_ENDED);
    open(cx, &[installed(plugin.path())]);
    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let mut store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .append_events(
            &thread(),
            Timestamp(2),
            [EventKind::ToolEnded {
                id: "tool_seed".into(),
                status: ToolOutcome::Ok,
            }],
        )
        .unwrap();
    drop(store);
    let Ok(OpResult::ToolReply { text, is_error }) = call(cx, "probe", json!({})) else {
        panic!("probe")
    };
    assert!(!is_error);
    let event: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(event["type"], "tool.ended", "{event}");
    assert_eq!(event["data"]["id"], "tool_seed", "{event}");
    assert_eq!(event["data"]["status"], "ok", "{event}");
}

fn job_seen(cx: &mut TestAppContext, path: &Path, kind: &str) -> Value {
    for _ in 0..2000 {
        cx.run_until_parked();
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(value) = serde_json::from_str::<Value>(&text)
            && value["type"] == kind
        {
            return value;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no {kind} job")
}

fn installing_now(cx: &mut TestAppContext) -> Option<String> {
    cx.read(|cx| crate::PluginHost::global(cx).read(cx).installing())
}

struct Indexed {
    source: String,
    commit: String,
    listed: String,
    missing: String,
    data: std::path::PathBuf,
    _repos: [TempDir; 2],
}

fn indexed(cx: &mut TestAppContext) -> Indexed {
    let plugin = tempfile::tempdir().unwrap();
    git(plugin.path(), &["init", "--quiet", "--initial-branch=main"]);
    write_plugin(&plugin.path().join("echo"), &sample_manifest());
    git(plugin.path(), &["add", "."]);
    git(plugin.path(), &["commit", "--quiet", "-m", "one"]);
    let commit = git(plugin.path(), &["rev-parse", "HEAD"]);
    let source = format!("git:{}@main", plugin.path().display());
    let index = tempfile::tempdir().unwrap();
    git(index.path(), &["init", "--quiet", "--initial-branch=main"]);
    std::fs::create_dir(index.path().join(".kagan")).unwrap();
    std::fs::write(
        index.path().join(".kagan/index.json"),
        json!({
            "schemaVersion": 1,
            "name": "acme",
            "plugins": [{ "name": "echo", "description": "Echoes text", "source": source, "plugin": "echo" }]
        })
        .to_string(),
    )
    .unwrap();
    git(index.path(), &["add", "."]);
    git(index.path(), &["commit", "--quiet", "-m", "index"]);
    let listed = format!("git:{}@main", index.path().display());
    let missing = format!("git:file://{}@main", index.path().join("missing").display());
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("indexes.json"),
        json!([missing]).to_string(),
    )
    .unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    Indexed {
        source,
        commit,
        listed,
        missing,
        data: path,
        _repos: [plugin, index],
    }
}

fn index_op(op: IndexOp) -> Op {
    Op::Plugin(PluginOp::Index { op })
}

fn search(cx: &mut TestAppContext, text: &str) -> (Vec<kg_domain::Listing>, Vec<String>) {
    let Ok(OpResult::Listings { listings, warnings }) =
        run(cx, Op::Plugin(PluginOp::Search { text: text.into() }))
    else {
        panic!("search")
    };
    (listings, warnings)
}

fn local_index(dir: &Path, name: &str, plugins: &Value) {
    std::fs::create_dir_all(dir.join(".kagan")).unwrap();
    std::fs::write(
        dir.join(".kagan/index.json"),
        json!({ "schemaVersion": 1, "name": name, "plugins": plugins }).to_string(),
    )
    .unwrap();
}

const PAGE_PLUGIN: &str = "dev.test.page";

struct PageRun {
    _plugin: tempfile::TempDir,
    seen: std::path::PathBuf,
    posted: async_channel::Receiver<String>,
    life: std::sync::Arc<()>,
}

fn page_script(seen: &std::path::Path) -> String {
    let path = serde_json::to_string(&seen.display().to_string()).unwrap();
    format!(
        r#"
import {{ View }} from "gpui-kit";
import {{ next, reply, fail }} from "kagan/jobs";
import {{ post }} from "kagan/panel";
import {{ writeFile }} from "fs/promises";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        try {{
          if (job.kind === "page") {{
            if (job.message && job.message.big) {{
              await post(job.panelId, "x".repeat(2 * 1024 * 1024));
            }} else {{
              await writeFile({path}, JSON.stringify(job.message));
              await post(job.panelId, {{ echo: job.message }});
            }}
          }}
          reply(job.id, null);
        }} catch (error) {{
          await writeFile({path}, "error:" + String(error));
          fail(job.id, String(error));
        }}
      }}
    }});
  }}
  render() {{ return ""; }}
}}
"#
    )
}

fn open_page(cx: &mut TestAppContext) -> PageRun {
    let plugin = tempfile::tempdir().unwrap();
    let ui = plugin.path().join("ui");
    std::fs::create_dir(&ui).unwrap();
    std::fs::write(ui.join("index.html"), "<p>page</p>").unwrap();
    let manifest = json!({
        "id": PAGE_PLUGIN,
        "name": "Page",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "ui": { "panels": [{ "id": "board", "title": "Board", "web": "ui/index.html", "placement": "right" }] }
    });
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        manifest.to_string(),
    )
    .unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "import { View } from \"gpui-kit\";\nexport default class Plugin extends View { render() { return \"\"; } }\n",
    )
    .unwrap();
    open(cx, &[installed_at(plugin.path())]);
    let data = cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned());
    let seen = data
        .join("plugins")
        .join(PAGE_PLUGIN)
        .join("data")
        .join("seen.json");
    std::fs::write(plugin.path().join("main.js"), page_script(&seen)).unwrap();
    let (sender, posted) = async_channel::bounded(4);
    cx.update(|cx| {
        let ui = crate::HostUi::global(cx).unwrap();
        cx.subscribe(&ui, move |_, event, _| {
            if let crate::HostUiEvent::PostPage { message, reply, .. } = event {
                let _ = sender.try_send(message.clone());
                let _ = reply.try_send(Ok(()));
            }
        })
        .detach();
    });
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update(|cx| {
        handle
            .update(cx, |_, _window, cx| {
                crate::PluginHost::global(cx)
                    .update(cx, |host, cx| {
                        host.load_plugin(&PluginId(PAGE_PLUGIN.into()), cx)
                    })
                    .unwrap();
            })
            .unwrap();
    });
    PageRun {
        _plugin: plugin,
        seen,
        posted,
        life: std::sync::Arc::new(()),
    }
}

fn deliver_page(cx: &mut TestAppContext, run: &PageRun, raw: &str) -> Result<(), OpError> {
    let alive = std::sync::Arc::downgrade(&run.life);
    cx.update(|cx| {
        crate::PluginHost::global(cx).update(cx, |host, cx| {
            host.deliver_page(&PluginId(PAGE_PLUGIN.into()), "board", raw, alive, cx)
        })
    })
}

fn open_page_holding_next(cx: &mut TestAppContext) -> tempfile::TempDir {
    let plugin = tempfile::tempdir().unwrap();
    let ui = plugin.path().join("ui");
    std::fs::create_dir(&ui).unwrap();
    std::fs::write(ui.join("index.html"), "<p>page</p>").unwrap();
    let manifest = json!({
        "id": PAGE_PLUGIN,
        "name": "Page",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "ui": { "panels": [{ "id": "board", "title": "Board", "web": "ui/index.html", "placement": "right" }] }
    });
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        manifest.to_string(),
    )
    .unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "import { View } from \"gpui-kit\";\nexport default class Plugin extends View { render() { return \"\"; } }\n",
    )
    .unwrap();
    open(cx, &[installed_at(plugin.path())]);
    let ready = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .data_dir()
            .join("plugins")
            .join(PAGE_PLUGIN)
            .join("data")
            .join("ready")
    });
    let ready_lit = serde_json::to_string(&ready.display().to_string()).unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            r#"
import {{ View }} from "gpui-kit";
import {{ writeFile }} from "fs/promises";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      await writeFile({ready_lit}, "ready");
      await new Promise(() => {{}});
    }});
  }}
  render() {{ return ""; }}
}}
"#
        ),
    )
    .unwrap();
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update(|cx| {
        handle
            .update(cx, |_, _window, cx| {
                crate::PluginHost::global(cx)
                    .update(cx, |host, cx| {
                        host.load_plugin(&PluginId(PAGE_PLUGIN.into()), cx)
                    })
                    .unwrap();
            })
            .unwrap();
    });
    for _ in 0..2000 {
        cx.run_until_parked();
        if ready.is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready.is_file(), "plugin never started");
    plugin
}

fn flood_page(
    cx: &mut TestAppContext,
    life: &std::sync::Arc<()>,
    raw: &str,
) -> Result<(), OpError> {
    let alive = std::sync::Arc::downgrade(life);
    cx.update(|cx| {
        crate::PluginHost::global(cx).update(cx, |host, cx| {
            host.deliver_page(&PluginId(PAGE_PLUGIN.into()), "board", raw, alive, cx)
        })
    })
}

fn pending_page_bytes(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| {
        crate::PluginHost::global(cx)
            .read(cx)
            .pending_page_bytes(&PluginId(PAGE_PLUGIN.into()))
    })
}

fn instruct_plugin(script: &str) -> TempDir {
    instruct_plugin_id("dev.test.instruct", script)
}

fn instruct_plugin_id(id: &str, script: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let manifest = json!({
        "id": id,
        "name": "Instruct",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instruct": true,
    });
    std::fs::write(dir.path().join(crate::manifest::FILE), manifest.to_string()).unwrap();
    std::fs::write(dir.path().join("main.js"), script).unwrap();
    dir
}

fn ask_instruct(cx: &mut TestAppContext) -> String {
    let host = cx.update(|cx| cx.global::<PluginHostGlobal>().0.clone());
    let task = cx.update(|cx| {
        cx.spawn(async move |cx| {
            crate::host::instruct(&host, &thread(), &ProjectId("prj_test".into()), cx).await
        })
    });
    cx.run_until_parked();
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Output(Some(text)) => text,
        other => panic!("unexpected {other:?}"),
    }
}
