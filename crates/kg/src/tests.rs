use clap::builder::styling::Styles;
use clap::{CommandFactory, Parser};
use interprocess::local_socket::Name;
use kg_domain::{
    AgentTool, ConfigOp, DiffOp, DiffQuery, DiffScope, EnvironmentSpec, IndexOp, Metadata, Op,
    OpError, OpResult, PluginId, PluginOp, ProjectId, TerminalOp, ThreadId, ThreadOp,
};
use serde_json::{Value, json};

use crate::{
    Cli, Command, json_error, mcp,
    socket::{self, Client},
};

fn parse(args: &[&str]) -> Op {
    match Cli::try_parse_from(std::iter::once("kg").chain(args.iter().copied()))
        .unwrap()
        .command
    {
        Some(Command::Op(op)) => *op,
        _ => panic!("{args:?} is not an op"),
    }
}

#[test]
fn guide_names_every_command() {
    fn check(command: &clap::Command, path: &str, guide: &str) {
        let children: Vec<_> = command
            .get_subcommands()
            .filter(|child| child.get_name() != "help")
            .collect();
        if children.is_empty() && path != "kg" {
            assert!(guide.contains(&format!("`{path}`")), "missing {path}");
        }
        for child in children {
            check(child, &format!("{path} {}", child.get_name()), guide);
        }
    }
    check(
        &Cli::command(),
        "kg",
        include_str!("../../core/src/guide-map.md"),
    );
}

#[test]
fn cli_thread_terminal_parses_its_verbs() {
    let id = ThreadId("thr_1".into());
    assert_eq!(
        parse(&["thread", "terminal", "new", "thr_1"]),
        Op::Thread(ThreadOp::Terminal(TerminalOp::New { id: id.clone() }))
    );
    assert_eq!(
        parse(&["thread", "terminal", "close", "--id", "thr_1", "2"]),
        Op::Thread(ThreadOp::Terminal(TerminalOp::Close {
            id: id.clone(),
            number: 2
        }))
    );
    assert_eq!(
        parse(&["thread", "links", "thr_1"]),
        Op::Thread(ThreadOp::Links { id: id.clone() })
    );
    assert_eq!(
        parse(&["thread", "input", "thr_1", "ls", "--terminal", "2"]),
        Op::Thread(ThreadOp::Input {
            id,
            text: "ls".into(),
            terminal: Some(2),
        })
    );
}

#[test]
fn cli_diff_list_parses_scope() {
    assert_eq!(
        parse(&["diff", "list", "thr_1", "--scope", "branch"]),
        Op::Diff(DiffOp::List {
            query: DiffQuery {
                thread_id: ThreadId("thr_1".into()),
                scope: DiffScope::Branch,
                base: None,
                rev: None,
            }
        })
    );
}

