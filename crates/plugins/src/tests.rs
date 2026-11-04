mod index;
mod page;
mod secret;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{
    ConfigOp, EnvironmentSpec, EventKind, IndexOp, KeymapOp, Metadata, Op, OpError, OpResult,
    Plugin, PluginId, PluginOp, Project, ProjectId, ProjectKind, Run, Thread, ThreadId, ThreadOp,
    ThreadStatus, Timestamp, TurnOutcome, Visibility,
};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::manifest::{self, Grant};
use crate::source;
use crate::{INSTRUCT_TIMEOUT, PluginHostGlobal, TOOL_TIMEOUT};

const THREAD: &str = "thr_1";
const ID: &str = "dev.test.echo";

#[cfg(unix)]
#[test]
fn git_fetch_respects_proxy_and_ssh_settings() {
    use std::os::unix::fs::PermissionsExt as _;

    if let Some(case) = std::env::var_os("KAGAN_FETCH_CASE") {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("called");
        let source = source::Source::parse(&std::env::var("KAGAN_FETCH_SOURCE").unwrap()).unwrap();
        let (_keep, stop) = async_channel::bounded(1);
        let result = source::fetch(&source, None, dir.path(), dir.path(), &stop);
        if case == "unreachable" {
            assert!(
                matches!(result, Err(OpError::Failed(message)) if message.contains("could not reach 127.0.0.1"))
            );
            assert!(!marker.exists());
        } else {
            assert!(
                matches!(result, Err(OpError::Failed(message)) if message.contains("git failed"))
            );
        }
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let git = dir.path().join("git");
    let log = dir.path().join("git-log");
    std::fs::write(&git, "#!/bin/sh\nif [ \"$1\" = config ]; then\n  case \"$3\" in http.proxy) printf '%s' \"$KAGAN_TEST_PROXY\";; core.sshCommand) printf '%s' \"$KAGAN_TEST_SSH\";; esac\n  exit 0\nfi\nprintf '%s' \"${GIT_SSH_COMMAND-unset}\" > \"$KAGAN_TEST_LOG\"\necho fake >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let https = format!("git:https://127.0.0.1:{port}/repo@main");
    let run = |case: &str, source: &str, vars: &[(&str, &str)], expected: Option<&str>| {
        let _ = std::fs::remove_file(&log);
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "tests::git_fetch_respects_proxy_and_ssh_settings",
                "--nocapture",
            ])
            .env_clear()
            .env("PATH", dir.path())
            .env("HOME", dir.path())
            .env("KAGAN_FETCH_CASE", case)
            .env("KAGAN_FETCH_SOURCE", source)
            .env("KAGAN_TEST_LOG", &log);
        for (key, value) in vars {
            child.env(key, value);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        if let Some(expected) = expected {
            assert_eq!(std::fs::read_to_string(&log).unwrap(), expected);
        } else {
            assert!(!log.exists());
        }
    };
    run("unreachable", &https, &[("HTTPS_PROXY", "")], None);
    run("unreachable", &https, &[("KAGAN_TEST_PROXY", "")], None);
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        run("proxy", &https, &[(key, "proxy")], Some("unset"));
    }
    run(
        "proxy",
        &https,
        &[("KAGAN_TEST_PROXY", "proxy")],
        Some("unset"),
    );
    let ssh = "git:ssh://example.invalid/repo@main";
    run("ssh", ssh, &[], Some("ssh -o ConnectTimeout=15"));
    run("ssh", ssh, &[("GIT_SSH_COMMAND", "custom")], Some("custom"));
    run("ssh", ssh, &[("KAGAN_TEST_SSH", "custom")], Some("unset"));
}

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

fn manifest() -> Value {
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
    std::fs::write(dir.join(manifest::FILE), manifest.to_string()).unwrap();
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
    write_plugin(dir.path(), &manifest());
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

#[test]
fn a_plain_button_in_any_plugin_script_is_a_warning_and_a_kg_button_is_not() {
    let dir = plugin_dir();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    std::fs::write(
        dir.path().join("ui/panel.js"),
        "export const n = new Button ( \"go\" ).label(\"Go\");\n",
    )
    .unwrap();
    assert_eq!(
        check_plugin(dir.path()).unwrap(),
        ["ui/panel.js uses Button, which kg ui cannot find or click; use KgButton"]
    );

    // A comment or a string that names Button constructs nothing.
    std::fs::write(
        dir.path().join("ui/panel.js"),
        "// new Button(\"old\")\nexport const n = \"new Button(x)\";\nexport const k = new KgButton(\"go\");\n",
    )
    .unwrap();
    assert_eq!(check_plugin(dir.path()).unwrap(), Vec::<String>::new());
}

#[test]
fn manifests_reject_unknown_keys_bad_ids_and_newer_requirements() {
    let dir = plugin_dir();
    let rejected = |edit: &dyn Fn(&mut Value), expected: &str| {
        let mut value = manifest();
        edit(&mut value);
        std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
        let error = read_manifest(dir.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    };
    rejected(&|value| value["extra"] = json!(1), "unknown key \"extra\"");
    rejected(
        &|value| value["downloads"] = json!({}),
        "unknown key \"downloads\"",
    );
    rejected(
        &|value| value["powers"]["spawn"] = json!([{ "program": "tool/bin/server" }]),
        "unknown field `spawn`",
    );
    rejected(
        &|value| value["powers"]["serve"] = json!(true),
        "unknown field `serve`",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["downloads"]),
        "unknown variant",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["agents"]),
        "unknown variant `agents`",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["notify"]),
        "unknown variant `notify`",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["hotkeys"]),
        "unknown variant `hotkeys`",
    );
    rejected(
        &|value| value["powers"]["exec"] = json!(true),
        "unknown field `exec`",
    );
    rejected(&|value| value["id"] = json!("echo"), "reverse-DNS");
    rejected(
        &|value| value["kagan"] = json!(">=9.0.0"),
        "needs kagan >=9.0.0",
    );
    rejected(&|value| value["entry"] = json!("../main.js"), "entry");
    rejected(
        &|value| value["tools"][0]["input"] = json!("/etc/passwd"),
        "inside the plugin",
    );
    rejected(
        &|value| value["tools"][1]["name"] = json!("echo"),
        "declared twice",
    );
    rejected(
        &|value| value["events"] = json!(["idle"]),
        "unknown event \"idle\"",
    );
    rejected(
        &|value| value["events"] = json!(["thread.nope"]),
        "unknown event \"thread.nope\"",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["audio"]),
        "unknown variant",
    );
    let mut value = manifest();
    value["powers"]["areas"] = json!(["threads"]);
    value["events"] = json!(["thread.idle", "thread.turn.ended"]);
    std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
    let read = read_manifest(dir.path()).unwrap();
    assert!(read.events.contains("thread.idle"));
    assert!(read.events.contains("thread.turn.ended"));
}

#[test]
fn settings_keep_the_order_written_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("main.js"),
        "export default class Plugin {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join(manifest::FILE),
        r#"{
            "id": "dev.example.order",
            "name": "Order",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "settings": {
                "zeta": { "type": "boolean", "title": "Zeta" },
                "alpha": { "type": "string", "title": "Alpha" },
                "mid": { "type": "select", "title": "Mid", "options": ["b", "a"], "default": "b" }
            }
        }"#,
    )
    .unwrap();
    let read = read_manifest(dir.path()).unwrap();
    let keys: Vec<_> = read.settings.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["zeta", "alpha", "mid"]);
}

#[test]
fn a_read_entry_must_be_a_folder_under_home() {
    let dir = plugin_dir();
    let rejected = |entry: &str, expected: &str| {
        let mut value = manifest();
        value["powers"]["read"] = json!([entry]);
        std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
        let error = read_manifest(dir.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "{entry}: {error}");
    };
    rejected("/tmp/notes", "written as ~/");
    rejected("~/../etc", "stay inside");
    rejected("~/.claude/../../etc", "stay inside");
    rejected("~/", "stay inside");
    rejected("~\\..", "written as ~/");
    rejected("C:\\x", "written as ~/");
    rejected("\\\\host\\share", "written as ~/");

    let home = home();
    let stamp = unique("file");
    let file = home.join(&stamp);
    std::fs::write(&file, "x").unwrap();
    rejected(&format!("~/{stamp}"), "must be a folder");
    std::fs::remove_file(&file).unwrap();

    let missing = format!("~/{}", unique("missing"));
    let mut value = manifest();
    value["powers"]["read"] = json!([missing]);
    std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
    read_manifest(dir.path()).unwrap();

    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        let name = unique("link");
        let link = home.join(&name);
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        rejected(&format!("~/{name}"), "outside");
        std::fs::remove_file(link).unwrap();
    }
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

fn read_manifest(root: &Path) -> Result<manifest::Manifest, OpError> {
    manifest::read(root, spare_data())
}

/// The catalog the plugin crate's own tests declare against: the gpui-kit components.
fn catalog() -> gpui_shell::FrozenComponentRegistry {
    gpui_shell::components::components().unwrap()
}

fn check_plugin(root: &Path) -> crate::Result<Vec<String>> {
    crate::development::check(root, spare_data())
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

#[test]
fn page_store_is_an_install_line_and_needs_approval_again() {
    let plugin = plugin_dir();
    let html = plugin.path().join("ui");
    std::fs::create_dir_all(&html).unwrap();
    std::fs::write(html.join("index.html"), "<p>hi</p>").unwrap();
    let base = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !base
            .lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later")
    );
    let mut value = manifest();
    value["ui"]["panels"] = json!([{
        "id": "board",
        "title": "Board",
        "web": "ui/index.html",
        "placement": "right",
        "store": true
    }]);
    write_plugin(plugin.path(), &value);
    let with = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        with.lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later"),
        "{:?}",
        with.lines()
    );
    assert!(base.within(&with));
    assert!(!with.within(&base));
}

#[test]
fn open_url_is_an_install_line_and_needs_approval_again() {
    let plugin = plugin_dir();
    let base = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !base
            .lines()
            .iter()
            .any(|line| line == "opens links in your browser")
    );
    let mut value = manifest();
    value["powers"]["openUrl"] = json!(true);
    write_plugin(plugin.path(), &value);
    let with = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        with.lines()
            .iter()
            .any(|line| line == "opens links in your browser"),
        "{:?}",
        with.lines()
    );
    assert!(base.within(&with));
    assert!(!with.within(&base));
}

#[test]
fn webview_store_true_in_the_script_asks_for_page_data() {
    let plugin = plugin_dir();
    std::fs::write(
        plugin.path().join("main.js"),
        r#"
import { View } from "gpui-kit";
import { WebView } from "gpui-component";
export default class Plugin extends View {
  render() {
    return new WebView("board").url("http://127.0.0.1/").store(true);
  }
}
"#,
    )
    .unwrap();
    let grant = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        grant
            .lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later"),
        "{:?}",
        grant.lines()
    );
}

#[test]
fn store_true_on_something_other_than_a_webview_asks_for_nothing() {
    let plugin = plugin_dir();
    std::fs::write(
        plugin.path().join("main.js"),
        r#"
import { View } from "gpui-kit";
export default class Plugin extends View {
  render() {
    const cache = new Map();
    cache.store(true);
    return null;
  }
}
"#,
    )
    .unwrap();
    let grant = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !grant
            .lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later"),
        "{:?}",
        grant.lines()
    );
}

#[test]
fn a_grant_grows_only_when_it_asks_for_more() {
    let dir = plugin_dir();
    let grant = |edit: &dyn Fn(&mut Value)| {
        let mut value = manifest();
        edit(&mut value);
        std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
        Grant::of(&read_manifest(dir.path()).unwrap(), dir.path())
    };
    let base = grant(&|value| {
        value["powers"]["fetch"] =
            json!([{ "host": "api.github.com", "methods": ["GET", "POST"] }]);
    });
    let fewer = grant(&|value| {
        value["powers"]["fetch"] = json!([{ "host": "api.github.com", "methods": ["GET"] }]);
        value["powers"]["clipboard"] = json!({});
    });
    let more = grant(&|value| {
        value["powers"]["fetch"] =
            json!([{ "host": "api.github.com", "methods": ["GET", "POST"] }]);
        value["powers"]["run"] = json!([{ "program": "gh" }]);
    });
    assert!(fewer.within(&base));
    assert!(!more.within(&base));
    assert_eq!(
        more.lines(),
        vec![
            "runs gh",
            "talks to api.github.com (GET, POST)",
            "writes to your clipboard",
            "reads a folder you choose (Notes)",
        ]
    );
}

#[test]
fn local_fetch_grants_http_on_that_port_and_install_line() {
    use gpui_shell::Capabilities;

    use crate::manifest::{Fetch, Method};

    let dir = plugin_dir();
    let mut value = manifest();
    value["powers"]["fetch"] = json!([
        { "host": "http://127.0.0.1:3847", "methods": ["GET"] },
        { "host": "http://localhost:3847", "methods": ["POST"] },
        { "host": "api.github.com", "methods": ["GET"] }
    ]);
    std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
    let grant = Grant::of(&read_manifest(dir.path()).unwrap(), dir.path());
    assert_eq!(
        grant.lines(),
        vec![
            "talks to programs on this computer (port 3847)",
            "talks to api.github.com (GET)",
            "writes to your clipboard",
            "reads a folder you choose (Notes)",
        ]
    );

    let caps = Capabilities::new().http_requests(
        [
            Fetch {
                host: "http://127.0.0.1:3847".into(),
                methods: [Method::Get].into_iter().collect(),
            },
            Fetch {
                host: "api.github.com".into(),
                methods: [Method::Get].into_iter().collect(),
            },
        ]
        .iter()
        .map(|fetch| fetch.http_grant().unwrap()),
    );
    assert!(caps.may_request("http", "127.0.0.1", Some(3847), "GET", "/"));
    assert!(!caps.may_request("http", "127.0.0.1", Some(3848), "GET", "/"));
    assert!(!caps.may_request("http", "127.0.0.1", Some(3847), "POST", "/"));
    assert!(!caps.may_request("https", "127.0.0.1", Some(3847), "GET", "/"));
    assert!(caps.may_request("https", "api.github.com", None, "GET", "/"));
    assert!(!caps.may_request("http", "api.github.com", None, "GET", "/"));
}

