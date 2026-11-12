use clap::Parser;
use std::path::PathBuf;

use kg_domain::{
    AutomationId, AutomationOp, ConfigOp, DiffOp, DiffQuery, DiffScope, EnvironmentSpec, Metadata,
    Op, OpResult, PluginId, PluginOp, ProgramOp, ProjectId, ProjectSection, SavedPromptId,
    SavedPromptOp, SectionId, SectionOp, SettingsOp, SkillOp, TerminalOp, ThreadId, ThreadOp,
};
use serde_json::{Value, json};

use crate::{Cli, Command, json_error};

use super::socket::{drop_inherited_ids, set_env};

pub(super) fn parse(args: &[&str]) -> Op {
    drop_inherited_ids();
    match Cli::try_parse_from(std::iter::once("kg").chain(args.iter().copied()))
        .unwrap()
        .command
    {
        Some(Command::Op(op)) => *op,
        _ => panic!("{args:?} is not an op"),
    }
}

#[test]
fn kg_update_checks_and_reads_status_but_cannot_install() {
    assert_eq!(
        parse(&["update", "check"]),
        Op::Update(kg_domain::UpdateOp::Check)
    );
    assert_eq!(
        parse(&["update", "status"]),
        Op::Update(kg_domain::UpdateOp::Status)
    );
    assert!(Cli::try_parse_from(["kg", "update", "install"]).is_err());
}

#[test]
fn cli_program_keeps_a_line_with_its_flags_as_one_name_and_one_line() {
    let line = "claude --model claude-opus-5-5 --effort high";
    assert_eq!(
        parse(&["program", "save", "Claude, careful", line]),
        Op::Program(ProgramOp::Save {
            name: "Claude, careful".into(),
            line: line.into(),
        })
    );
    assert_eq!(
        parse(&["program", "pin", "Claude, careful", "true"]),
        Op::Program(ProgramOp::Pin {
            name: "Claude, careful".into(),
            pinned: true,
        })
    );
    assert!(Cli::try_parse_from(["kg", "program", "pin", "Claude, careful"]).is_err());
}