#[test]
fn cli_run_parses_a_program() {
    let Op::Thread(ThreadOp::Run(run)) = parse(&[
        "thread",
        "run",
        "--project-id",
        "prj_1",
        "--program",
        "claude",
        "--environment",
        "worktree:main",
    ]) else {
        panic!("not a run");
    };
    assert_eq!(run.program, "claude");
    assert_eq!(run.project_id, ProjectId("prj_1".into()));
    assert_eq!(
        run.environment,
        EnvironmentSpec::Worktree {
            base_branch: Some("main".into()),
            branch: None,
        }
    );
    let Op::Thread(ThreadOp::Run(run)) = parse(&[
        "thread",
        "run",
        "--project-id",
        "prj_1",
        "--program",
        "codex",
        "--metadata-plugin",
        "p",
        "--metadata",
        r#"{"master":true}"#,
    ]) else {
        panic!("not a run");
    };
    assert_eq!(
        run.metadata,
        Some(Metadata {
            plugin_id: PluginId("p".into()),
            value: json!({ "master": true }),
        })
    );
    assert!(
        Cli::try_parse_from([
            "kg",
            "thread",
            "set-metadata",
            "t",
            "--metadata-plugin",
            "p"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from(["kg", "thread", "spawn", "--project-id", "p", "--input", "x"])
            .is_err()
    );
}

#[test]
fn cli_plugin_commands() {
    assert_eq!(
        parse(&[
            "plugin",
            "install",
            "git:https://x/y@v1",
            "--plugin",
            "canvas",
            "--thread-id",
            "thr_1"
        ]),
        Op::Plugin(PluginOp::Install {
            source: "git:https://x/y@v1".into(),
            plugin: Some("canvas".into()),
            thread_id: Some(ThreadId("thr_1".into())),
        })
    );
    assert_eq!(
        parse(&[
            "plugin",
            "update",
            "dev.kagan.canvas",
            "--thread-id",
            "thr_2"
        ]),
        Op::Plugin(PluginOp::Update {
            id: PluginId("dev.kagan.canvas".into()),
            thread_id: Some(ThreadId("thr_2".into())),
        })
    );
    assert_eq!(
        parse(&[
            "plugin",
            "config",
            "p",
            "--thread-id",
            "thr_3",
            "set",
            "sounds",
            "false"
        ]),
        Op::Plugin(PluginOp::Config {
            id: PluginId("p".into()),
            op: ConfigOp::Set {
                key: "sounds".into(),
                value: Some(json!(false)),
            },
            thread_id: Some(ThreadId("thr_3".into())),
        })
    );
    assert_eq!(
        parse(&[
            "plugin",
            "config",
            "p",
            "--thread-id",
            "thr_3",
            "set",
            "label",
            "compact"
        ]),
        Op::Plugin(PluginOp::Config {
            id: PluginId("p".into()),
            op: ConfigOp::Set {
                key: "label".into(),
                value: Some(json!("compact")),
            },
            thread_id: Some(ThreadId("thr_3".into())),
        })
    );
}

#[test]
fn cli_without_command_opens() {
    let cli = Cli::try_parse_from(["kg"]).unwrap();
    assert!(cli.command.is_none());
    assert!(matches!(
        Cli::try_parse_from(["kg", "mcp", "--thread", "thr_1"])
            .unwrap()
            .command,
        Some(Command::Mcp { .. })
    ));
}

fn test_seq() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(unix)]
fn test_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("kg-{label}-{}-{}", std::process::id(), test_seq()))
}

fn test_name(label: &str) -> Name<'static> {
    #[cfg(unix)]
    {
        socket::name(&socket::location_in(&test_dir(label)).unwrap()).unwrap()
    }
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        std::ffi::OsString::from(format!("kg-{label}-{}-{}", std::process::id(), test_seq()))
            .to_ns_name::<GenericNamespaced>()
            .unwrap()
    }
}