#[test]
fn thread_events_need_the_threads_area() {
    let dir = plugin_dir();
    let mut value = manifest();
    value["events"] = json!(["thread.message"]);
    std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
    let error = read_manifest(dir.path()).unwrap_err().to_string();
    assert!(error.contains("threads area"), "{error}");
}

#[test]
fn a_shown_source_has_no_credentials() {
    assert_eq!(
        crate::index::shown("git:https://user:token@example.com/acme/plugin@main"),
        "git:https://example.com/acme/plugin@main"
    );
    assert_eq!(
        crate::index::shown("git:ssh://git@example.com/acme/plugin@main"),
        "git:ssh://git@example.com/acme/plugin@main"
    );
    assert_eq!(crate::index::shown("/local/plugin"), "/local/plugin");
}

#[test]
fn local_fetch_hosts_are_checked() {
    let dir = plugin_dir();
    let rejected = |host: &str, expected: &str| {
        let mut value = manifest();
        value["powers"]["fetch"] = json!([{ "host": host, "methods": ["GET"] }]);
        std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
        let error = read_manifest(dir.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "host {host:?}: {error}");
    };
    rejected("127.0.0.1", "http://127.0.0.1:<port>");
    rejected("localhost", "http://localhost:<port>");
    rejected("::1", "IPv6 is unsupported");
    rejected("[::1]", "IPv6 is unsupported");
    rejected("::ffff:127.0.0.1", "IPv6 is unsupported");
    rejected("[::ffff:127.0.0.1]", "IPv6 is unsupported");
    rejected("[::ffff:7f00:1]", "IPv6 is unsupported");
    rejected("0.0.0.0", "IPv6 is unsupported");
    rejected("[::]", "IPv6 is unsupported");
    rejected("[::ffff:0:0]", "IPv6 is unsupported");
    rejected("localhost.", "IPv6 is unsupported");
    rejected("foo.localhost", "IPv6 is unsupported");
    rejected("http://127.0.0.1", "explicit port");
    rejected("http://localhost", "explicit port");
    rejected("http://example.com:80", "only for 127.0.0.1 or localhost");
    rejected(
        "https://127.0.0.1:443",
        "hostname, or http://127.0.0.1:<port>",
    );
    rejected("http://[::1]:3847", "only for 127.0.0.1 or localhost");
    rejected("127.0.0.1:3847", "hostname, or http://127.0.0.1:<port>");
    rejected("example.com:443", "hostname, or http://127.0.0.1:<port>");

    for host in ["http://127.0.0.1:80", "http://localhost:80"] {
        let mut value = manifest();
        value["powers"]["fetch"] = json!([{ "host": host, "methods": ["GET"] }]);
        std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
        let grant = Grant::of(&read_manifest(dir.path()).unwrap(), dir.path());
        assert!(
            grant.lines().iter().any(|line| line.contains("port 80")),
            "{host}: {:?}",
            grant.lines()
        );
    }

    let mut value = manifest();
    value["powers"]["fetch"] = (1..=manifest::LOCAL_FETCH_CAP + 1)
        .map(|port| json!({ "host": format!("http://127.0.0.1:{port}"), "methods": ["GET"] }))
        .collect();
    std::fs::write(dir.path().join(manifest::FILE), value.to_string()).unwrap();
    let error = read_manifest(dir.path()).unwrap_err().to_string();
    assert!(
        error.contains(&format!("at most {}", manifest::LOCAL_FETCH_CAP)),
        "{error}"
    );
}

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
            metadata: Metadata {
                plugin_id: PluginId(ID.into()),
                value: json!({ "master": true }),
            },
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

fn cli_plugin() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut value = manifest();
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

#[gpui_kit::test]
fn settings_are_typed_by_the_manifest(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let config = |op| {
        Op::Plugin(PluginOp::Config {
            id: PluginId(ID.into()),
            op,
            thread_id: None,
        })
    };

    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "sounds".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!(true))
    );
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "retries".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!(3))
    );
    let error = run(
        cx,
        config(ConfigOp::Set {
            key: "sounds".into(),
            value: Some(json!("loud")),
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("takes true or false"), "{error}");
    let number_error = run(
        cx,
        config(ConfigOp::Set {
            key: "retries".into(),
            value: Some(json!("three")),
        }),
    )
    .unwrap_err();
    assert!(
        number_error
            .to_string()
            .contains("takes a number, like 3 or 2.5"),
        "{number_error}"
    );
    assert!(matches!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "missing".into()
            })
        ),
        Err(OpError::Invalid(_))
    ));
    run(
        cx,
        config(ConfigOp::Set {
            key: "sounds".into(),
            value: Some(json!(false)),
        }),
    )
    .unwrap();
    run(
        cx,
        config(ConfigOp::Set {
            key: "retries".into(),
            value: Some(json!(2.5)),
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "retries".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!(2.5))
    );
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi false"),
            is_error: false
        }
    );
}

#[gpui_kit::test]
fn a_removed_project_keeps_its_stored_id(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    let config = |op| {
        Op::Plugin(PluginOp::Config {
            id: id.clone(),
            op,
            thread_id: None,
        })
    };
    run(
        cx,
        config(ConfigOp::Set {
            key: "board".into(),
            value: Some(json!("prj_test")),
        }),
    )
    .unwrap();
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "board", &json!("prj_gone"))
            .unwrap();
    });
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: id.clone(),
            thread_id: None,
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!("prj_gone"))
    );
}

#[gpui_kit::test]
fn a_stored_project_id_loads_only_as_text_or_null(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    let get = Op::Plugin(PluginOp::Config {
        id: id.clone(),
        op: ConfigOp::Get {
            key: "board".into(),
        },
        thread_id: None,
    });
    let reload = Op::Plugin(PluginOp::Reload {
        id: id.clone(),
        thread_id: None,
    });
    for old in [json!(true), json!(3), json!({ "id": "prj_kagan" })] {
        cx.update(|cx| {
            let data = Workspace::global(cx).read(cx).data_dir().to_owned();
            let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
            store.set_plugin_value(&id, "board", &old).unwrap();
        });
        run(cx, reload.clone()).unwrap();
        assert_eq!(
            run(cx, get.clone()).unwrap(),
            OpResult::Setting(Value::Null)
        );
    }
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "board", &json!("prj_gone"))
            .unwrap();
    });
    run(cx, reload).unwrap();
    assert_eq!(run(cx, get).unwrap(), OpResult::Setting(json!("prj_gone")));
}

#[gpui_kit::test]
fn a_stored_number_coerces_or_falls_back_on_load(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    let get = Op::Plugin(PluginOp::Config {
        id: id.clone(),
        op: ConfigOp::Get {
            key: "retries".into(),
        },
        thread_id: None,
    });
    let reload = Op::Plugin(PluginOp::Reload {
        id: id.clone(),
        thread_id: None,
    });
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "retries", &json!("30"))
            .unwrap();
    });
    run(cx, reload.clone()).unwrap();
    assert_eq!(run(cx, get.clone()).unwrap(), OpResult::Setting(json!(30)));
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "retries", &json!(true))
            .unwrap();
    });
    run(cx, reload).unwrap();
    assert_eq!(run(cx, get).unwrap(), OpResult::Setting(json!(3)));
}

#[gpui_kit::test]
fn an_install_or_removal_whose_command_stopped_never_acts(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let source = plugin.path().display().to_string();
    open(cx, &[]);
    let approvals =
        |cx: &mut TestAppContext| cx.read(|cx| Workspace::global(cx).read(cx).approvals().count());
    let install_for = |cx: &mut TestAppContext, gone| {
        let op = Op::Plugin(install(source.clone(), None, None));
        let task =
            cx.update(|cx| Workspace::dispatch_for(op, Some(kg_core::Requester::new(gone)), cx));
        cx.run_until_parked();
        task
    };

    let (present, gone) = async_channel::bounded::<()>(1);
    let stopped = install_for(cx, gone);
    assert_eq!(approvals(cx), 1);
    drop(present);
    cx.run_until_parked();
    assert_eq!(
        approvals(cx),
        0,
        "the card stayed after its command stopped"
    );
    assert!(cx.foreground_executor().block_test(stopped).is_err());
    assert!(records(cx).is_empty());

    let (_present, gone) = async_channel::bounded::<()>(1);
    let approved = install_for(cx, gone);
    answer_standalone(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(approved).unwrap(),
        OpResult::Done
    );
    assert_eq!(records(cx).len(), 1);

    let (present, gone) = async_channel::bounded::<()>(1);
    let removing = cx.update(|cx| {
        Workspace::dispatch_for(
            Op::Plugin(PluginOp::Remove {
                id: PluginId(ID.into()),
                thread_id: None,
            }),
            Some(kg_core::Requester::new(gone)),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(approvals(cx), 1);
    drop(present);
    cx.run_until_parked();
    assert_eq!(
        approvals(cx),
        0,
        "the card stayed after its command stopped"
    );
    assert!(cx.foreground_executor().block_test(removing).is_err());
    assert_eq!(records(cx).len(), 1);
}

#[gpui_kit::test]
fn installing_needs_a_person_with_or_without_a_thread(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let source = plugin.path().display().to_string();
    open(cx, &[]);

    let standalone = plugin_op(cx, install(source.clone(), None, None));
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .unwrap()
    });
    assert!(approval.title.contains("writes to your clipboard"));
    let invalid =
        cx.update(|cx| Workspace::answer_standalone(approval.id.clone(), "invented".into(), cx));
    assert!(cx.foreground_executor().block_test(invalid).is_err());
    assert_eq!(
        cx.read(|cx| Workspace::global(cx).read(cx).approvals().count()),
        1
    );
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(standalone).is_err());
    assert!(records(cx).is_empty());

    let declined = plugin_op(cx, install(source.clone(), Some(thread()), None));
    let title = answer(cx, "decline");
    assert_eq!(
        title,
        format!(
            "Install Echo 1.0.0 ({ID})?\nFrom the folder {}.\nCaution: a folder on this computer is not pinned to a commit.\nIt:\n- writes to your clipboard\n- reads a folder you choose (Notes)",
            dunce::canonicalize(plugin.path()).unwrap().display()
        )
    );
    assert!(cx.foreground_executor().block_test(declined).is_err());
    assert!(records(cx).is_empty());
    assert_eq!(tool_names(cx).1, ["skill_load"]);

    let approved = plugin_op(cx, install(source, Some(thread()), None));
    answer(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(approved).unwrap(),
        OpResult::Done
    );
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(record.root, dunce::canonicalize(plugin.path()).unwrap());
    assert!(record.enabled && record.commit.is_none());
    assert_eq!(
        tool_names(cx).1,
        ["echo", "broken", "hang", "peek", "skill_load"]
    );
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

#[gpui_kit::test]
fn git_installs_lock_to_a_commit_and_updates_ask_only_for_new_powers(cx: &mut TestAppContext) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--initial-branch=main"]);
    write_plugin(&repo.path().join("echo"), &manifest());
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "--quiet", "-m", "one"]);
    let first = git(repo.path(), &["rev-parse", "HEAD"]);
    let source = format!("git:{}@main", repo.path().display());
    open(cx, &[]);

    let installing = plugin_op(cx, install(source.clone(), Some(thread()), Some("echo")));
    answer(cx, "approve");
    cx.foreground_executor().block_test(installing).unwrap();
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(record.commit.as_deref(), Some(first.as_str()));
    assert!(record.root.ends_with(Path::new(&first).join("echo")));
    assert_eq!(record.source, source);
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );

    let update = |thread_id| PluginOp::Update {
        id: PluginId(ID.into()),
        thread_id,
    };
    assert_eq!(
        run(cx, Op::Plugin(update(None))).unwrap(),
        OpResult::Done,
        "the same commit needs no approval"
    );

    let mut grown = manifest();
    grown["version"] = json!("1.1.0");
    grown["powers"]["run"] = json!([{ "program": "gh" }]);
    write_plugin(&repo.path().join("echo"), &grown);
    git(repo.path(), &["commit", "--quiet", "-am", "two"]);
    let second = git(repo.path(), &["rev-parse", "HEAD"]);

    let declined = plugin_op(cx, update(None));
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());
    assert_eq!(records(cx)[0].commit.as_deref(), Some(first.as_str()));

    let updating = plugin_op(cx, update(Some(thread())));
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Update Echo to 1.1.0 ({ID})?\nFrom {} at main, commit {second}.\nCaution: this repository is on this computer, not where an index you added lives.\nIt:\n- runs gh",
            repo.path().display()
        )
    );
    cx.foreground_executor().block_test(updating).unwrap();
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(record.commit.as_deref(), Some(second.as_str()));
    assert!(!loaded(cx), "an update drops the old load");
}

#[test]
fn git_sources_need_a_ref() {
    for text in ["git:git@github.com:o/r", "git:https://example.com/r"] {
        let Err(error) = source::Source::parse(text) else {
            panic!("{text} parsed")
        };
        assert!(error.to_string().contains("needs a ref @<ref>"), "{error}");
    }
    assert!(source::Source::parse("git:git@github.com:o/r@v1").is_ok());
}

#[gpui_kit::test]
fn powers_that_grow_on_disk_are_not_loaded_until_approved(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = || PluginId(ID.into());

    let mut grown = manifest();
    grown["powers"]["clipboard"]["read"] = json!(true);
    grown["powers"]["fetch"] = json!([{ "host": "api.github.com", "methods": ["POST"] }]);
    write_plugin(plugin.path(), &grown);
    run(cx, Op::Plugin(PluginOp::Disable { id: id() })).unwrap();
    let error = run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("needs approval for: talks to api.github.com (POST), reads your clipboard"),
        "{error}"
    );
    assert_eq!(tool_names(cx).1, ["skill_load"]);
    assert!(matches!(
        call(cx, "echo", json!({ "text": "hi" })),
        Err(OpError::NotFound(_))
    ));

    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: id(),
            thread_id: Some(thread()),
        },
    );
    let root = records(cx)[0].root.clone();
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Update Echo to 1.0.0 ({ID})?\nFrom the folder {}.\nCaution: a folder on this computer is not pinned to a commit.\nIt:\n- talks to api.github.com (POST)\n- reads your clipboard",
            root.display()
        )
    );
    cx.foreground_executor().block_test(updating).unwrap();
    run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap();
    assert_eq!(
        tool_names(cx).1,
        ["echo", "broken", "hang", "peek", "skill_load"]
    );
}