#[test]
fn cli_thread_terminal_parses_its_verbs() {
    let id = ThreadId("thr_1".into());
    assert_eq!(
        parse(&["thread", "terminal", "new", "thr_1"]),
        Op::Thread(ThreadOp::Terminal(TerminalOp::New { id: id.clone() }))
    );
    assert_eq!(
        parse(&["thread", "terminal", "close", "thr_1", "2"]),
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

const TERMINAL_COMMANDS: [(&[&str], &str); 16] = [
    (
        &["input", "thr_1", "ls"],
        r#"{"input":{"id":"thr_1","text":"ls"}}"#,
    ),
    (
        &["input", "thr_1", "ls", "--terminal", "2"],
        r#"{"input":{"id":"thr_1","terminal":2,"text":"ls"}}"#,
    ),
    (
        &["resize", "thr_1", "100", "40"],
        r#"{"resize":{"id":"thr_1","cols":100,"rows":40}}"#,
    ),
    (
        &["resize", "thr_1", "100", "40", "--terminal", "2"],
        r#"{"resize":{"id":"thr_1","terminal":2,"cols":100,"rows":40}}"#,
    ),
    (
        &["screen", "thr_1"],
        r#"{"screen":{"id":"thr_1","scrollback":false}}"#,
    ),
    (
        &[
            "screen",
            "thr_1",
            "--terminal",
            "2",
            "--scrollback",
            "--lines",
            "20",
        ],
        r#"{"screen":{"id":"thr_1","terminal":2,"scrollback":true,"lines":20}}"#,
    ),
    (
        &["scroll", "thr_1", "-3"],
        r#"{"scroll":{"id":"thr_1","lines":-3}}"#,
    ),
    (
        &["scroll", "thr_1", "5", "--terminal", "2"],
        r#"{"scroll":{"id":"thr_1","terminal":2,"lines":5}}"#,
    ),
    (
        &["select", "thr_1", "word", "4", "1"],
        r#"{"select":{"id":"thr_1","mode":"word","column":4,"row":1,"right":false}}"#,
    ),
    (
        &[
            "select",
            "thr_1",
            "extend",
            "4",
            "1",
            "--right",
            "--terminal",
            "2",
        ],
        r#"{"select":{"id":"thr_1","terminal":2,"mode":"extend","column":4,"row":1,"right":true}}"#,
    ),
    (&["copy", "thr_1"], r#"{"copy":{"id":"thr_1"}}"#),
    (
        &["copy", "thr_1", "--terminal", "2"],
        r#"{"copy":{"id":"thr_1","terminal":2}}"#,
    ),
    (
        &["report", "thr_1", "27", "91", "77"],
        r#"{"report":{"id":"thr_1","bytes":[27,91,77]}}"#,
    ),
    (
        &["report", "thr_1", "27", "91", "77", "--terminal", "2"],
        r#"{"report":{"id":"thr_1","terminal":2,"bytes":[27,91,77]}}"#,
    ),
    (
        &["paste", "thr_1", "hi"],
        r#"{"paste":{"id":"thr_1","text":"hi"}}"#,
    ),
    (
        &["paste", "thr_1", "--file", "-", "--terminal", "2"],
        r#"{"paste":{"id":"thr_1","terminal":2,"file":"-"}}"#,
    ),
];

#[test]
fn terminal_commands_parse_to_their_exact_json() {
    let _thread = set_env("KG_THREAD_ID", std::ffi::OsStr::new("thr_env"));
    for (args, wire) in TERMINAL_COMMANDS {
        let with_id = [&["thread"], args].concat();
        let Op::Thread(op) = parse(&with_id) else {
            panic!("{args:?} is not a thread op");
        };
        assert_eq!(serde_json::to_string(&op).unwrap(), wire, "{args:?}");

        if args[2..]
            .first()
            .is_some_and(|word| !word.starts_with("--"))
        {
            continue;
        }
        let without_id: Vec<&str> = with_id
            .iter()
            .copied()
            .filter(|word| *word != "thr_1")
            .collect();
        let Op::Thread(op) = parse(&without_id) else {
            panic!("{without_id:?} is not a thread op");
        };
        assert_eq!(
            serde_json::to_string(&op).unwrap(),
            wire.replace("thr_1", "thr_env"),
            "{without_id:?}"
        );
    }
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
        "--plugin-id",
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
        Cli::try_parse_from(["kg", "thread", "set-metadata", "t", "--plugin-id", "p"]).is_err()
    );
    assert!(
        Cli::try_parse_from(["kg", "thread", "spawn", "--project-id", "p", "--input", "x"])
            .is_err()
    );
}

#[test]
fn cli_config_set_takes_negative_numbers_without_a_separator() {
    for text in ["-0", "-2.5", "-1e2"] {
        let Op::Plugin(PluginOp::Config {
            op: ConfigOp::Set { value, .. },
            ..
        }) = parse(&["plugin", "config", "p", "set", "retries", text])
        else {
            panic!("{text} is not a config set");
        };
        assert_eq!(value, Some(serde_json::from_str(text).unwrap()), "{text}");
    }
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
            target: kg_domain::UpdateTarget::One(PluginId("dev.kagan.canvas".into())),
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
            template: kg_domain::SkillTemplate::Blank,
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
    let wire =
        serde_json::to_value(parse(&["plugin", "logs", "dev.test.plugin", "--follow"])).unwrap();
    assert!(wire["plugin"]["logs"].get("follow").is_none());
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
                let argv: Vec<String> = std::iter::once("kg")
                    .chain(args.iter().copied())
                    .map(String::from)
                    .collect();
                let Err(error) = Cli::try_parse_from(argv.clone()) else {
                    panic!("{args:?} parsed without an id");
                };
                let value: Value = serde_json::from_str(&json_error(&error, &argv)).unwrap();
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
            let argv = ["kg", "thread", "run"].map(String::from);
            let Err(error) = Cli::try_parse_from(argv.clone()) else {
                panic!("thread run parsed without its arguments");
            };
            let value: Value = serde_json::from_str(&json_error(&error, &argv)).unwrap();
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
                        "tests::cli::a_missing_id_names_the_variable_that_would_fill_it",
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
fn thread_tool_takes_its_id_and_name_positionally() {
    if std::env::var_os("KG_TEST_DEFAULTS").is_some() {
        let cli = Cli::try_parse_from(["kg", "thread", "call-tool", "thr_1", "echo"]).unwrap();
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
        .args([
            "--exact",
            "tests::cli::thread_tool_takes_its_id_and_name_positionally",
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
        parse(&["thread", "rename", "thr_1", "New title"]),
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
fn cli_automations_parse_next_to_thread() {
    let project = ProjectId("prj_1".into());
    let id = AutomationId("auto_1".into());
    assert_eq!(
        parse(&[
            "automation",
            "add",
            "Morning",
            "ping",
            "--project-id",
            "prj_1",
            "--program",
            "claude",
            "--minutes",
            "15"
        ]),
        Op::Automation(AutomationOp::Add {
            name: "Morning".into(),
            body: "ping".into(),
            project_id: project,
            program: "claude".into(),
            minutes: Some(15),
            in_minutes: None,
        })
    );
    assert_eq!(
        parse(&["automation", "pause", "auto_1"]),
        Op::Automation(AutomationOp::Pause { id: id.clone() })
    );
    assert_eq!(
        parse(&["automation", "resume", "auto_1"]),
        Op::Automation(AutomationOp::Resume { id: id.clone() })
    );
    assert_eq!(
        parse(&["automation", "delete", "auto_1"]),
        Op::Automation(AutomationOp::Delete { id })
    );
    assert_eq!(
        parse(&["automation", "list"]),
        Op::Automation(AutomationOp::List { project_id: None })
    );
    assert_eq!(
        parse(&[
            "automation",
            "add",
            "Once",
            "ping",
            "--program",
            "codex",
            "--in",
            "5",
            "--project-id",
            "prj_1"
        ]),
        Op::Automation(AutomationOp::Add {
            name: "Once".into(),
            body: "ping".into(),
            project_id: ProjectId("prj_1".into()),
            program: "codex".into(),
            minutes: None,
            in_minutes: Some(5),
        })
    );
    // One unit for a one-time run: minutes from now, as the window has it.
    assert!(
        Cli::try_parse_from([
            "kg",
            "automation",
            "add",
            "Once",
            "ping",
            "--program",
            "codex",
            "--at",
            "1000",
            "--project-id",
            "prj_1"
        ])
        .is_err()
    );
}

#[test]
fn cli_sections_parse_their_verbs() {
    let thread = ThreadId("thr_1".into());
    let section = SectionId("sec_1".into());
    assert_eq!(
        parse(&["section", "create", "Reviews"]),
        Op::Section(SectionOp::Create {
            name: "Reviews".into(),
            thread_id: None,
        })
    );
    assert_eq!(
        parse(&["section", "create", "Reviews", "--thread-id", "thr_1"]),
        Op::Section(SectionOp::Create {
            name: "Reviews".into(),
            thread_id: Some(thread.clone()),
        })
    );
    assert_eq!(
        parse(&["section", "rename", "sec_1", "Review"]),
        Op::Section(SectionOp::Rename {
            id: section.clone(),
            name: "Review".into(),
        })
    );
    assert_eq!(
        parse(&["section", "delete", "sec_1"]),
        Op::Section(SectionOp::Delete {
            id: section.clone(),
        })
    );
    assert_eq!(
        parse(&[
            "section",
            "file",
            "--thread-id",
            "thr_1",
            "--section-id",
            "sec_1"
        ]),
        Op::Section(SectionOp::File {
            thread_id: thread.clone(),
            section_id: Some(section),
        })
    );
    assert_eq!(
        parse(&["section", "file", "--thread-id", "thr_1"]),
        Op::Section(SectionOp::File {
            thread_id: thread,
            section_id: None,
        })
    );
}

#[test]
fn cli_skills_parse_their_verbs() {
    let project = ProjectId("prj_1".into());
    assert_eq!(
        parse(&["skill", "list"]),
        Op::Skill(SkillOp::List { project_id: None })
    );
    assert_eq!(
        parse(&["skill", "list", "--project-id", "prj_1"]),
        Op::Skill(SkillOp::List {
            project_id: Some(project.clone()),
        })
    );
    assert_eq!(
        parse(&["skill", "show", "kg-cli"]),
        Op::Skill(SkillOp::Show {
            name: "kg-cli".into(),
            project_id: None,
        })
    );
    assert_eq!(
        parse(&["skill", "files", "kg-cli", "--project-id", "prj_1"]),
        Op::Skill(SkillOp::Files {
            name: "kg-cli".into(),
            project_id: Some(project.clone()),
        })
    );
    assert_eq!(
        parse(&["skill", "new", "my-task", "--project"]),
        Op::Skill(SkillOp::New {
            name: "my-task".into(),
            project: true,
            template: kg_domain::SkillTemplate::Blank,
            project_id: None,
        })
    );
    assert_eq!(
        parse(&["skill", "add", "./skill", "--project-id", "prj_1"]),
        Op::Skill(SkillOp::Add {
            path: PathBuf::from("./skill"),
            project: false,
            project_id: Some(project),
        })
    );
    assert_eq!(
        parse(&["skill", "remove", "my-task"]),
        Op::Skill(SkillOp::Remove {
            name: "my-task".into(),
            project_id: None,
        })
    );
}

#[test]
fn cli_browser_verbs_parse_and_take_the_thread_from_the_environment() {
    let thread = std::ffi::OsStr::new("thr_browser");
    let _env = set_env("KG_THREAD_ID", thread);
    let id = ThreadId("thr_browser".into());
    assert_eq!(
        parse(&["browser", "open", "example.com"]),
        Op::Browser(kg_domain::BrowserOp::Open {
            url: "example.com".into(),
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "status"]),
        Op::Browser(kg_domain::BrowserOp::Status {
            thread_id: id.clone()
        })
    );
    assert_eq!(
        parse(&["browser", "snapshot"]),
        Op::Browser(kg_domain::BrowserOp::Snapshot {
            thread_id: id.clone()
        })
    );
    assert_eq!(
        parse(&["browser", "goto", "https://example.org"]),
        Op::Browser(kg_domain::BrowserOp::Goto {
            url: "https://example.org".into(),
            thread_id: id.clone(),
        })
    );
    for (verb, want) in [
        (
            "back",
            kg_domain::BrowserOp::Back {
                thread_id: id.clone(),
            },
        ),
        (
            "forward",
            kg_domain::BrowserOp::Forward {
                thread_id: id.clone(),
            },
        ),
        (
            "reload",
            kg_domain::BrowserOp::Reload {
                thread_id: id.clone(),
            },
        ),
        (
            "close",
            kg_domain::BrowserOp::Close {
                thread_id: id.clone(),
            },
        ),
    ] {
        assert_eq!(parse(&["browser", verb]), Op::Browser(want), "{verb}");
    }
}

#[test]
fn cli_browser_acting_verbs_parse_and_take_the_thread_from_the_environment() {
    let thread = std::ffi::OsStr::new("thr_browser");
    let _env = set_env("KG_THREAD_ID", thread);
    let id = ThreadId("thr_browser".into());
    assert_eq!(
        parse(&["browser", "click", "2"]),
        Op::Browser(kg_domain::BrowserOp::Click {
            ref_id: 2,
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "fill", "2", "ada@example.com"]),
        Op::Browser(kg_domain::BrowserOp::Fill {
            ref_id: 2,
            text: Some("ada@example.com".into()),
            file: None,
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "fill", "3", "--file", "-"]),
        Op::Browser(kg_domain::BrowserOp::Fill {
            ref_id: 3,
            text: None,
            file: Some(PathBuf::from("-")),
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "type", "hello"]),
        Op::Browser(kg_domain::BrowserOp::Type {
            text: "hello".into(),
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "press", "enter"]),
        Op::Browser(kg_domain::BrowserOp::Press {
            key: "enter".into(),
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "select", "4", "Blue"]),
        Op::Browser(kg_domain::BrowserOp::Select {
            ref_id: 4,
            option: "Blue".into(),
            thread_id: id.clone(),
        })
    );
    assert_eq!(
        parse(&["browser", "scroll", "-200"]),
        Op::Browser(kg_domain::BrowserOp::Scroll {
            dy: -200.0,
            thread_id: id.clone(),
        })
    );
}

#[test]
fn cli_browser_text_omits_an_empty_title_and_text() {
    let snapshot = kg_domain::BrowserSnapshot {
        url: "https://example.com/login".into(),
        title: String::new(),
        controls: vec![kg_domain::BrowserControl {
            ref_id: 2,
            role: "textbox".into(),
            name: "Email".into(),
            value: Some("ada@example.com".into()),
            focused: false,
            disabled: false,
            checked: None,
        }],
        text: String::new(),
        truncated: false,
    };
    assert_eq!(
        crate::cli::format_browser_snapshot(&snapshot),
        "https://example.com/login\n[2] textbox \"Email\" value=\"ada@example.com\"\n"
    );
}

#[test]
fn ui_scroll_takes_a_negative_pixel_count_without_a_separator() {
    assert_eq!(
        parse(&["ui", "scroll", "list", "-300"]),
        Op::Ui(kg_domain::UiOp::Scroll {
            element_id: "list".into(),
            dy: -300.,
            within: None,
            window: 0,
        })
    );
}

#[test]
fn env_set_given_a_value_argument_says_the_value_comes_on_stdin() {
    let argv: Vec<String> = ["kg", "env", "set", "NAME", "value"]
        .map(String::from)
        .into();
    let error = Cli::try_parse_from(&argv)
        .err()
        .expect("a value argument is unexpected");
    let hint = crate::hint(&error, &argv).expect("a hint for the value");
    assert!(hint.contains("stdin"), "{hint}");
}

#[test]
fn text_ends_with_one_newline_in_a_terminal_and_is_unchanged_in_a_pipe() {
    let mut out = Vec::new();
    crate::cli::write_text(&mut out, "rules", true).unwrap();
    assert_eq!(out, b"rules\n");
    out.clear();
    crate::cli::write_text(&mut out, "rules\n", true).unwrap();
    assert_eq!(out, b"rules\n");
    out.clear();
    crate::cli::write_text(&mut out, "rules", false).unwrap();
    assert_eq!(out, b"rules");
}

#[cfg(unix)]
#[test]
fn a_word_that_is_not_utf8_becomes_lossy_text_instead_of_a_panic() {
    use std::os::unix::ffi::OsStringExt as _;
    let words = crate::argv_words([
        std::ffi::OsString::from("kg"),
        std::ffi::OsString::from_vec(vec![0xff, b'a']),
    ]);
    assert_eq!(words, ["kg", "\u{fffd}a"]);
}

#[test]
fn cli_saved_prompt_edit_takes_the_id_then_the_title_and_text() {
    assert_eq!(
        parse(&["saved-prompt", "edit", "svp_1", "Plans", "new text"]),
        Op::SavedPrompt(SavedPromptOp::Edit {
            id: SavedPromptId("svp_1".into()),
            title: "Plans".into(),
            body: "new text".into(),
        })
    );
}

#[test]
fn cli_settings_project_can_open_its_automations() {
    assert_eq!(
        parse(&["settings", "project", "prj_1", "--section", "automations"]),
        Op::Settings(SettingsOp::Project {
            id: ProjectId("prj_1".into()),
            section: Some(ProjectSection::Automations),
        })
    );
}