#[cfg(unix)]
#[test]
fn socket_dir_is_private_and_stale_socket_is_replaced() {
    use std::os::unix::fs::PermissionsExt;

    let dir = test_dir("stale");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let sock = dir.join("kg.sock");
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    let name = socket::name(&socket::location_in(&dir).unwrap()).unwrap();
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(sock.exists(), "stale socket file is gone");
    // Closing the listener does not make connect fail at once. bind treats a
    // successful connect as another running kg.
    let started = std::time::Instant::now();
    let listener = loop {
        if let Some(listener) = socket::bind(name.borrow()).unwrap() {
            break listener;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "stale socket still answers"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    socket::serve(listener, |_, _| Ok(OpResult::Done));
    assert_eq!(
        Client::connect(name).unwrap().call(&Op::Sidebar),
        Ok(OpResult::Done)
    );
}

#[test]
fn socket_round_trip() {
    let name = test_name("round-trip");
    let listener = socket::bind(name.borrow()).unwrap().unwrap();
    socket::serve(listener, |op, _| match op {
        Op::Sidebar => Ok(OpResult::Done),
        _ => Err(OpError::NotFound("thread thr_9 not found".into())),
    });
    let mut client = Client::connect(name.borrow()).unwrap();
    assert_eq!(client.call(&Op::Sidebar), Ok(OpResult::Done));
    assert_eq!(
        client.call(&Op::Thread(ThreadOp::Get {
            id: ThreadId("thr_9".into())
        })),
        Err(OpError::NotFound("thread thr_9 not found".into()))
    );
    assert!(
        socket::bind(name).unwrap().is_none(),
        "second instance bound"
    );
}

#[test]
fn a_client_that_leaves_closes_its_requester_while_the_op_waits() {
    use std::io::Write as _;
    use std::sync::mpsc;

    use interprocess::local_socket::traits::Stream as _;

    let name = test_name("requester-gone");
    let listener = socket::bind(name.borrow()).unwrap().unwrap();
    let (started, waiting) = mpsc::channel();
    let (ended, gone) = mpsc::channel();
    socket::serve(listener, move |_, requester| {
        started.send(requester.is_closed()).unwrap();
        ended.send(requester.recv_blocking().is_err()).unwrap();
        Ok(OpResult::Done)
    });
    let mut stream = interprocess::local_socket::Stream::connect(name).unwrap();
    let mut line = serde_json::to_vec(&Op::Sidebar).unwrap();
    line.push(b'\n');
    stream.write_all(&line).unwrap();
    let timeout = std::time::Duration::from_secs(5);
    assert!(
        !waiting.recv_timeout(timeout).unwrap(),
        "gone before leaving"
    );
    drop(stream);
    assert!(
        gone.recv_timeout(timeout).unwrap(),
        "the requester stayed open"
    );
}

#[cfg(unix)]
#[test]
fn connect_or_start_uses_the_socket_that_is_already_listening() {
    let dir = test_dir("live");
    let location = socket::location_in(&dir).unwrap();
    let name = socket::name(&location).unwrap();
    let listener = socket::bind(name.borrow()).unwrap().unwrap();
    socket::serve(listener, |_, _| Ok(OpResult::Done));
    let _guard = socket_var(&location);
    let mut client = Client::connect_or_start().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(client.call(&Op::Sidebar), Ok(OpResult::Done));
}

#[cfg(unix)]
#[test]
fn a_socket_that_appears_is_accepted_and_a_missed_deadline_is_not() {
    let dir = test_dir("late");
    let location = socket::location_in(&dir).unwrap();
    let Err(missed) = socket::wait_for_socket(&location, std::time::Duration::ZERO) else {
        panic!("a missing socket answered");
    };
    assert!(missed.raw_os_error().is_some() || missed.kind() == std::io::ErrorKind::NotFound);

    let listen_at = location.clone();
    let ready = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        socket::bind(socket::name(&listen_at).unwrap())
            .unwrap()
            .unwrap()
    });
    let client = socket::wait_for_socket(&location, std::time::Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("{error}"));
    drop(client);
    drop(ready.join().unwrap());
}

#[cfg(unix)]
struct SocketVar(Option<std::ffi::OsString>);

#[cfg(unix)]
fn socket_var(location: &std::ffi::OsStr) -> SocketVar {
    let previous = std::env::var_os("KG_SOCKET");
    unsafe { std::env::set_var("KG_SOCKET", location) };
    SocketVar(previous)
}

#[cfg(unix)]
impl Drop for SocketVar {
    fn drop(&mut self) {
        unsafe {
            match self.0.take() {
                Some(value) => std::env::set_var("KG_SOCKET", value),
                None => std::env::remove_var("KG_SOCKET"),
            }
        }
    }
}