#[gpui_kit::test]
fn a_restart_does_not_load_powers_nobody_approved(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut record = installed(plugin.path());
    record.approved = serde_json::to_value(Grant::default()).unwrap();
    open(cx, &[record]);
    assert_eq!(tool_names(cx).1, ["skill_load"]);
    assert!(!loaded(cx));
}

#[gpui_kit::test]
fn a_folder_setting_is_approved_by_a_person(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    open(cx, &[installed(plugin.path())]);
    let set = |thread_id| PluginOp::Config {
        id: PluginId(ID.into()),
        op: ConfigOp::Set {
            key: "notes".into(),
            value: Some(json!(notes.path().display().to_string())),
        },
        thread_id,
    };

    let declined = plugin_op(cx, set(None));
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());

    let setting = plugin_op(cx, set(Some(thread())));
    let path = dunce::canonicalize(notes.path()).unwrap();
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Let Echo ({ID}) read a folder?\nIt:\n- reads {} (Notes)",
            path.display()
        )
    );
    cx.foreground_executor().block_test(setting).unwrap();
    assert_eq!(
        run(
            cx,
            Op::Plugin(PluginOp::Config {
                id: PluginId(ID.into()),
                op: ConfigOp::Get {
                    key: "notes".into()
                },
                thread_id: None,
            })
        )
        .unwrap(),
        OpResult::Setting(json!(path.display().to_string()))
    );
}

#[gpui_kit::test]
fn a_folder_that_becomes_text_loses_its_read_root(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let folders = ["approved", "typed", "default"].map(|name| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret.txt"), name).unwrap();
        dir
    });
    let path = |ix: usize| {
        dunce::canonicalize(folders[ix].path())
            .unwrap()
            .display()
            .to_string()
    };
    // The file read runs off the test executor, so wait on real time instead of the
    // test clock, which would fire the tool timeout first.
    let peek = |cx: &mut TestAppContext, folder: &str| {
        let input = json!({ "path": Path::new(folder).join("secret.txt") });
        let task = cx.update(|cx| {
            Workspace::dispatch(
                Op::Thread(ThreadOp::CallTool {
                    id: thread(),
                    name: "peek".into(),
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
        panic!("peek did not answer")
    };
    let config = |value: String, thread_id| PluginOp::Config {
        id: PluginId(ID.into()),
        op: ConfigOp::Set {
            key: "notes".into(),
            value: Some(json!(value)),
        },
        thread_id,
    };
    open(cx, &[installed(plugin.path())]);

    let setting = plugin_op(cx, config(path(0), Some(thread())));
    answer(cx, "approve");
    cx.foreground_executor().block_test(setting).unwrap();
    assert_eq!(
        peek(cx, &path(0)),
        OpResult::ToolReply {
            text: "approved".into(),
            is_error: false
        }
    );

    let mut text = manifest();
    text["settings"]["notes"] = json!({ "type": "string", "title": "Notes", "default": path(2) });
    write_plugin(plugin.path(), &text);
    let id = || PluginId(ID.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: id() })).unwrap();
    run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap();
    run(cx, Op::Plugin(config(path(1), None))).unwrap();
    for folder in [path(0), path(1), path(2)] {
        let OpResult::ToolReply { is_error, .. } = peek(cx, &folder) else {
            panic!("tool reply")
        };
        assert!(is_error, "{folder} must not be readable");
    }
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
    let mut value = manifest();
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

#[gpui_kit::test]
fn a_declared_folder_is_readable_in_ranges_and_a_sibling_is_not(cx: &mut TestAppContext) {
    let name = unique("read");
    let root = home().join(&name);
    let allowed = root.join("allowed");
    let sibling = root.join("sibling");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    let _sweep = Sweep(root);
    std::fs::write(allowed.join("note.txt"), "hello-allowed").unwrap();
    std::fs::write(sibling.join("note.txt"), "secret-sibling").unwrap();
    let big = allowed.join("big.txt");
    std::fs::write(
        &big,
        "A".repeat(1_000) + &"B".repeat(1_000) + &"C".repeat(1_000),
    )
    .unwrap();

    let plugin = plugin_dir();
    files_plugin(plugin.path(), &format!("~/{name}/allowed"));
    open(cx, &[installed(plugin.path())]);

    let OpResult::ToolReply { text, is_error } =
        slice(cx, json!({ "path": big, "offset": 0, "maxBytes": 1000 }))
    else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["text"], "A".repeat(1_000));
    assert_eq!(value["next"], 1000);
    assert_eq!(value["size"], 3000);

    let OpResult::ToolReply { text, is_error } =
        slice(cx, json!({ "path": big, "offset": 1000, "maxBytes": 1000 }))
    else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["text"], "B".repeat(1_000));
    assert_eq!(value["next"], 2000);

    let OpResult::ToolReply { text, is_error } =
        slice(cx, json!({ "list": true, "path": &sibling }))
    else {
        panic!("tool reply")
    };
    assert!(is_error, "{text}");
    assert!(text.contains("not a granted folder"), "{text}");
}

struct Sweep(std::path::PathBuf);
impl Drop for Sweep {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn refused_read(cx: &mut TestAppContext, entry: &str) -> String {
    let plugin = plugin_dir();
    let mut value = manifest();
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

#[gpui_kit::test]
fn declaring_the_data_dir_a_folder_inside_it_or_ssh_is_refused(cx: &mut TestAppContext) {
    let data = tempfile::Builder::new()
        .prefix(".ms89-")
        .tempdir_in(home())
        .unwrap();
    let name = data
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    std::fs::create_dir_all(data.path().join("notes")).unwrap();
    let shown = dunce::canonicalize(data.path()).unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let data_entry = format!("~/{name}");
    let error = refused_read(cx, &data_entry);
    assert!(
        error.contains(&format!(
            "read path \"{data_entry}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );

    let inside = format!("~/{name}/notes");
    let error = refused_read(cx, &inside);
    assert!(
        error.contains(&format!(
            "read path \"{inside}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );

    let error = refused_read(cx, "~/.ssh");
    assert!(
        error.contains("read path \"~/.ssh\" cannot cover ~/.ssh"),
        "{error}"
    );
}

#[gpui_kit::test]
fn a_declared_folder_that_contains_a_data_dir_outside_home_is_refused(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let name = unique("cover");
    let link = home().join(&name);
    let _unlink = Unlink(link.clone());
    point_at(&link, parent.path());
    let shown = dunce::canonicalize(&data).unwrap();
    open_kept(cx, &data, parent, &[]);
    let entry = format!("~/{name}");
    let error = refused_read(cx, &entry);
    assert!(
        error.contains(&format!(
            "read path \"{entry}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );
}

#[gpui_kit::test]
fn check_refuses_a_read_of_the_data_dir(cx: &mut TestAppContext) {
    let data = tempfile::Builder::new()
        .prefix(".ms89-")
        .tempdir_in(home())
        .unwrap();
    let name = data
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let shown = dunce::canonicalize(data.path()).unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let plugin = plugin_dir();
    let clean = plugin_op(
        cx,
        PluginOp::Check {
            path: plugin.path().to_path_buf(),
        },
    );
    assert_eq!(
        cx.foreground_executor().block_test(clean).unwrap(),
        OpResult::Done
    );

    let entry = format!("~/{name}");
    let mut value = manifest();
    value["powers"]["read"] = json!([entry]);
    write_plugin(plugin.path(), &value);
    let checking = plugin_op(
        cx,
        PluginOp::Check {
            path: plugin.path().to_path_buf(),
        },
    );
    let error = cx
        .foreground_executor()
        .block_test(checking)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&format!(
            "read path \"{entry}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );
}

#[gpui_kit::test]
fn an_update_that_adds_a_read_folder_asks_again(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let folder = format!("~/{}", unique("added"));
    let mut grown = manifest();
    grown["powers"]["read"] = json!([folder]);
    write_plugin(plugin.path(), &grown);
    let id = || PluginId(ID.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: id() })).unwrap();
    let error = run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap_err();
    assert!(
        error.to_string().contains(&format!("reads {folder}")),
        "{error}"
    );
    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: id(),
            thread_id: Some(thread()),
        },
    );
    let root = records(cx)[0].root.clone();
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Update Echo to 1.0.0 ({ID})?\nFrom the folder {}.\nCaution: a folder on this computer is not pinned to a commit.\nIt:\n- reads {folder}",
            root.display()
        )
    );
    cx.foreground_executor().block_test(updating).unwrap();
}

#[gpui_kit::test]
fn disabling_while_a_folder_waits_for_approval_keeps_it_disabled(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    open(cx, &[installed(plugin.path())]);

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
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    answer(cx, "approve");
    let error = cx.foreground_executor().block_test(setting).unwrap_err();
    assert!(
        error.to_string().contains("changed while this was saving"),
        "{error}"
    );

    let [record] = records(cx).try_into().unwrap();
    assert!(
        !record.enabled,
        "the approval must not re-enable the plugin"
    );
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert!(
        !approved.paths.contains_key("notes"),
        "a disabled plugin does not take the folder that was still saving"
    );
    assert_eq!(stored_value(cx, "notes"), None);
}

#[gpui_kit::test]
fn a_save_held_across_reload_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
    let release = hold_writes(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "sounds".into(),
                value: Some(json!(false)),
            },
            thread_id: None,
        },
    );
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(reloading).unwrap();
    assert_ne!(activation_of(cx), before);
    let error = cx.foreground_executor().block_test(setting).unwrap_err();
    assert!(
        error.to_string().contains("changed while this was saving"),
        "{error}"
    );
    assert_eq!(
        run(
            cx,
            Op::Plugin(PluginOp::Config {
                id: PluginId(ID.into()),
                op: ConfigOp::Get {
                    key: "sounds".into(),
                },
                thread_id: None,
            }),
        )
        .unwrap(),
        OpResult::Setting(json!(true))
    );
    assert_eq!(stored_value(cx, "sounds"), None);
}

#[gpui_kit::test]
fn a_save_held_across_disable_and_enable_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let release = hold_writes(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "sounds".into(),
                value: Some(json!(false)),
            },
            thread_id: None,
        },
    );
    let disabling = plugin_op(
        cx,
        PluginOp::Disable {
            id: PluginId(ID.into()),
        },
    );
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(disabling).unwrap();
    cx.foreground_executor().block_test(enabling).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_eq!(stored_value(cx, "sounds"), None);
    assert_eq!(
        run(
            cx,
            Op::Plugin(PluginOp::Config {
                id: PluginId(ID.into()),
                op: ConfigOp::Get {
                    key: "sounds".into(),
                },
                thread_id: None,
            }),
        )
        .unwrap(),
        OpResult::Setting(json!(true))
    );
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

#[gpui_kit::test]
fn a_folder_save_queued_across_reload_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, setting, _notes) = queue_folder(cx);
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(reloading).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_folder_did_not_land(cx);
}

#[gpui_kit::test]
fn a_folder_save_queued_across_update_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, setting, _notes) = queue_folder(cx);
    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(updating).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_folder_did_not_land(cx);
}

#[gpui_kit::test]
fn a_folder_save_queued_across_disable_and_enable_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, setting, _notes) = queue_folder(cx);
    let disabling = plugin_op(
        cx,
        PluginOp::Disable {
            id: PluginId(ID.into()),
        },
    );
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(disabling).unwrap();
    cx.foreground_executor().block_test(enabling).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_folder_did_not_land(cx);
    let [record] = records(cx).try_into().unwrap();
    assert!(record.enabled);
}

#[gpui_kit::test]
fn a_folder_save_on_the_new_activation_survives_enable(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut installed = installed(plugin.path());
    installed.enabled = false;
    open(cx, &[installed]);
    let release = hold_activation(cx);
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    let (_notes, path) = save_notes_now(cx);
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(enabling).unwrap();
    assert_folder_landed(cx, &path);
    let [record] = records(cx).try_into().unwrap();
    assert!(record.enabled);
}

#[gpui_kit::test]
fn a_folder_save_on_the_new_activation_survives_reload(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
    let release = hold_activation(cx);
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    assert_ne!(activation_of(cx), before);
    let (_notes, path) = save_notes_now(cx);
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(reloading).unwrap();
    assert_folder_landed(cx, &path);
}

#[gpui_kit::test]
fn a_folder_save_on_the_new_activation_survives_disable(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let release = hold_lifecycle(cx);
    let disabling = plugin_op(
        cx,
        PluginOp::Disable {
            id: PluginId(ID.into()),
        },
    );
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    cx.foreground_executor().block_test(enabling).unwrap();
    let (_notes, path) = save_notes_now(cx);
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(disabling).unwrap();
    assert_folder_landed(cx, &path);
}

#[gpui_kit::test]
fn a_folder_save_held_across_remove_and_reinstall_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    let path = dunce::canonicalize(notes.path()).unwrap();
    open(cx, &[installed(plugin.path())]);
    let release = hold_commits(cx);
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
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: Some(thread()),
        },
    );
    answer(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    release.try_send(()).unwrap();
    let installing = plugin_op(
        cx,
        install(plugin.path().display().to_string(), Some(thread()), None),
    );
    answer(cx, "approve");
    cx.foreground_executor().block_test(installing).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    let [record] = records(cx).try_into().unwrap();
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert_ne!(approved.paths.get("notes"), Some(&path));
    assert_eq!(stored_value(cx, "notes"), None);
}