fn fake(op: Op) -> Result<OpResult, OpError> {
    match op {
        Op::Thread(ThreadOp::Tools { .. }) => Ok(OpResult::Tools {
            instructions: Some("Use editor_open to show a file.".into()),
            tools: vec![AgentTool {
                name: "editor_open".into(),
                description: "Open a file".into(),
                input_schema: json!({ "type": "object" }),
            }],
        }),
        Op::Thread(ThreadOp::CallTool { name, input, .. }) if name == "editor_open" => {
            Ok(OpResult::ToolReply {
                text: format!("opened {}", input["path"]),
                is_error: false,
            })
        }
        _ => Err(OpError::NotFound("no such tool".into())),
    }
}

fn mcp(requests: &[Value]) -> Vec<Value> {
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = Vec::new();
    mcp::run(
        &ThreadId("thr_1".into()),
        input.as_bytes(),
        &mut output,
        fake,
    )
    .unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn mcp_legacy_session() {
    let replies = mcp(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "capabilities": {} } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "editor_open", "arguments": { "path": "a.rs" } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "missing" } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" }),
    ]);
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        replies[0]["result"]["instructions"],
        "Use editor_open to show a file."
    );
    assert_eq!(
        replies[1]["result"]["tools"],
        json!([{ "name": "editor_open", "description": "Open a file",
                 "inputSchema": { "type": "object" } }])
    );
    assert_eq!(
        replies[2]["result"]["content"],
        json!([{ "type": "text", "text": "opened \"a.rs\"" }])
    );
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(replies[3]["result"]["content"][0]["text"], "no such tool");
    assert_eq!(replies[4]["error"]["code"], -32601);
}

#[test]
fn mcp_modern_requests() {
    let meta = |version: &str| json!({ "io.modelcontextprotocol/protocolVersion": version });
    let replies = mcp(&[
        json!({ "jsonrpc": "2.0", "id": "d", "method": "server/discover",
                "params": { "_meta": meta("2026-07-28") } }),
        json!({ "jsonrpc": "2.0", "id": "old", "method": "tools/list",
                "params": { "_meta": meta("1900-01-01") } }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "initialize",
                "params": { "protocolVersion": "1900-01-01" } }),
    ]);
    let discover = &replies[0]["result"];
    assert_eq!(discover["resultType"], "complete");
    assert_eq!(discover["supportedVersions"][0], "2026-07-28");
    assert_eq!(discover["instructions"], "Use editor_open to show a file.");
    assert_eq!(discover["ttlMs"], 0);
    assert_eq!(replies[1]["id"], "old");
    assert_eq!(replies[1]["error"]["code"], -32022);
    assert_eq!(
        replies[1]["error"]["data"]["supported"],
        json!(["2026-07-28"])
    );
    assert_eq!(replies[2]["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn mcp_bad_json() {
    let mut output = Vec::new();
    mcp::run(
        &ThreadId("thr_1".into()),
        "not json\n".as_bytes(),
        &mut output,
        fake,
    )
    .unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(reply["id"], Value::Null);
    assert_eq!(reply["error"]["code"], -32700);
}

#[test]
fn self_extension_commands_derive_from_ops() {
    assert_eq!(
        parse(&["guide", "skills"]),
        Op::Guide {
            chapter: Some(kg_domain::GuideChapter::Skills)
        }
    );
    assert_eq!(
        parse(&[
            "skill",
            "new",
            "my-task",
            "--project",
            "--project-id",
            "prj_1"
        ]),
        Op::Skill(kg_domain::SkillOp::New {
            name: "my-task".into(),
            project: true,
            project_id: Some(ProjectId("prj_1".into()))
        })
    );
    assert_eq!(
        parse(&["plugin", "dev", "./plugin", "--thread-id", "thr_1"]),
        Op::Plugin(PluginOp::Dev {
            path: "./plugin".into(),
            thread_id: Some(ThreadId("thr_1".into()))
        })
    );
    assert_eq!(
        parse(&["plugin", "logs", "dev.test.plugin", "--follow"]),
        Op::Plugin(PluginOp::Logs {
            id: PluginId("dev.test.plugin".into()),
            follow: true,
            after: None
        })
    );
}

#[test]
fn mcp_passes_skill_load_through_the_thread_context() {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "skill_load", "arguments": { "name": "kg-cli" } } }).to_string();
    let mut output = Vec::new();
    mcp::run(
        &ThreadId("thr_1".into()),
        request.as_bytes(),
        &mut output,
        |op| {
            assert_eq!(
                op,
                Op::Thread(ThreadOp::CallTool {
                    id: ThreadId("thr_1".into()),
                    name: "skill_load".into(),
                    input: json!({"name": "kg-cli"})
                })
            );
            Ok(OpResult::ToolReply {
                text: "skill body and files".into(),
                is_error: false,
            })
        },
    )
    .unwrap();
    let response: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        response["result"]["content"][0]["text"],
        "skill body and files"
    );
}