#[gpui_kit::test]
fn a_folder_save_keeps_the_activation_and_a_decline_stores_nothing(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
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
    assert_eq!(activation_of(cx), before);
    assert!(stored_value(cx, "notes").is_some());

    let again = plugin_op(
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
    answer(cx, "decline");
    assert!(cx.foreground_executor().block_test(again).is_err());
    assert_eq!(activation_of(cx), before);
}

#[gpui_kit::test]
fn a_folder_approved_while_an_update_waits_is_kept(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    let path = dunce::canonicalize(notes.path()).unwrap();
    open(cx, &[installed(plugin.path())]);

    let mut grown = manifest();
    grown["powers"]["run"] = json!([{ "program": "gh" }]);
    write_plugin(plugin.path(), &grown);
    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: PluginId(ID.into()),
            thread_id: Some(thread()),
        },
    );
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
    let Ok(OpResult::Interactions(pending)) =
        run(cx, Op::Thread(ThreadOp::Interactions { id: thread() }))
    else {
        panic!("interactions")
    };
    let folder = pending
        .iter()
        .find(|interaction| interaction.title.contains("read a folder"))
        .unwrap();
    let update = pending
        .iter()
        .find(|interaction| interaction.title.starts_with("Update"))
        .unwrap();
    for interaction in [folder, update] {
        let answered = cx.update(|cx| {
            Workspace::answer(&thread(), interaction.id.clone(), "approve".into(), cx)
        });
        cx.run_until_parked();
        cx.foreground_executor().block_test(answered).unwrap();
    }
    cx.foreground_executor().block_test(setting).unwrap();
    cx.foreground_executor().block_test(updating).unwrap();

    let [record] = records(cx).try_into().unwrap();
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert_eq!(approved.powers.run.len(), 1, "the update committed");
    assert_eq!(approved.paths.get("notes"), Some(&path));
}

#[gpui_kit::test]
fn native_modules_exist_only_when_their_areas_are_granted(cx: &mut TestAppContext) {
    for area in ["process", "threads"] {
        let plugin = plugin_dir();
        let import = format!("import * as native from 'kagan/{area}';\n");
        std::fs::write(plugin.path().join("main.js"), format!("{import}{MAIN}")).unwrap();
        open(cx, &[installed(plugin.path())]);
        let error = call(cx, "echo", json!({ "text": "hi" })).unwrap_err();
        assert!(
            error.to_string().contains(&format!("kagan/{area}")),
            "{error}"
        );
        assert!(!loaded(cx));

        let mut declared = manifest();
        declared["powers"]["areas"] = json!([area]);
        std::fs::write(plugin.path().join(manifest::FILE), declared.to_string()).unwrap();
        open(cx, &[installed(plugin.path())]);
        assert!(matches!(
            call(cx, "echo", json!({ "text": "hi" })),
            Ok(OpResult::ToolReply {
                is_error: false,
                ..
            })
        ));
    }
}

#[test]
fn program_environment_grants_can_shrink_but_cannot_grow() {
    use crate::manifest::{Env, Program};

    let program = |env| Program {
        program: "git".into(),
        env,
    };
    let mut approved = Grant::default();
    approved.powers.run = vec![program(Some(Env::Inherit))];
    let mut asked = approved.clone();
    asked.powers.run = vec![program(None)];
    assert!(asked.within(&approved));
    assert!(!approved.within(&asked));
    assert_eq!(approved.intersect(&asked).powers.run, asked.powers.run);
    assert!(
        approved
            .lines()
            .iter()
            .any(|line| line == "runs git with your environment")
    );
}

#[gpui_kit::test]
fn reload_and_dev_wait_for_app_approval_for_added_powers(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let mut changed = manifest();
    changed["powers"]["clipboard"]["read"] = true.into();
    std::fs::write(plugin.path().join(manifest::FILE), changed.to_string()).unwrap();
    let reload = |thread_id| PluginOp::Reload {
        id: PluginId(ID.into()),
        thread_id,
    };
    assert!(run(cx, Op::Plugin(reload(None))).is_err());
    let pending = plugin_op(cx, reload(Some(thread())));
    assert!(answer(cx, "decline").contains("reads your clipboard"));
    assert!(cx.foreground_executor().block_test(pending).is_err());
    let pending = plugin_op(
        cx,
        PluginOp::Dev {
            path: plugin.path().into(),
            thread_id: Some(thread()),
        },
    );
    assert!(answer(cx, "approve").contains("reads your clipboard"));
    assert!(matches!(
        cx.foreground_executor().block_test(pending).unwrap(),
        OpResult::Plugins(_)
    ));
    assert_eq!(run(cx, Op::Plugin(reload(None))).unwrap(), OpResult::Done);
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    assert!(
        plugin.path().join("main.js").exists(),
        "removal keeps the local source"
    );
    assert_eq!(
        run(cx, Op::Plugin(PluginOp::List)).unwrap(),
        OpResult::Plugins(Vec::new())
    );
}

#[gpui_kit::test]
fn removing_a_plugin_waits_for_a_person(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let remove = |thread_id| PluginOp::Remove {
        id: PluginId(ID.into()),
        thread_id,
    };
    let removing = plugin_op(cx, remove(Some(thread())));
    assert_eq!(answer(cx, "decline"), format!("Remove “{ID}”?"));
    let error = cx.foreground_executor().block_test(removing).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("declined: Remove “{ID}”?")),
        "{error}"
    );
    assert_eq!(records(cx).len(), 1);

    let removing = plugin_op(cx, remove(Some(thread())));
    answer(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(removing).unwrap(),
        OpResult::Done
    );
    assert!(records(cx).is_empty());
}

#[gpui_kit::test]
fn removing_a_plugin_defers_and_retries_store_delete_past_the_drop_turn(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;

    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let dropped = Rc::new(Cell::new(false));
    let mark_drop = dropped.clone();
    cx.update(|cx| {
        let host = crate::PluginHost::global(cx);
        host.update(cx, |host, _| {
            host.set_drop_pages(Rc::new(move |_, _, _| {
                mark_drop.set(true);
            }));
        });
    });
    let attempts = Rc::new(Cell::new(0));
    let count = attempts.clone();
    let _hook = crate::web_store::set_remove_hook(move |_| {
        let n = count.get() + 1;
        count.set(n);
        if n < 3 {
            crate::web_store::StoreAttempt::InUse
        } else {
            crate::web_store::StoreAttempt::Removed
        }
    });
    let data = cx.update(|cx| {
        crate::PluginHost::global(cx)
            .read(cx)
            .plugin_data(&PluginId(ID.into()))
    });
    crate::keep_store(&data, "board").expect("store");
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.run_until_parked();
    assert!(dropped.get(), "pages drop in the remove turn");
    assert_eq!(
        attempts.get(),
        0,
        "remove_data_store waits for the native release after drop"
    );
    assert!(
        crate::store_dir(&data, "board").join("store.id").is_file(),
        "store.id stays until WebKit accepts the delete"
    );
    cx.executor().advance_clock(
        crate::web_store::RELEASE_WAIT
            .checked_sub(Duration::from_millis(1))
            .expect("release wait"),
    );
    cx.run_until_parked();
    assert_eq!(attempts.get(), 0, "still inside the release wait");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(attempts.get(), 1, "first attempt after the release wait");
    assert!(
        crate::store_dir(&data, "board").join("store.id").is_file(),
        "InUse keeps store.id for a retry"
    );
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert_eq!(
        cx.foreground_executor().block_test(removing).unwrap(),
        OpResult::Done
    );
    assert_eq!(attempts.get(), 3, "retries InUse until Removed");
    assert!(
        !crate::store_dir(&data, "board").join("store.id").is_file(),
        "store.id clears only after WebKit accepts the delete"
    );
}

#[gpui_kit::test]
fn reload_stops_when_the_plugin_was_removed(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, _| host.keep_hold = Some(hold));
    });
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    release.try_send(()).unwrap();
    assert_eq!(
        cx.foreground_executor()
            .block_test(reloading)
            .unwrap_err()
            .to_string(),
        "plugin was removed"
    );
    assert!(records(cx).is_empty());
}

#[test]
fn check_parses_exports_without_running_plugin_code() {
    let plugin = plugin_dir();
    let mut changed = manifest();
    changed["ui"] = json!({
        "panels": [{"id": "panel", "title": "Panel", "view": "Panel", "placement": "left"}]
    });
    std::fs::write(plugin.path().join(manifest::FILE), changed.to_string()).unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "throw new Error('must not run'); export default class Plugin {} export class Panel {} ",
    )
    .unwrap();
    check_plugin(plugin.path()).unwrap();
    std::fs::write(plugin.path().join("main.js"), "export default class Plugin {} // export class Panel {}\nconst text = 'export class Panel {}';").unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("Panel")
    );
    std::fs::write(plugin.path().join("main.js"), "export default class {").unwrap();
    assert!(check_plugin(plugin.path()).is_err());
}

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
fn declared_skills_validate_paths_and_frontmatter() {
    let plugin = plugin_dir();
    let skill = plugin.path().join("skills/my-task");
    std::fs::create_dir_all(&skill).unwrap();
    let mut changed = manifest();
    changed["skills"] = json!(["skills/my-task"]);
    std::fs::write(plugin.path().join(manifest::FILE), changed.to_string()).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: my-task\ndescription: My task\n---\nInstructions",
    )
    .unwrap();
    check_plugin(plugin.path()).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: wrong-name\ndescription: My task\n---\nInstructions",
    )
    .unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("must match")
    );
    changed["skills"] = json!(["../outside"]);
    std::fs::write(plugin.path().join(manifest::FILE), changed.to_string()).unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("must stay inside")
    );
}

#[test]
fn string_views_require_exports() {
    let plugin = plugin_dir();
    let mut changed = manifest();
    changed["ui"] = json!({"settings": "Missing"});
    std::fs::write(plugin.path().join(manifest::FILE), changed.to_string()).unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class Plugin {}",
    )
    .unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("Missing")
    );
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class Plugin {} export class Missing {}",
    )
    .unwrap();
    check_plugin(plugin.path()).unwrap();
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

fn probe_plugin(areas: &[&str], script: &str) -> TempDir {
    let dir = plugin_dir();
    let mut value = manifest();
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

#[gpui_kit::test]
fn a_plugin_rename_keeps_a_locked_title(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads"], RENAME_PROBE);
    open(cx, &[installed(plugin.path())]);
    let OpResult::Thread(named) = run(
        cx,
        Op::Thread(ThreadOp::Rename {
            id: thread(),
            title: "Named".into(),
        }),
    )
    .unwrap() else {
        panic!("window rename")
    };
    assert!(named.title_locked);
    let OpResult::ToolReply { text, .. } = real_call(cx, "probe").unwrap() else {
        panic!("tool reply")
    };
    assert!(
        text.contains("the title is kept; rename it in the window"),
        "{text}"
    );
    let OpResult::Thread(kept) = run(cx, Op::Thread(ThreadOp::Get { id: thread() })).unwrap()
    else {
        panic!("thread")
    };
    assert_eq!(kept.title, "Named");
}

#[gpui_kit::test]
fn approved_areas_dispatch_core_and_keep_database_transactions(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads", "database"], AREAS_PROBE);
    open(cx, &[installed(plugin.path())]);
    assert_eq!(
        run(
            cx,
            Op::Keymap(KeymapOp::Set {
                action: format!("{ID}:ping"),
                key: "ctrl-alt-o".into(),
                reset: false,
            })
        )
        .unwrap(),
        OpResult::Setting(json!({ "ok": true }))
    );
    let OpResult::ToolReply { text, is_error } = real_call(cx, "probe").unwrap() else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["thread"]["id"], THREAD);
    assert_eq!(value["thread"]["pinned"], true);
    assert_eq!(value["metadata"], json!({ "own": true }));
    assert_eq!(value["sidebar"]["threads"][0]["pinned"], true);
    assert!(value["sidebar"]["threads"][0].get("provider").is_none());
    assert_eq!(value["rows"], json!([{ "id": 1, "text": "kept" }]));

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let commands = cx.update(|cx| crate::PluginHost::global(cx).read(cx).commands());
    assert_eq!(commands[0].key(), Some("ctrl-alt-o"));
}

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
            let refused = "";
            try {
              await threads.answer(id, interaction, "yes");
              refused = "answered";
            } catch (error) {
              refused = String(error);
            }
            return { interaction, refused };
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

#[gpui_kit::test]
fn a_plugin_calls_every_thread_operation(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads"], THREAD_OPS);
    let path = plugin.path().join(manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["settings"]["worker"] = json!({ "type": "provider", "title": "Worker" });
    value["settings"]["spare"] = json!({ "type": "provider", "title": "Spare" });
    std::fs::write(&path, value.to_string()).unwrap();
    open(cx, &[installed(plugin.path())]);
    pick_worker(cx);
    let OpResult::ToolReply { text, is_error } = call_and_approve(cx).unwrap() else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let steps: Vec<Value> = serde_json::from_str(&text).unwrap();
    let step = |name: &str| {
        steps
            .iter()
            .find(|step| step["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    let ok = |name: &str| {
        let step = step(name);
        assert!(step["ok"].as_bool().unwrap(), "{name}: {}", step["error"]);
        step
    };
    let err = |name: &str, needle: &str| {
        let step = step(name);
        let error = step["error"].as_str().unwrap_or("");
        assert!(!step["ok"].as_bool().unwrap(), "{name}: {error}");
        assert!(error.contains(needle), "{name}: {error}");
    };
    assert!(ok("list")["value"].is_array());
    assert!(ok("listFiltered")["value"].is_array());
    let shown = &ok("get")["value"];
    assert_eq!(shown["id"], THREAD);
    hides_agent(shown);
    for listed in ok("list")["value"].as_array().unwrap() {
        hides_agent(listed);
    }
    assert!(ok("events")["value"].is_array());
    assert_eq!(ok("settle")["value"], "exited");
    err("send", "not found");
    err("output", "not found");
    let asked = &ok("ask")["value"];
    assert!(
        asked["interaction"]
            .as_str()
            .unwrap()
            .starts_with("interaction_")
    );
    assert!(
        asked["refused"]
            .as_str()
            .unwrap()
            .contains("a person answers this"),
        "{}",
        asked["refused"]
    );
    err("wait", "unknown event type");
    ok("pin");
    ok("read");
    ok("writeMeta");
    assert_eq!(ok("meta")["value"], json!({ "n": 1 }));
    assert!(
        ok("projects")["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["id"] == "prj_test")
    );
    assert_eq!(ok("project")["value"]["id"], "prj_test");
    err("environment", "environment");
    err("restore", "no environment");
    err("spawn", "was not found");
    err("spawnMeta", "was not found");
    err("spawnAgent", "unknown field `agent`");
    err("spawnProvider", "unknown field `provider`");
    assert!(ok("settings")["value"].get("worker").is_none());
    ok("stop");
    ok("archive");
    ok("unarchive");
    ok("remove");
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

const ANSWER_PLUGIN: &str = r#"
import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as threads from "kagan/threads";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          const value = await threads.answer(job.input.thread, job.input.interaction, job.input.option);
          reply(job.id, { text: JSON.stringify({ ok: true, value }) });
        } catch (error) {
          reply(job.id, { text: JSON.stringify({ ok: false, error: String(error) }) });
        }
      }
    });
  }
  render() { return "answer"; }
}
"#;

fn questions(cx: &mut TestAppContext) -> Vec<(String, String)> {
    let Ok(OpResult::Interactions(rows)) =
        run(cx, Op::Thread(ThreadOp::Interactions { id: thread() }))
    else {
        panic!("interactions")
    };
    rows.into_iter().map(|row| (row.id.0, row.title)).collect()
}

fn answered_events(cx: &mut TestAppContext) -> Vec<(String, String)> {
    let Ok(OpResult::Events(events)) = run(
        cx,
        Op::Thread(ThreadOp::Events {
            id: thread(),
            after_seq: None,
            limit: None,
            order: None,
        }),
    ) else {
        panic!("events")
    };
    events
        .into_iter()
        .filter_map(|event| match event.kind {
            EventKind::Answered { title, option } => Some((title, option)),
            _ => None,
        })
        .collect()
}

fn choice(id: &str, label: &str) -> kg_domain::InteractionOption {
    kg_domain::InteractionOption {
        id: id.into(),
        label: label.into(),
    }
}

fn plugin_refuses(cx: &mut TestAppContext, interaction: &str) {
    let rows = questions(cx);
    let refused = plugin_answer(cx, interaction, "approve");
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("a person answers this"),
        "{refused}"
    );
    assert_eq!(questions(cx), rows);
}

fn person_declines(cx: &mut TestAppContext, interaction: &str) {
    cx.foreground_executor()
        .block_test(cx.update(|cx| {
            Workspace::answer(
                &thread(),
                kg_domain::PendingInteractionId(interaction.to_owned()),
                "decline".into(),
                cx,
            )
        }))
        .unwrap();
}

fn plugin_answer(cx: &mut TestAppContext, interaction: &str, option: &str) -> Value {
    let OpResult::ToolReply { text, is_error } = call(
        cx,
        "probe",
        json!({ "thread": THREAD, "interaction": interaction, "option": option }),
    )
    .unwrap() else {
        panic!("reply")
    };
    assert!(!is_error, "{text}");
    serde_json::from_str(&text).unwrap()
}

#[gpui_kit::test]
fn a_plugin_answers_a_question_and_leaves_person_approvals(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads"], ANSWER_PLUGIN);
    open(cx, &[installed(plugin.path())]);

    let asked =
        cx.update(|cx| Workspace::ask(&thread(), "Ship it".into(), vec![choice("yes", "Yes")], cx));
    cx.run_until_parked();
    let rows = questions(cx);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let replied = plugin_answer(cx, &rows[0].0, "yes");
    assert_eq!(replied["ok"], true, "{replied}");
    assert!(questions(cx).is_empty());
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
    assert_eq!(answered_events(cx), vec![("Ship it".into(), "yes".into())]);

    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: Some(thread()),
        },
    );
    cx.run_until_parked();
    let rows = questions(cx);
    assert!(rows[0].1.starts_with("Remove "), "{rows:?}");
    plugin_refuses(cx, &rows[0].0);
    assert_eq!(answered_events(cx), vec![("Ship it".into(), "yes".into())]);
    person_declines(cx, &rows[0].0);
    assert!(
        cx.foreground_executor()
            .block_test(removing)
            .unwrap_err()
            .to_string()
            .contains("declined")
    );

    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: thread(),
                approval_thread_id: Some(thread()),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let rows = questions(cx);
    assert!(rows[0].1.starts_with("Delete "), "{rows:?}");
    plugin_refuses(cx, &rows[0].0);
    person_declines(cx, &rows[0].0);
    assert!(
        cx.foreground_executor()
            .block_test(deleting)
            .unwrap_err()
            .to_string()
            .contains("declined")
    );

    let standalone = cx.update(|cx| {
        Workspace::ask_for_person(
            None,
            None,
            "Install powers?".into(),
            vec![choice("approve", "Approve"), choice("decline", "Decline")],
            cx,
        )
    });
    cx.run_until_parked();
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    plugin_refuses(cx, &approval.0);
    assert!(cx.read(|cx| Workspace::global(cx).read(cx).approvals().next().is_some()));
    answer_standalone(cx, "decline");
    assert_eq!(
        cx.foreground_executor().block_test(standalone).unwrap(),
        "decline"
    );
}

#[test]
fn repo_index_selects_named_plugin_and_refuses_ambiguous_or_escaping_sources() {
    let (_keep, stop) = async_channel::bounded::<()>(1);
    let repo = tempfile::tempdir().unwrap();
    write_plugin(&repo.path().join("plugins/echo"), &manifest());
    std::fs::create_dir(repo.path().join(".kagan")).unwrap();
    let path = repo.path().join(".kagan/plugins.json");
    let index = |source: &str| json!({ "schemaVersion": 1, "name": "Test", "plugins": [{ "name": "named", "source": source }] });
    std::fs::write(&path, index("./plugins/echo").to_string()).unwrap();
    let source = source::Source::Local(repo.path().into());
    let plugins = tempfile::tempdir().unwrap();
    let selected = source::fetch(
        &source,
        Some(Path::new("named")),
        plugins.path(),
        spare_data(),
        &stop,
    )
    .unwrap();
    assert!(selected.root.ends_with("plugins/echo"));
    assert!(
        source::fetch(
            &source,
            Some(Path::new("plugins/echo")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .unwrap()
        .root
        .ends_with("plugins/echo")
    );
    assert!(
        source::fetch(
            &source,
            Some(Path::new("../plugins/echo")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .is_err()
    );
    std::fs::write(&path, index("plugins/echo").to_string()).unwrap();
    assert!(
        source::fetch(
            &source,
            Some(Path::new("./plugins/echo")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .unwrap()
        .root
        .ends_with("plugins/echo")
    );
    assert!(source::fetch(&source, None, plugins.path(), spare_data(), &stop).is_ok());
    assert!(
        source::fetch(
            &source,
            Some(Path::new("unknown")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .is_err()
    );
    let mut ambiguous = index("./plugins/echo");
    ambiguous["plugins"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "second", "source": "./plugins/echo" }));
    std::fs::write(&path, ambiguous.to_string()).unwrap();
    assert!(source::fetch(&source, None, plugins.path(), spare_data(), &stop).is_err());
    std::fs::write(&path, index("../outside").to_string()).unwrap();
    assert!(
        source::fetch(
            &source,
            Some(Path::new("named")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("plugins/out")).unwrap();
        std::fs::write(&path, index("./plugins/out").to_string()).unwrap();
        match source::fetch(
            &source,
            Some(Path::new("plugins/out")),
            plugins.path(),
            spare_data(),
            &stop,
        ) {
            Err(error) => assert!(error.to_string().contains("escapes"), "{error}"),
            Ok(_) => panic!("outward symlink was accepted"),
        }
    }
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
fn keymap_sets_removes_and_reads_plugin_keys_and_they_survive_reload(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&[], COMMAND_PROBE);
    let path = plugin.path().join(manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["commands"].as_array_mut().unwrap().extend([
        json!({ "id": "other", "title": "Other", "key": "alt-ctrl-o", "when": "always" }),
        json!({ "id": "third", "title": "Third", "key": "ctrl-alt-o", "when": "always" }),
    ]);
    std::fs::write(path, value.to_string()).unwrap();
    open(cx, &[installed(plugin.path())]);
    let set = |command: &str, key: &str| {
        Op::Keymap(KeymapOp::Set {
            action: format!("{ID}:{command}"),
            key: key.into(),
            reset: false,
        })
    };
    let remove = |command: &str| {
        Op::Keymap(KeymapOp::Remove {
            action: format!("{ID}:{command}"),
        })
    };
    assert!(matches!(
        run(cx, set("missing", "ctrl-alt-o")),
        Err(OpError::NotFound(_))
    ));
    assert!(run(cx, set("ping", "")).is_err());
    assert!(run(cx, set("ping", "ctrl-alt-junk")).is_err());
    assert!(run(cx, remove("missing")).is_err());
    assert_eq!(
        run(cx, set("ping", "ctrl-alt-o")).unwrap(),
        OpResult::Setting(json!({ "ok": false, "conflict": format!("{ID}:other") }))
    );
    run(cx, remove("other")).unwrap();
    run(cx, remove("third")).unwrap();
    assert_eq!(
        run(cx, set("ping", "ctrl-alt-o")).unwrap(),
        OpResult::Setting(json!({ "ok": true }))
    );
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let OpResult::Setting(disabled) = run(cx, Op::Keymap(KeymapOp::Get)).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(disabled["plugins"], json!({}));
    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let OpResult::Setting(keys) = run(cx, Op::Keymap(KeymapOp::Get)).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(
        keys["plugins"],
        json!({
            format!("{ID}:ping"): "ctrl-alt-o",
            format!("{ID}:other"): null,
            format!("{ID}:third"): null,
        })
    );
    run(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: format!("{ID}:other"),
            key: String::new(),
            reset: true,
        }),
    )
    .unwrap();
    let OpResult::Setting(keys) = run(cx, Op::Keymap(KeymapOp::Get)).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(keys["plugins"][format!("{ID}:other")], "ctrl-alt-o");
}

#[test]
fn database_refuses_external_files_and_transaction_control_in_statements() {
    use gpui_shell::HostArguments;
    let data = tempfile::tempdir().unwrap();
    let module = crate::database::module(&data.path().join("database.sqlite3"));
    let call = |name, args: Vec<Value>| {
        let args = HostArguments::new(args.into_iter().map(crate::area::to_host));
        futures::executor::block_on(module.begin(name, &args).unwrap())
            .map(|value| crate::area::from_host(&value))
    };
    let migration = json!(["CREATE TABLE notes (text TEXT NOT NULL)"]);
    call("migrate", vec![migration.clone()]).unwrap();
    call("migrate", vec![migration]).unwrap();
    assert!(call("migrate", vec![json!(["CREATE TABLE other (text TEXT)"])]).is_err());
    assert!(
        call(
            "run",
            vec![
                json!("ATTACH DATABASE ? AS other"),
                json!([data.path().join("other.sqlite3")])
            ]
        )
        .is_err()
    );
    assert!(!data.path().join("other.sqlite3").exists());
    assert!(call("run", vec![json!("PRAGMA foreign_keys = OFF"), json!([])]).is_err());
    assert!(
        call(
            "query",
            vec![
                json!("INSERT INTO notes VALUES ('bad') RETURNING text"),
                json!([])
            ]
        )
        .is_err()
    );
    assert!(
        call(
            "transaction",
            vec![json!([
                { "sql": "INSERT INTO notes VALUES ('rolled back')" },
                { "sql": "COMMIT" }
            ])]
        )
        .is_err()
    );
    assert_eq!(
        call("query", vec![json!("SELECT * FROM notes"), json!([])]).unwrap(),
        json!([])
    );
    call(
        "transaction",
        vec![json!([{ "sql": "INSERT INTO notes VALUES (?)", "params": ["kept"] }])],
    )
    .unwrap();
    assert_eq!(
        call("query", vec![json!("SELECT * FROM notes"), json!([])]).unwrap(),
        json!([{ "text": "kept" }])
    );
}

#[gpui_kit::test]
fn a_plugin_can_mkdir_under_process_data_dir(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    // mkdir runs off the test executor, so wait on real time instead of the
    // test clock, which would fire the tool timeout first.
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"data-dir"}),
            }),
            cx,
        )
    });
    let (sender, reply) = async_channel::bounded(1);
    cx.foreground_executor()
        .spawn(async move { sender.send(task.await).await })
        .detach();
    let result = (|| {
        for _ in 0..2000 {
            cx.run_until_parked();
            if let Ok(result) = reply.try_recv() {
                return result.unwrap();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("data-dir did not answer")
    })();
    let OpResult::ToolReply {
        text,
        is_error: false,
    } = result
    else {
        panic!("data-dir: {result:?}");
    };
    let expected = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .data_dir()
            .join("plugins")
            .join(ID)
            .join("data")
    });
    assert_eq!(text, expected.display().to_string());
    assert!(expected.join("proof").is_dir(), "{text}");
}

#[gpui_kit::test]
fn a_plugin_with_no_powers_sees_none_of_the_environment(cx: &mut TestAppContext) {
    let named = ["HTTPS_PROXY", "PATH", "HOME", "USER", "HOSTNAME", "SHELL"];
    // SAFETY: nextest runs each test in its own process, and nothing else here reads the
    // environment while it is set.
    unsafe {
        std::env::set_var("HTTPS_PROXY", "http://user:secret@proxy.test:8080");
        for name in &named[1..] {
            if std::env::var_os(name).is_none() {
                std::env::set_var(name, "set");
            }
        }
    }
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["powers"] = json!({});
    write_plugin(plugin.path(), &declared);
    open(cx, &[installed(plugin.path())]);
    let asked = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"env"}),
            }),
            cx,
        )
    });
    let OpResult::ToolReply { text, .. } = cx.foreground_executor().block_test(asked).unwrap()
    else {
        panic!("env");
    };
    let seen: Vec<String> = serde_json::from_str(&text).unwrap();
    for name in named {
        assert!(
            !seen.iter().any(|seen| seen.eq_ignore_ascii_case(name)),
            "{name} in {seen:?}"
        );
    }
}

#[gpui_kit::test]
fn cancel_stops_a_git_install_and_leaves_nothing_staged(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    // A server that takes the connection and never answers, so git waits on it.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });
    let install = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source: format!("git:http://127.0.0.1:{port}/plugin.git@main"),
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    });
    // The test executor runs the git step on this thread, where it would wait on the server for
    // ever. So run one task at a time until the install can be stopped, then run Cancel at once:
    // it acts on its first poll, before the git step gets the thread.
    let host = cx.update(|cx| crate::PluginHost::global(cx));
    while !cx.read(|cx| host.read(cx).stoppable()) {
        assert!(
            cx.executor().tick(),
            "the install never reached its git step"
        );
    }
    let mut async_cx = cx.to_async();
    let cancelled = futures::FutureExt::now_or_never(crate::ops::run(
        &host,
        PluginOp::CancelInstall,
        None,
        &mut async_cx,
    ))
    .expect("Cancel answers at once");
    assert!(matches!(cancelled, Ok(OpResult::Done)), "{cancelled:?}");
    let error = cx.foreground_executor().block_test(install).unwrap_err();
    assert_eq!(error, OpError::Failed("the install was stopped".into()));
    let plugins = path.join("plugins");
    let left = std::fs::read_dir(&plugins).map_or(0, Iterator::count);
    assert_eq!(
        left,
        0,
        "no staging folder is left in {}",
        plugins.display()
    );
}