#[test]
fn a_missing_id_names_the_variable_that_would_fill_it() {
    match std::env::var("KG_TEST_ID_HINT").ok().as_deref() {
        Some("set") => {
            let cli = Cli::try_parse_from(["kg", "--json", "thread", "get"]).unwrap();
            let Some(Command::Op(op)) = cli.command else {
                panic!("get");
            };
            assert!(cli.json);
            assert_eq!(
                *op,
                Op::Thread(ThreadOp::Get {
                    id: ThreadId("thr_1".into()),
                })
            );
        }
        Some("missing") => {
            for (args, variable) in [
                (&["thread", "get"][..], "KG_THREAD_ID"),
                (&["project", "get"][..], "KG_PROJECT_ID"),
                (&["environment", "get"][..], "KG_ENVIRONMENT_ID"),
            ] {
                let Err(error) =
                    Cli::try_parse_from(std::iter::once("kg").chain(args.iter().copied()))
                else {
                    panic!("{args:?} parsed without an id");
                };
                let value: Value = serde_json::from_str(&json_error(&error)).unwrap();
                assert_eq!(value["error"]["kind"], "invalid");
                assert!(
                    value["error"]["message"]
                        .as_str()
                        .is_some_and(|text| !text.is_empty())
                );
                assert_eq!(
                    value["error"]["hint"],
                    format!("set {variable}, or pass the id")
                );
                assert!(value.get("ok").is_none());
            }
            let Err(error) = Cli::try_parse_from(["kg", "thread", "run"]) else {
                panic!("thread run parsed without its arguments");
            };
            let value: Value = serde_json::from_str(&json_error(&error)).unwrap();
            assert!(value["error"].get("hint").is_none(), "{value}");
        }
        _ => {
            let run = |mode: &str, thread: Option<&str>| {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .env("KG_TEST_ID_HINT", mode)
                    .env_remove("KG_PROJECT_ID")
                    .env_remove("KG_ENVIRONMENT_ID")
                    .args([
                        "--exact",
                        "tests::a_missing_id_names_the_variable_that_would_fill_it",
                    ]);
                if let Some(id) = thread {
                    command.env("KG_THREAD_ID", id);
                } else {
                    command.env_remove("KG_THREAD_ID");
                }
                let output = command.output().unwrap();
                assert!(
                    output.status.success(),
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            };
            run("set", Some("thr_1"));
            run("missing", None);
        }
    }
}

#[test]
fn thread_tool_defaults_leave_the_tool_name_positional() {
    if std::env::var_os("KG_TEST_DEFAULTS").is_some() {
        let cli = Cli::try_parse_from(["kg", "thread", "call-tool", "echo"]).unwrap();
        let Some(Command::Op(op)) = cli.command else {
            panic!("call-tool is not an op");
        };
        let Op::Thread(ThreadOp::CallTool { id, name, .. }) = op.as_ref() else {
            panic!("call-tool is not a thread tool");
        };
        assert_eq!(*id, ThreadId("thr_1".into()));
        assert_eq!(name, "echo");
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .env("KG_TEST_DEFAULTS", "1")
        .env("KG_THREAD_ID", "thr_1")
        .args([
            "--exact",
            "tests::thread_tool_defaults_leave_the_tool_name_positional",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_reference_matches_help() {
    let mut root = Cli::command().styles(Styles::plain()).term_width(100);
    root.set_bin_name("kg");
    let mut discard = Vec::new();
    root.write_long_help(&mut discard).expect("help");

    let mut out = String::from(
        "# `kg`\n\nThis page is generated from `kg --help`.\n\nEvery command accepts `--json`. A failure prints the error and exits non-zero. `kg mcp` is how the agent connects. You rarely type it.\n\n",
    );
    append_help(&root, "kg", &mut out);

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../book/src/cli.md");
    if std::env::var_os("KG_UPDATE_CLI").is_some() {
        std::fs::write(&path, &out).expect("write cli.md");
    }
    let on_disk = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        on_disk, out,
        "book/src/cli.md does not match clap help; rerun with KG_UPDATE_CLI=1"
    );
}

fn append_help(cmd: &clap::Command, path: &str, out: &mut String) {
    let mut built = cmd.clone().styles(Styles::plain()).term_width(100);
    built.set_bin_name(path);
    let mut buf = Vec::new();
    built.write_long_help(&mut buf).expect("help");
    let help = String::from_utf8(buf).expect("utf-8").replace("\r\n", "\n");
    out.push_str("## `");
    out.push_str(path);
    out.push_str("`\n\n```text\n");
    out.push_str(help.trim_end());
    out.push_str("\n```\n\n");
    if cmd.get_name() == "help" {
        return;
    }
    if path == "kg"
        && let Some(help) = cmd.find_subcommand("help")
    {
        append_help(help, "kg help", out);
    }
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        append_help(sub, &format!("{path} {}", sub.get_name()), out);
    }
}

#[test]
fn unknown_subcommands_go_to_plugins() {
    let cli = Cli::try_parse_from(["kg", "probe", "say", "a", "b"]).unwrap();
    let Some(Command::Plugin(words)) = cli.command else {
        panic!("plugin command")
    };
    assert_eq!(words, ["probe", "say", "a", "b"]);
}

#[test]
fn a_mistyped_built_in_gets_a_hint_and_a_plugin_command_does_not() {
    use std::ffi::OsString;
    let words = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(
        crate::suggestion(&words(&["projcts", "list"])).as_deref(),
        Some("project")
    );
    assert_eq!(crate::suggestion(&words(&["zzz", "say"])), None);
}

#[test]
fn release_ms_does_not_enable_gpui_kit_test_support_on_kg_ui() {
    // A release build does not change feature resolution. The normal tree is the check.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("cargo")
        .current_dir(root)
        .args([
            "tree",
            "-e",
            "normal,features",
            "-p",
            "kg-ui",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo tree");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        assert!(
            !(line.contains("gpui-kit") && line.contains("test-support")),
            "kg-ui enables gpui-kit test-support in normal deps: {line}"
        );
    }
}

#[test]
fn cli_talks_to_a_threads_terminal() {
    let id = ThreadId("thr_1".into());
    assert_eq!(
        parse(&["thread", "input", "thr_1", "ls\r"]),
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            text: "ls\r".into(),
            terminal: None,
        })
    );
    assert_eq!(
        parse(&["thread", "resize", "thr_1", "100", "40"]),
        Op::Thread(ThreadOp::Resize {
            id: id.clone(),
            cols: 100,
            rows: 40,
            terminal: None,
        })
    );
    assert_eq!(
        parse(&["thread", "rename", "--id", "thr_1", "New title"]),
        Op::Thread(ThreadOp::Rename {
            id: id.clone(),
            title: "New title".into()
        })
    );
    assert_eq!(
        parse(&["thread", "screen", "thr_1"]),
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            scrollback: false,
            lines: None,
            terminal: None,
        })
    );
    assert_eq!(
        parse(&["thread", "screen", "thr_1", "--scrollback", "--lines", "20"]),
        Op::Thread(ThreadOp::Screen {
            id,
            scrollback: true,
            lines: Some(20),
            terminal: None,
        })
    );
}