#[gpui_kit::test]
fn context_waiters_receive_changes_and_are_cancelled_on_unload(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, EmptyView, WindowOptions};
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let watch = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"watch"}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let window = cx.update(|cx| {
        let window = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, cx| {
            ui.set_context(
                window.into(),
                Some(thread()),
                Some(ProjectId("proj_1".into())),
                true,
                &[],
                cx,
            );
        });
        window.window_id()
    });
    let OpResult::ToolReply { text, .. } = cx.foreground_executor().block_test(watch).unwrap()
    else {
        panic!("context");
    };
    let context: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(context["threadId"], THREAD);
    assert_eq!(context["focused"], true);
    assert!(context.get("overlay").is_none());
    let watch = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"watch"}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, _| {
            ui.set_overlay(
                window,
                gpui_kit::size(gpui_kit::px(640.), gpui_kit::px(480.)),
            );
        });
    });
    let OpResult::ToolReply { text, .. } = cx.foreground_executor().block_test(watch).unwrap()
    else {
        panic!("overlay");
    };
    let context: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(context["overlay"], json!({ "width": 640, "height": 480 }));
    let watch = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"watch"}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    assert!(cx.foreground_executor().block_test(watch).is_err());
}

#[gpui_kit::test]
fn the_overlay_size_is_the_front_windows(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, EmptyView, WindowOptions};
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let size = cx.update(|cx| {
        let back = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let front = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, cx| {
            ui.set_context(back.into(), None, None, false, &[], cx);
            ui.set_context(front.into(), None, None, true, &[], cx);
            ui.set_overlay(
                back.window_id(),
                gpui_kit::size(gpui_kit::px(300.), gpui_kit::px(200.)),
            );
            ui.set_overlay(
                front.window_id(),
                gpui_kit::size(gpui_kit::px(640.), gpui_kit::px(480.)),
            );
            crate::area::from_host(&ui.context_for(&PluginId(ID.into())))["overlay"].clone()
        })
    });
    assert_eq!(size, json!({ "width": 640, "height": 480 }));
}

#[test]
fn a_replacement_panel_names_one_slot_in_its_own_place() {
    let plugin = plugin_dir();
    let panels = |panels: Value| {
        let mut declared = manifest();
        declared["ui"]["panels"] = panels;
        write_plugin(plugin.path(), &declared);
        read_manifest(plugin.path()).map(|_| ())
    };
    assert!(
        panels(json!([
            { "id": "list", "title": "List", "view": "List", "placement": "left", "replaces": "threads" },
            { "id": "diff", "title": "Diff", "view": "Diff", "placement": "center", "replaces": "review" }
        ]))
        .is_ok()
    );
    for (declared, message) in [
        (
            json!([{ "id": "list", "title": "List", "view": "List", "placement": "center", "replaces": "threads" }]),
            "panel list replaces threads, so it must be placed left",
        ),
        (
            json!([{ "id": "diff", "title": "Diff", "view": "Diff", "placement": "left", "replaces": "review" }]),
            "panel diff replaces review, so it must be placed center",
        ),
        (
            json!([
                { "id": "one", "title": "One", "view": "One", "placement": "left", "replaces": "threads" },
                { "id": "two", "title": "Two", "view": "Two", "placement": "left", "replaces": "threads" }
            ]),
            "only one panel may replace threads",
        ),
        (
            json!([{ "id": "list", "title": "List", "view": "List", "placement": "left", "replaces": "terminal" }]),
            "unknown variant",
        ),
    ] {
        let error = panels(declared).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
    }
    let mut declared = manifest();
    declared["ui"]["panels"] = json!([
        { "id": "diff", "title": "Diff", "view": "Diff", "placement": "center", "replaces": "review" }
    ]);
    declared["powers"]["run"] = json!([{ "program": "git" }]);
    write_plugin(plugin.path(), &declared);
    assert!(read_manifest(plugin.path()).is_ok());
}

/// Install and check both read the manifest first, so a manifest for a spot kagan does not draw is
/// refused before any code runs.
#[test]
fn a_manifest_for_a_spot_kagan_does_not_draw_is_refused() {
    let plugin = plugin_dir();
    let refused = |declared: Value, message: &str| {
        write_plugin(plugin.path(), &declared);
        let error = read_manifest(plugin.path()).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
    };
    for spot in ["toolbar", "status-bar", "sidebar-footer"] {
        let mut declared = manifest();
        declared["ui"][spot] = json!([{ "id": "item", "view": "Item" }]);
        refused(declared, &format!("ui.{spot} is not in this version"));
    }
    let mut declared = manifest();
    declared["ui"]["thread-list"] = json!("Board");
    refused(
        declared,
        "declare a left panel with \"replaces\": \"threads\"",
    );
    for placement in ["left", "right", "center"] {
        let mut declared = manifest();
        declared["ui"]["panels"] =
            json!([{ "id": "notes", "title": "Notes", "view": "Notes", "placement": placement }]);
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_ok(), "{placement}");
    }
    let mut declared = manifest();
    declared["ui"]["panels"] =
        json!([{ "id": "notes", "title": "Notes", "view": "Notes", "placement": "right" }]);
    write_plugin(plugin.path(), &declared);
    let error = crate::development::check(plugin.path(), spare_data())
        .unwrap_err()
        .to_string();
    assert_eq!(error, "declared view Notes is not exported by main.js");
    let mut declared = manifest();
    declared["ui"]["panels"] =
        json!([{ "id": "notes", "title": "Notes", "view": "Notes", "placement": "bottom" }]);
    refused(declared, "bottom panels are not in this version");
    let mut declared = manifest();
    declared["ui"]["panels"] = json!([{
        "id": "notes",
        "title": "Notes",
        "view": "Notes",
        "placement": "right",
        "store": true
    }]);
    refused(declared, "store belongs on WebView.store(true)");
}

#[test]
fn a_header_command_needs_when_thread() {
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["commands"] = json!([{ "id": "hand", "title": "Hand off", "header": true }]);
    write_plugin(plugin.path(), &declared);
    let error = read_manifest(plugin.path()).unwrap_err().to_string();
    assert!(
        error.contains("a header command needs when thread"),
        "{error}"
    );
    declared["commands"] =
        json!([{ "id": "hand", "title": "Hand off", "when": "thread", "header": true }]);
    write_plugin(plugin.path(), &declared);
    assert!(read_manifest(plugin.path()).is_ok());
}

/// A plugin's theme is a file in its folder, checked at install and at check with the schema of
/// a person's theme file.
#[test]
fn a_manifest_declares_theme_files_inside_the_plugin() {
    let plugin = plugin_dir();
    let _outside = write_theme_files(&plugin.path().join("themes"));
    let declare = |themes: Value| {
        let mut declared = manifest();
        declared["themes"] = themes;
        write_plugin(plugin.path(), &declared);
        read_manifest(plugin.path())
            .and_then(|manifest| manifest.themes(plugin.path()).map_err(OpError::Invalid))
    };

    let read = declare(json!([{ "id": "midnight", "file": "themes/midnight.json" }])).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].0, format!("{ID}/midnight"));
    assert_eq!(read[0].1.name, "Ocean");
    assert!(check_plugin(plugin.path()).is_ok());

    for (themes, message) in [
        (json!([{ "name": "midnight" }]), "unknown field `name`"),
        (
            json!([{ "id": "Mid Night", "file": "themes/midnight.json" }]),
            "theme id \"Mid Night\" must be unique",
        ),
        (
            json!([
                { "id": "midnight", "file": "themes/midnight.json" },
                { "id": "midnight", "file": "themes/midnight.json" }
            ]),
            "theme id \"midnight\" must be unique",
        ),
        (
            json!([{ "id": "away", "file": "../midnight.json" }]),
            "theme away file ../midnight.json: must be a path inside the plugin",
        ),
        (
            json!([{ "id": "gone", "file": "themes/gone.json" }]),
            "theme gone file themes/gone.json: ",
        ),
        (
            json!([{ "id": "blank", "file": "themes/blank.json" }]),
            "theme blank file themes/blank.json: theme name is empty",
        ),
        (
            json!([{ "id": "odd", "file": "themes/odd.json" }]),
            "theme odd file themes/odd.json: expected #RRGGBB",
        ),
        (
            json!([{ "id": "big", "file": "themes/big.json" }]),
            "theme big file themes/big.json: larger than 64 KiB",
        ),
        (
            json!([{ "id": "long", "file": "themes/long.json" }]),
            "theme long file themes/long.json: theme name must be one line of at most 64 characters",
        ),
        (
            json!([{ "id": "lines", "file": "themes/lines.json" }]),
            "theme lines file themes/lines.json: theme name must be one line",
        ),
        (
            json!([{ "id": "dir", "file": "themes/dir.json" }]),
            "theme dir file themes/dir.json: not a regular file",
        ),
    ]
    .into_iter()
    .chain(
        [
            (
                json!([{ "id": "link", "file": "themes/link.json" }]),
                "theme link file themes/link.json: must stay inside the plugin",
            ),
            (
                json!([{ "id": "pipe", "file": "themes/pipe.json" }]),
                "theme pipe file themes/pipe.json: not a regular file",
            ),
        ]
        .into_iter()
        .filter(|_| cfg!(unix)),
    ) {
        let error = declare(themes).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
        let checked = check_plugin(plugin.path()).unwrap_err().to_string();
        assert!(checked.contains(message), "{checked}");
    }
}

/// One good theme and one of each bad kind. The returned folder holds the target of a symlink
/// that leaves the plugin.
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

#[gpui_kit::test]
fn a_right_panel_loads_without_a_skip_note(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["ui"]["panels"] = json!([
        {"id":"side","title":"Side","view":"Side","placement":"right"},
        {"id":"tool","title":"Tool","view":"Side","placement":"left"}
    ]);
    write_plugin(plugin.path(), &declared);
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            "{MAIN}\nexport class Side extends View {{\n  render() {{ return \"side\"; }}\n}}\n"
        ),
    )
    .unwrap();
    open(cx, &[installed(plugin.path())]);
    let read = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let host = crate::PluginHost::global(cx).read(cx);
            let status = host
                .status()
                .into_iter()
                .find(|status| status.id.0 == ID)
                .expect("status");
            (
                status.error.clone(),
                status.note.as_deref().map(str::to_owned),
                host.failure(&PluginId(ID.into())),
            )
        })
    };
    let (error, note, failure) = read(cx);
    assert!(error.is_none(), "{error:?}");
    assert!(failure.is_none(), "{failure:?}");
    assert!(note.is_none(), "{note:?}");
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                crate::PluginHost::global(cx)
                    .update(cx, |host, cx| {
                        host.mount_panel(&PluginId(ID.into()), "side", window, cx)
                    })
                    .unwrap()
            })
            .unwrap();
    });
    let (error, note, failure) = read(cx);
    assert!(error.is_none(), "{error:?}");
    assert!(failure.is_none(), "{failure:?}");
    assert!(note.is_none(), "{note:?}");
}

#[test]
fn an_overlay_names_one_exported_view() {
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["ui"]["overlay"] = json!("Plate");
    write_plugin(plugin.path(), &declared);
    let read = read_manifest(plugin.path()).unwrap();
    assert_eq!(read.ui.overlay.as_deref(), Some("Plate"));
    for overlay in [json!(""), json!(["Plate"]), json!({ "view": "Plate" })] {
        declared["ui"]["overlay"] = overlay;
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_err());
    }
}

#[test]
fn ui_manifest_rejects_unknown_spots_and_duplicate_panels() {
    let plugin = plugin_dir();
    for ui in [
        json!({"unknown": "View"}),
        json!({"panels": [{"id":"same","title":"A","view":"A","placement":"left"}, {"id":"same","title":"B","view":"B","placement":"right"}]}),
        json!({"overlay":"../escape"}),
    ] {
        let mut declared = manifest();
        declared["ui"] = ui;
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_err());
    }
}

#[test]
fn a_file_opener_is_refused_with_a_reason() {
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["ui"] = json!({"file-opener": {"extensions": [".rs"], "job": "open"}});
    write_plugin(plugin.path(), &declared);
    let error = read_manifest(plugin.path()).unwrap_err().to_string();
    assert!(
        error.contains("ui.file-opener is not in this version of kagan"),
        "{error}"
    );
}

#[gpui_kit::test]
fn a_named_export_mounts_beside_the_default_view(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["ui"]["panels"] = json!([
        {"id":"side","title":"Side","view":"Side","placement":"right"}
    ]);
    write_plugin(plugin.path(), &declared);
    open(cx, &[installed(plugin.path())]);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                let host = crate::PluginHost::global(cx);
                let missing = host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId(ID.into()), "missing", window, cx)
                });
                assert!(
                    matches!(missing, Err(crate::MountError::Missing(ref message)) if message.contains("has no panel")),
                    "{missing:?}"
                );
                let failed = host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId(ID.into()), "side", window, cx)
                });
                let Err(crate::MountError::Failed(error)) = failed else {
                    panic!("{failed:?}");
                };
                assert!(error.contains("export `Side`"), "{error}");
                assert!(!host.update(cx, |host, cx| {
                    host.reject_mount(&PluginId(ID.into()), &error, cx)
                }));
            })
            .unwrap();
    });
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );

    let plugin = plugin_dir();
    write_plugin(plugin.path(), &declared);
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            "{MAIN}\nexport class Side extends View {{\n  render() {{ return \"side\"; }}\n}}\n"
        ),
    )
    .unwrap();
    open(cx, &[installed(plugin.path())]);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    let side = cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                let host = crate::PluginHost::global(cx);
                host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId(ID.into()), "side", window, cx)
                })
                .unwrap()
            })
            .unwrap()
    });
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );
    drop(side);
}