#[test]
fn thread_search_takes_text_a_limit_and_archived() {
    assert_eq!(
        parse(&[
            "thread",
            "search",
            "cargo test",
            "--limit",
            "5",
            "--archived"
        ]),
        Op::Thread(ThreadOp::Search {
            text: "cargo test".into(),
            limit: Some(5),
            archived: true,
        })
    );
    assert_eq!(
        parse(&["thread", "search", "error"]),
        Op::Thread(ThreadOp::Search {
            text: "error".into(),
            limit: None,
            archived: false,
        })
    );
    let found = serde_json::to_value(OpResult::Found(vec![kg_domain::Found {
        thread_id: ThreadId("thr_1".into()),
        title: "Build".into(),
        project_id: ProjectId("prj_1".into()),
        project: "kagan".into(),
        archived: false,
        snippet: Some(kg_domain::Snippet {
            text: "error: failed".into(),
            scroll: 3,
        }),
    }]))
    .unwrap();
    assert_eq!(
        found,
        json!({"found": [{
            "threadId": "thr_1",
            "title": "Build",
            "projectId": "prj_1",
            "project": "kagan",
            "archived": false,
            "snippet": {"text": "error: failed", "scroll": 3}
        }]})
    );
}

#[test]
fn a_setting_left_off_the_command_line_is_read_as_text_and_never_parsed() {
    use crate::development::take_value_with;
    let set = |value| {
        Op::Plugin(PluginOp::Config {
            id: PluginId("p".into()),
            op: ConfigOp::Set {
                key: "token".into(),
                value,
            },
            thread_id: None,
        })
    };
    let typed = parse(&["plugin", "config", "p", "set", "token"]);
    assert_eq!(typed, set(None));
    let read = take_value_with(typed.clone(), |key| {
        assert_eq!(key, "token");
        Ok("12345\r\n".into())
    });
    assert_eq!(read.unwrap(), set(Some(json!("12345"))));
    let empty = take_value_with(typed, |_| Ok("\n".into())).unwrap_err();
    assert!(
        empty.to_string().contains("type it at the prompt"),
        "{empty}"
    );
    let given = set(Some(json!(false)));
    assert_eq!(
        take_value_with(given.clone(), |_| panic!("not read")).unwrap(),
        given
    );
    assert_eq!(
        parse(&["plugin", "config", "p", "clear", "token"]),
        Op::Plugin(PluginOp::Config {
            id: PluginId("p".into()),
            op: ConfigOp::Clear {
                key: "token".into()
            },
            thread_id: None,
        })
    );
}

#[test]
fn paste_reads_a_file_or_stdin_and_sends_the_text() {
    use crate::development::take_paste_from;
    let id = ThreadId("thr_1".into());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    let body = "say \"hi\" and `date`";
    std::fs::write(&path, body).unwrap();
    let from_file = take_paste_from(
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: None,
            file: Some(path.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap();
    let sent = Op::Thread(ThreadOp::Paste {
        id: id.clone(),
        text: Some(body.into()),
        file: None,
        terminal: None,
    });
    assert_eq!(from_file, sent);
    assert_eq!(
        take_paste_from(
            Op::Thread(ThreadOp::Paste {
                id: id.clone(),
                text: None,
                file: Some("-".into()),
                terminal: None,
            }),
            std::io::Cursor::new(body),
        )
        .unwrap(),
        sent
    );
    let unchanged = Op::Thread(ThreadOp::Paste {
        id: id.clone(),
        text: Some("hello".into()),
        file: None,
        terminal: None,
    });
    assert_eq!(
        take_paste_from(unchanged.clone(), std::io::empty()).unwrap(),
        unchanged
    );
    assert_eq!(
        parse(&["thread", "paste", "thr_1", "hello"]),
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: Some("hello".into()),
            file: None,
            terminal: None,
        })
    );
    assert_eq!(
        parse(&["thread", "paste", "thr_1", "--file", "-"]),
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: None,
            file: Some("-".into()),
            terminal: None,
        })
    );
    assert!(
        Cli::try_parse_from(["kg", "thread", "paste", "thr_1", "hello", "--file", "a"]).is_err()
    );
    let big = dir.path().join("big");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(1024 * 1024 + 8)
        .unwrap();
    let err = take_paste_from(
        Op::Thread(ThreadOp::Paste {
            id,
            text: None,
            file: Some(big.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(matches!(err, OpError::Invalid(_)));
    assert!(message.contains("1048584"), "{message}");
    assert!(message.contains("1 MiB"), "{message}");
}

#[test]
fn paste_rejects_invalid_utf8_from_a_file_or_stdin() {
    use crate::development::take_paste_from;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad");
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    let pasted = Op::Thread(ThreadOp::Paste {
        id: ThreadId("thr_1".into()),
        text: None,
        file: Some(path.display().to_string()),
        terminal: None,
    });
    let from_file = take_paste_from(pasted, std::io::empty()).unwrap_err();
    assert!(from_file.to_string().contains("UTF-8"), "{from_file}");
    let from_stdin = take_paste_from(
        Op::Thread(ThreadOp::Paste {
            id: ThreadId("thr_1".into()),
            text: None,
            file: Some("-".into()),
            terminal: None,
        }),
        std::io::Cursor::new([0xff, 0xfe]),
    )
    .unwrap_err();
    assert!(from_stdin.to_string().contains("UTF-8"), "{from_stdin}");
}

#[cfg(unix)]
#[test]
fn paste_rejects_a_fifo_past_the_cap() {
    use std::io::Write;

    use crate::development::take_paste_from;
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("paste.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let writing = fifo.clone();
    let writer = std::thread::spawn(move || {
        let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(&writing) else {
            return;
        };
        let chunk = vec![b'a'; 64 * 1024];
        let mut sent = 0usize;
        while sent < 2 * 1024 * 1024 {
            if file.write_all(&chunk).is_err() {
                break;
            }
            sent += chunk.len();
        }
    });
    let streamed = take_paste_from(
        Op::Thread(ThreadOp::Paste {
            id: ThreadId("thr_1".into()),
            text: None,
            file: Some(fifo.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap_err();
    let _ = writer.join();
    let streamed = streamed.to_string();
    assert!(streamed.contains("1 MiB"), "{streamed}");
    assert!(!streamed.contains("2097152"), "{streamed}");
}

#[test]
fn install_sends_a_bare_name_to_the_indexes_and_a_folder_as_its_path() {
    use crate::development::paths;
    let source = |args: &[&str]| match paths(parse(args)).unwrap() {
        Op::Plugin(PluginOp::Install { source, .. }) => source,
        Op::Plugin(PluginOp::Index {
            op: IndexOp::Add { location },
        }) => location,
        Op::Plugin(PluginOp::Search { text }) => text,
        op => panic!("{op:?}"),
    };
    assert_eq!(
        source(&["plugin", "install", "no-such-folder-here"]),
        "no-such-folder-here"
    );
    let folder = std::env::current_dir().unwrap().join(".");
    assert_eq!(
        source(&["plugin", "install", "."]),
        folder.display().to_string()
    );
    assert_eq!(
        source(&["plugin", "index", "add", "."]),
        folder.display().to_string()
    );
    assert_eq!(
        source(&["plugin", "search"]),
        String::new(),
        "search with no text lists every plugin"
    );
}