#[gpui_kit::test]
fn a_named_mount_that_fails_to_construct_unloads_and_logs(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut declared = manifest();
    declared["ui"]["panels"] = json!([
        {"id":"side","title":"Side","view":"Side","placement":"right"}
    ]);
    write_plugin(plugin.path(), &declared);
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            "{MAIN}\nexport class Side extends View {{\n  init() {{ throw new Error(\"side broke\"); }}\n  render() {{ return \"side\"; }}\n}}\n"
        ),
    )
    .unwrap();
    open(cx, &[installed(plugin.path())]);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    let failed = cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                crate::PluginHost::global(cx).update(cx, |host, cx| {
                    host.mount_named_view(&PluginId(ID.into()), "Side", window, cx)
                })
            })
            .unwrap()
    });
    let Err(crate::MountError::Failed(error)) = failed else {
        panic!("{failed:?}");
    };
    assert!(error.contains("side broke"), "{error}");
    assert!(!loaded(cx));
    cx.run_until_parked();
    let log = cx.read(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.read(cx);
        let path = host.data_dir.join("plugins").join(ID).join("logs.jsonl");
        std::fs::read_to_string(path).unwrap_or_default()
    });
    assert!(log.contains("side broke"), "{log}");
    assert!(log.contains("\"level\":\"error\""), "{log}");
}

#[gpui_kit::test]
fn open_panel_props_stay_on_the_plugin_that_sent_them(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, EmptyView, WindowOptions};
    open(cx, &[]);
    let mine = PluginId(ID.into());
    let other = PluginId("dev.test.other".into());
    let (sender, receiver) = async_channel::bounded(1);
    cx.update(|cx| {
        let window = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, cx| {
            let window: gpui_kit::AnyWindowHandle = window.into();
            ui.set_context(window, Some(thread()), None, true, &[], cx);
            ui.push_waiter(other.clone(), sender);
            ui.deliver(window.window_id(), &mine, json!({"panel": "side"}));
        });
        let ui = crate::HostUi::global(cx).unwrap();
        let mine_context = {
            let value = ui.read(cx).context_for(&mine);
            crate::area::from_host(&value)
        };
        let other_context = {
            let value = ui.read(cx).context_for(&other);
            crate::area::from_host(&value)
        };
        assert_eq!(mine_context["props"], json!({"panel": "side"}));
        assert!(other_context.get("props").is_none(), "{other_context}");
        assert_eq!(other_context["focused"], true);
    });
    let waited = crate::area::from_host(&receiver.try_recv().unwrap());
    assert!(
        waited.get("props").is_none(),
        "plugin B must not see A's props: {waited}"
    );
    assert_eq!(waited["threadId"], THREAD);
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

#[gpui_kit::test]
fn a_declared_cli_command_runs_as_a_cli_job(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&[], CLI_PROBE);
    let mut value = manifest();
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
    let mut value = manifest();
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

#[test]
fn an_approved_grant_reads_as_plain_lines() {
    let plugin = plugin_dir();
    let mut value = manifest();
    value["powers"] = json!({ "areas": ["threads"] });
    write_plugin(plugin.path(), &value);
    let approved = installed(plugin.path()).approved;
    assert_eq!(
        crate::power_lines(&approved).unwrap(),
        [
            "reads, starts and changes your threads, can type into threads and press Enter, reads their screen text, and reads their changed files",
            "reads a folder you choose (Notes)",
        ]
    );
    assert!(crate::power_lines(&json!("x")).is_err());
}

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
    let mut value = manifest();
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

#[gpui_kit::test]
fn process_run_answers_inside_a_script_call(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, args) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let output = forward(cx, "process", "run", &json!([name, args])).unwrap();
    assert_eq!(output["exitCode"], 0);
    assert_eq!(output["stdout"].as_str().unwrap().trim(), "hello");
    assert_eq!(output["stderr"].as_str().unwrap().trim(), "error");
    let refused = forward(cx, "process", "run", &json!(["unapproved", []])).unwrap_err();
    assert!(refused.contains("not granted"), "{refused}");
}

#[gpui_kit::test]
fn threads_output_reads_scrollback_plain_text(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let plugin = probe_plugin(&["threads"], THREADS_FORWARD);
    let mut value = manifest();
    value["powers"] = json!({ "areas": ["threads"] });
    value["tools"].as_array_mut().unwrap().push(json!({
        "name": "probe",
        "description": "Forward a host call",
        "input": "schemas/any.json",
        "scope": "all"
    }));
    write_plugin(plugin.path(), &value);
    std::fs::write(plugin.path().join("main.js"), THREADS_FORWARD).unwrap();
    open(cx, &[installed(plugin.path())]);

    let program = if cfg!(windows) {
        "cmd.exe".into()
    } else {
        "/bin/sh".into()
    };
    let OpResult::Thread(started) = run(
        cx,
        Op::Thread(ThreadOp::Run(Run {
            project_id: ProjectId("prj_test".into()),
            program,
            title: Some("Output".into()),
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
    for _ in 0..200 {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(5));
        let status = cx.update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .thread(&started.id)
                .map(|thread| thread.read(cx).record().status)
        });
        if matches!(status, Some(ThreadStatus::Active | ThreadStatus::Idle)) {
            break;
        }
    }
    let marker = "kagan-output-marker";
    let typed = if cfg!(windows) {
        format!("echo {marker}\r\n")
    } else {
        format!("printf '{marker}\\n'\n")
    };
    run(
        cx,
        Op::Thread(ThreadOp::Input {
            id: started.id.clone(),
            text: typed,
            terminal: None,
        }),
    )
    .unwrap();
    let mut text = String::new();
    for _ in 0..200 {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(5));
        let value = forward(cx, "threads", "output", &json!([started.id.0])).unwrap();
        text = value["output"].as_str().unwrap_or("").to_owned();
        if text.contains(marker) {
            break;
        }
    }
    assert!(text.contains(marker), "output missing {marker}: {text:?}");
    let missing = forward(cx, "threads", "output", &json!(["thr_missing"])).unwrap_err();
    assert!(missing.contains("not found"), "{missing}");
}

fn sleeping() -> Value {
    if cfg!(windows) {
        json!(["/C", "ping -n 30 127.0.0.1 >nul"])
    } else {
        json!(["-c", "sleep 30"])
    }
}

#[gpui_kit::test]
fn process_run_times_out(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let receive = dispatched(
        cx,
        forwarded(
            "process",
            "run",
            &json!([name, sleeping(), { "timeoutMs": 1 }]),
        ),
    );
    let error = forwarded_reply(settled(cx, &receive, Duration::from_millis(1))).unwrap_err();
    assert!(error.contains("timed out"), "{error}");
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
#[gpui_kit::test]
fn unloading_a_plugin_ends_its_running_command_and_kills_the_process_tree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let (pids, receive) = start_tree(cx, name);
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let error = settled(cx, &receive, Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("unloaded"), "{error}");
    for _ in 0..2000 {
        if pids.iter().all(|pid| !alive(pid)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the process tree outlived the plugin: {pids:?}");
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

#[cfg(unix)]
#[gpui_kit::test]
fn quitting_the_app_ends_a_running_command_and_kills_the_process_tree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let (pids, _receive) = start_tree(cx, name);
    cx.quit();
    for _ in 0..2000 {
        if pids.iter().all(|pid| !alive(pid)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the process tree outlived the app: {pids:?}");
}

#[gpui_kit::test]
fn unloading_a_plugin_ends_a_running_command_with_an_unloaded_error(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let receive = dispatched(cx, forwarded("process", "run", &json!([name, sleeping()])));
    cx.run_until_parked();
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let error = settled(cx, &receive, Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("unloaded"), "{error}");
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

#[gpui_kit::test]
fn bundled_examples_run_through_the_host(cx: &mut TestAppContext) {
    let examples = crate::examples();
    let plugins: Vec<_> = examples
        .iter()
        .map(|example| installed_example(&example.root))
        .collect();
    assert!(plugins.len() >= 2, "{:?}", examples.len());
    open(cx, &plugins);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    let mounted = cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                let host = crate::PluginHost::global(cx);
                host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId("dev.kagan.panel".into()), "hello", window, cx)
                })
                .unwrap()
            })
            .unwrap()
    });
    assert_eq!(
        answered(
            cx,
            Op::Plugin(PluginOp::Cli {
                name: "hello".into(),
                command: "hi".into(),
                argv: Vec::new(),
                cwd: "/work".into(),
                thread_id: None,
                project_id: None,
            }),
        )
        .unwrap(),
        OpResult::Cli {
            exit_code: 0,
            stdout: "Hello\n".into(),
            stderr: String::new(),
        }
    );
    drop(mounted);
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
    std::fs::write(dir.path().join(manifest::FILE), manifest.to_string()).unwrap();
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
fn a_plugin_that_did_not_start_says_why_when_listed(cx: &mut TestAppContext) {
    // An install from before the refusal: its manifest still declares a toolbar.
    let plugin = plugin_dir();
    let mut old = manifest();
    old["ui"] = json!({"toolbar": [{"id": "canvas-open", "view": "CanvasButton"}]});
    std::fs::write(plugin.path().join(manifest::FILE), old.to_string()).unwrap();
    let root = dunce::canonicalize(plugin.path()).unwrap();
    let record = Plugin {
        id: PluginId(ID.into()),
        source: root.display().to_string(),
        approved: json!({"powers": {}, "downloads": {}}),
        root,
        commit: None,
        enabled: true,
        error: None,
    };
    open(cx, &[record]);
    let OpResult::Plugins(list) = run(cx, Op::Plugin(PluginOp::List)).unwrap() else {
        panic!("list returned no plugins");
    };
    let error = list[0]
        .error
        .as_deref()
        .expect("the start failure is listed");
    assert!(error.contains("toolbar"), "{error}");
}

#[gpui_kit::test]
fn a_second_install_is_refused_while_one_waits_for_approval(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[]);
    let source = plugin.path().display().to_string();
    let first = plugin_op(cx, install(source.clone(), None, None));
    let second = plugin_op(cx, install(source.clone(), None, None));
    let error = cx
        .foreground_executor()
        .block_test(second)
        .unwrap_err()
        .to_string();
    assert!(error.contains("already installing"), "{error}");
    answer_standalone(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(first).unwrap(),
        OpResult::Done
    );
    assert!(installing_now(cx).is_none(), "the slot is free again");
}

fn installing_now(cx: &mut TestAppContext) -> Option<String> {
    cx.read(|cx| crate::PluginHost::global(cx).read(cx).installing())
}

#[gpui_kit::test]
fn a_disabled_plugin_keeps_its_name_and_version_from_its_manifest(cx: &mut TestAppContext) {
    // A plugin that is off is never activated. Its manifest still names it, as at a restart.
    let plugin = plugin_dir();
    let mut record = installed(plugin.path());
    record.enabled = false;
    open(cx, &[record]);
    let status = cx.read(|cx| {
        crate::PluginHost::global(cx)
            .read(cx)
            .status()
            .into_iter()
            .find(|status| status.id.0 == ID)
            .expect("the plugin is listed")
    });
    assert!(!status.enabled);
    assert_eq!(status.name.as_str(), "Echo");
    assert_eq!(status.version.as_deref(), Some("1.0.0"));
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
    let mut value = manifest();
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

#[gpui_kit::test]
fn check_names_the_invalid_javascript_file_and_line(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class View {}\nconst = ;\n",
    )
    .unwrap();
    open(cx, &[]);
    let task = plugin_op(
        cx,
        PluginOp::Check {
            path: plugin.path().to_path_buf(),
        },
    );
    let error = cx
        .foreground_executor()
        .block_test(task)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("main.js") && error.contains("line 2"),
        "{error}"
    );
}

/// A git repository with the echo plugin, an index repository listing it as `echo`, and an
/// open host whose first index, `missing`, cannot be read.
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
    write_plugin(&plugin.path().join("echo"), &manifest());
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

#[gpui_kit::test]
fn indexes_are_added_in_order_searched_and_an_unreadable_one_is_a_warning(cx: &mut TestAppContext) {
    let Indexed {
        source,
        listed,
        missing,
        data,
        _repos,
        ..
    } = indexed(cx);
    let add = |location: &str| {
        index_op(IndexOp::Add {
            location: location.into(),
        })
    };

    assert_eq!(
        run(cx, add(&listed)).unwrap(),
        OpResult::Indexes(vec![missing.clone(), listed.clone()])
    );
    let again = run(cx, add(&listed)).unwrap_err();
    assert!(again.to_string().contains("already added"), "{again}");
    assert!(
        run(cx, add(&missing.replace("missing", "gone"))).is_err(),
        "an index that cannot be read is not added"
    );

    let (listings, warnings) = search(cx, "ECHOES");
    assert_eq!(
        listings
            .iter()
            .map(|listing| (listing.name.as_str(), listing.source.as_str()))
            .collect::<Vec<_>>(),
        [("echo", source.as_str())]
    );
    let [warning] = warnings.as_slice() else {
        panic!("{warnings:?}")
    };
    assert!(
        warning.starts_with(&format!("could not read index {missing}")),
        "{warning}"
    );
    assert!(search(cx, "canvas").0.is_empty());

    let error = run(cx, Op::Plugin(install("echo".into(), Some(thread()), None))).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("could not read index {missing}")),
        "an earlier index that cannot be read stops the lookup: {error}"
    );
    run(cx, index_op(IndexOp::Remove { location: missing })).unwrap();
    assert_eq!(
        std::fs::read_to_string(data.join("indexes.json"))
            .unwrap()
            .replace([' ', '\n'], ""),
        json!([listed]).to_string()
    );
}

#[gpui_kit::test]
fn install_by_name_reads_the_index_and_still_asks_a_person(cx: &mut TestAppContext) {
    let Indexed {
        source,
        commit,
        listed,
        missing,
        _repos,
        ..
    } = indexed(cx);
    run(cx, index_op(IndexOp::Remove { location: missing })).unwrap();
    run(
        cx,
        index_op(IndexOp::Add {
            location: listed.clone(),
        }),
    )
    .unwrap();
    let nothing = run(
        cx,
        Op::Plugin(install("nothing".into(), Some(thread()), None)),
    )
    .unwrap_err();
    assert!(
        nothing
            .to_string()
            .contains("no index lists a plugin named nothing"),
        "{nothing}"
    );

    let other = run(
        cx,
        Op::Plugin(install("other/echo".into(), Some(thread()), None)),
    );
    assert!(other.is_err_and(|error| {
        error
            .to_string()
            .contains("no index lists a plugin named other/echo")
    }));

    let declined = plugin_op(cx, install("acme/echo".into(), Some(thread()), None));
    assert_eq!(
        answer(cx, "decline"),
        format!(
            "Install Echo 1.0.0 ({ID})?\nFrom {} at main, commit {commit}, listed by index acme at {listed}.\nCaution: this repository is on this computer, not where an index you added lives.\nIt:\n- writes to your clipboard\n- reads a folder you choose (Notes)",
            source.trim_start_matches("git:").trim_end_matches("@main")
        )
    );
    assert!(cx.foreground_executor().block_test(declined).is_err());
    assert!(records(cx).is_empty());

    let approved = plugin_op(cx, install("echo".into(), Some(thread()), None));
    answer(cx, "approve");
    cx.foreground_executor().block_test(approved).unwrap();
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(
        record.source, source,
        "the record keeps the git source, not the name"
    );
    assert_eq!(record.commit.as_deref(), Some(commit.as_str()));
    let again = run(cx, Op::Plugin(install("echo".into(), Some(thread()), None))).unwrap_err();
    assert!(
        again.to_string().contains("already installed"),
        "an index cannot point an installed plugin at another repository: {again}"
    );
    let index = Path::new(listed.trim_start_matches("git:").trim_end_matches("@main"));
    std::fs::write(
        index.join(".kagan/index.json"),
        json!({
            "schemaVersion": 1,
            "name": "acme",
            "plugins": [{ "name": "echo", "description": "Moved", "source": "git:https://example.com/evil/echo@main" }]
        })
        .to_string(),
    )
    .unwrap();
    git(index, &["commit", "--quiet", "-am", "moved"]);
    let update = PluginOp::Update {
        id: PluginId(ID.into()),
        thread_id: None,
    };
    assert_eq!(
        run(cx, Op::Plugin(update)).unwrap(),
        OpResult::Done,
        "update reads the recorded source, not the index"
    );
    assert_eq!(records(cx)[0].source, source);
}

#[gpui_kit::test]
fn share_prints_an_entry_that_pins_the_pushed_commit_and_installs_through_approval(
    cx: &mut TestAppContext,
) {
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--quiet", "--bare"]);
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "--quiet", "--initial-branch=main"]);
    let folder = work.path().join("echo");
    write_plugin(&folder, &manifest());
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "--quiet", "-m", "one"]);
    let commit = git(work.path(), &["rev-parse", "HEAD"]);
    let url = remote.path().display().to_string();
    git(work.path(), &["remote", "add", "origin", &url]);
    open(cx, &[]);
    let share = |path: &Path| Op::Plugin(PluginOp::Share { path: path.into() });

    let unpushed = run(cx, share(&folder)).unwrap_err();
    assert!(unpushed.to_string().contains("push commit"), "{unpushed}");
    git(work.path(), &["push", "--quiet", "origin", "main"]);
    std::fs::write(folder.join("notes.txt"), "draft").unwrap();
    let changed = run(cx, share(&folder)).unwrap_err();
    assert!(
        changed.to_string().contains("commit the changes"),
        "{changed}"
    );
    std::fs::remove_file(folder.join("notes.txt")).unwrap();

    let Ok(OpResult::Shared { entry, install }) = run(cx, share(&folder)) else {
        panic!("share")
    };
    let source = format!("git:{url}@{commit}");
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({ "name": "echo", "description": "Echo things back.", "source": source, "plugin": "echo" })
    );
    assert_eq!(install, format!("kg plugin install {source} --plugin echo"));
    let index = json!({ "schemaVersion": 1, "name": "acme", "plugins": [entry] }).to_string();
    assert_eq!(
        crate::index::parse(&index, None).unwrap().1[0].source,
        source
    );

    let installing = plugin_op(cx, self::install(source, Some(thread()), Some("echo")));
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Install Echo 1.0.0 ({ID})?\nFrom {url} at {commit}, commit {commit}.\nCaution: this repository is on this computer, not where an index you added lives.\nIt:\n- writes to your clipboard\n- reads a folder you choose (Notes)"
        )
    );
    cx.foreground_executor().block_test(installing).unwrap();
    assert_eq!(records(cx)[0].commit.as_deref(), Some(commit.as_str()));
}

fn local_index(dir: &Path, name: &str, plugins: &Value) {
    std::fs::create_dir_all(dir.join(".kagan")).unwrap();
    std::fs::write(
        dir.join(".kagan/index.json"),
        json!({ "schemaVersion": 1, "name": name, "plugins": plugins }).to_string(),
    )
    .unwrap();
}

#[gpui_kit::test]
fn an_index_cannot_fake_card_lines_or_take_another_index_name(cx: &mut TestAppContext) {
    let (first, second, faker) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let echo = json!([{ "name": "echo", "description": "Echo", "source": "git:https://example.com/evil/echo@main" }]);
    local_index(first.path(), "acme", &json!([]));
    local_index(second.path(), "other", &echo);
    local_index(
        faker.path(),
        "acme.\nIt asks for no extra powers.",
        &json!([]),
    );
    open(cx, &[]);
    let add = |dir: &TempDir| {
        index_op(IndexOp::Add {
            location: dir.path().display().to_string(),
        })
    };

    let lines = run(cx, add(&faker)).unwrap_err();
    assert!(lines.to_string().contains("a name of up to 64"), "{lines}");
    run(cx, add(&first)).unwrap();
    local_index(faker.path(), "acme", &echo);
    let taken = run(cx, add(&faker)).unwrap_err();
    assert!(
        taken
            .to_string()
            .contains("two indexes cannot share a name"),
        "{taken}"
    );

    run(cx, add(&second)).unwrap();
    local_index(second.path(), "acme", &echo);
    let hijack = run(
        cx,
        Op::Plugin(install("acme/echo".into(), Some(thread()), None)),
    )
    .unwrap_err();
    assert!(
        hijack
            .to_string()
            .contains("no index lists a plugin named acme/echo"),
        "a later index that renames itself acme is not acme: {hijack}"
    );
    let (listings, warnings) = search(cx, "");
    assert!(listings.is_empty(), "{listings:?}");
    assert!(warnings[0].contains("is also named acme"), "{warnings:?}");

    let pinned = run(
        cx,
        Op::Plugin(install("echo".into(), Some(thread()), Some("other"))),
    )
    .unwrap_err();
    assert!(pinned.to_string().contains("--plugin is for"), "{pinned}");
}

#[gpui_kit::test]
fn a_saved_list_that_cannot_be_read_is_not_overwritten(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("indexes.json"), "{ not json").unwrap();
    let team = tempfile::tempdir().unwrap();
    local_index(team.path(), "team", &json!([]));
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let error = run(
        cx,
        index_op(IndexOp::Add {
            location: team.path().display().to_string(),
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot be read"), "{error}");
    assert_eq!(
        std::fs::read_to_string(path.join("indexes.json")).unwrap(),
        "{ not json"
    );
}

#[gpui_kit::test]
fn share_keeps_the_ssh_user_and_drops_a_token(cx: &mut TestAppContext) {
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "--quiet", "--initial-branch=main"]);
    write_plugin(work.path(), &manifest());
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "--quiet", "-m", "one"]);
    let commit = git(work.path(), &["rev-parse", "HEAD"]);
    git(
        work.path(),
        &[
            "remote",
            "add",
            "origin",
            "ssh://git:hunter2@example.com/acme/echo",
        ],
    );
    git(
        work.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    open(cx, &[]);

    let Ok(OpResult::Shared { entry, install }) = run(
        cx,
        Op::Plugin(PluginOp::Share {
            path: work.path().into(),
        }),
    ) else {
        panic!("share")
    };
    let source = format!("git:ssh://git@example.com/acme/echo@{commit}");
    assert_eq!(entry.source, source);
    assert_eq!(entry.plugin, None);
    assert_eq!(install, format!("kg plugin install {source}"));
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
    std::fs::write(plugin.path().join(manifest::FILE), manifest.to_string()).unwrap();
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

#[gpui_kit::test]
fn a_page_message_round_trips_through_the_plugin(cx: &mut TestAppContext) {
    let run = open_page(cx);
    deliver_page(cx, &run, r#"{"from":"page"}"#).unwrap();
    let mut echoed = String::new();
    for _ in 0..2000 {
        cx.run_until_parked();
        if echoed.is_empty()
            && let Ok(message) = run.posted.try_recv()
        {
            echoed = message;
        }
        if run.seen.is_file() && !echoed.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read_to_string(&run.seen).unwrap(),
        r#"{"from":"page"}"#
    );
    assert!(
        echoed.contains("\"echo\"") && echoed.contains("\"from\":\"page\""),
        "{echoed}"
    );
}

#[gpui_kit::test]
fn a_page_message_over_the_cap_is_refused(cx: &mut TestAppContext) {
    let run = open_page(cx);
    let over = deliver_page(cx, &run, &format!("\"{}\"", "y".repeat(crate::MESSAGE_CAP)));
    assert!(over.unwrap_err().to_string().contains("over the"));
    deliver_page(cx, &run, r#"{"big":true}"#).unwrap();
    for _ in 0..2000 {
        cx.run_until_parked();
        if run.seen.is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let error = std::fs::read_to_string(&run.seen).unwrap();
    assert!(error.starts_with("error:"), "{error}");
    assert!(error.contains("over the"), "{error}");
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
    std::fs::write(plugin.path().join(manifest::FILE), manifest.to_string()).unwrap();
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

#[gpui_kit::test]
fn a_page_flood_while_next_is_withheld_is_bounded(cx: &mut TestAppContext) {
    let _plugin = open_page_holding_next(cx);
    let life = std::sync::Arc::new(());
    let mut accepted = 0;
    for i in 0..crate::jobs::PAGE_PENDING_CAP + 8 {
        let raw = format!(r#"{{"n":{i}}}"#);
        match flood_page(cx, &life, &raw) {
            Ok(()) => accepted += 1,
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("cannot take another page message"),
                    "{error}"
                );
                break;
            }
        }
    }
    assert_eq!(accepted, crate::jobs::PAGE_PENDING_CAP);
    assert!(
        pending_page_bytes(cx) <= crate::jobs::PAGE_PENDING_BYTES,
        "{}",
        pending_page_bytes(cx)
    );
    assert!(
        flood_page(cx, &life, r#"{"n":"over"}"#)
            .unwrap_err()
            .to_string()
            .contains("cannot take another page message")
    );

    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(pending_page_bytes(cx), 0);
    let mut after = 0;
    for i in 0..crate::jobs::PAGE_PENDING_CAP + 8 {
        let raw = format!(r#"{{"again":{i}}}"#);
        match flood_page(cx, &life, &raw) {
            Ok(()) => after += 1,
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("cannot take another page message"),
                    "{error}"
                );
                break;
            }
        }
    }
    assert_eq!(after, crate::jobs::PAGE_PENDING_CAP);
    assert!(
        pending_page_bytes(cx) <= crate::jobs::PAGE_PENDING_BYTES,
        "{}",
        pending_page_bytes(cx)
    );
}

#[gpui_kit::test]
fn a_page_job_expires_when_its_page_token_dies(cx: &mut TestAppContext) {
    let run = open_page(cx);
    cx.update(|cx| {
        let stale_life = std::sync::Arc::new(());
        let alive = std::sync::Arc::downgrade(&stale_life);
        crate::PluginHost::global(cx)
            .update(cx, |host, cx| {
                host.deliver_page(
                    &PluginId(PAGE_PLUGIN.into()),
                    "board",
                    r#"{"at":"stale"}"#,
                    alive,
                    cx,
                )
            })
            .unwrap();
        drop(stale_life);
    });
    deliver_page(cx, &run, r#"{"at":"fresh"}"#).unwrap();
    for _ in 0..2000 {
        cx.run_until_parked();
        if std::fs::read_to_string(&run.seen)
            .unwrap_or_default()
            .contains("fresh")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let text = std::fs::read_to_string(&run.seen).unwrap();
    assert!(!text.contains("stale"), "{text}");
    assert!(text.contains("fresh"), "{text}");
}

#[gpui_kit::test]
fn a_project_setting_takes_a_live_project_id(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let config = |op| {
        Op::Plugin(PluginOp::Config {
            id: PluginId(ID.into()),
            op,
            thread_id: None,
        })
    };
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(Value::Null)
    );
    let gone = run(
        cx,
        config(ConfigOp::Set {
            key: "board".into(),
            value: Some(json!("prj_gone")),
        }),
    )
    .unwrap_err();
    assert!(gone.to_string().contains("no project prj_gone"), "{gone}");
    run(
        cx,
        config(ConfigOp::Set {
            key: "board".into(),
            value: Some(json!("prj_test")),
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!("prj_test"))
    );
    run(
        cx,
        config(ConfigOp::Clear {
            key: "board".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(Value::Null)
    );
}

#[test]
fn instruct_is_an_install_line_and_needs_approval_again() {
    let plugin = plugin_dir();
    let base = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !base
            .lines()
            .iter()
            .any(|line| line.contains("instructions"))
    );
    let mut value = manifest();
    value["instruct"] = json!(true);
    write_plugin(plugin.path(), &value);
    let with = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        with.lines()
            .iter()
            .any(|line| line == "adds instructions to a thread's program"),
        "{:?}",
        with.lines()
    );
    assert!(base.within(&with));
    assert!(!with.within(&base));
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
    std::fs::write(dir.path().join(manifest::FILE), manifest.to_string()).unwrap();
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

#[gpui_kit::test]
fn a_failed_store_delete_keeps_its_id_and_says_why(cx: &mut TestAppContext) {
    let _hook = crate::web_store::set_remove_hook(|_| {
        crate::web_store::StoreAttempt::Failed("the store is locked".into())
    });
    let mut async_cx = cx.to_async();
    let removal = cx
        .foreground_executor()
        .block_test(crate::web_store::remove_web_stores(
            vec![[7; 16]],
            &mut async_cx,
        ));
    assert!(removal.removed.is_empty(), "a failed delete keeps store.id");
    assert_eq!(removal.warnings, vec!["the store is locked".to_owned()]);
}
