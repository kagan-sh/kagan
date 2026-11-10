use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use kg_domain::*;
use serde_json::json;

use crate::Workspace;

use super::{Fixture, events, op, screen_with, started, thread, type_in, wait_for_status};
#[cfg(unix)]
use super::{claude_line, fake_agent, install_plugin, ran};

#[cfg(unix)]
#[gpui_kit::test]
async fn thread_programs_receive_a_utf8_locale(cx: &mut TestAppContext) {
    if let Ok(case) = std::env::var("KAGAN_LOCALE_TEST") {
        let fixture = Fixture::new(cx).await;
        if case == "prompt" {
            let (id, _reap) = started(&fixture, "/bin/zsh -f", cx).await;
            op(
                Op::Thread(ThreadOp::Resize {
                    id: id.clone(),
                    cols: 40,
                    rows: 8,
                    terminal: None,
                }),
                cx,
            )
            .await
            .unwrap();
            assert!(screen_with(&id, "demo", cx).await.contains("demo"));
            type_in(
                &id,
                "echo hi from kagan; echo extra >> notes.txt; echo new > added.txt",
                cx,
            )
            .await;
            screen_with(&id, "added", cx).await;
            let grid = cx.update(|cx| Workspace::global(cx).read(cx).grid(&id, None).unwrap());
            let row: String = grid.lines[0].iter().map(|cell| cell.c).collect();
            assert!(row.starts_with("➜ demo (main) ✗ echo hi"), "{row}");
            return;
        }
        let run = fixture.dir.path().join("locale");
        let program = fake_agent(&run, "locale-test");
        let (_id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
        let (_, env) = ran(&run);
        let lang = env.get("LANG").expect("LANG was set");
        if case == "existing" {
            assert_eq!(lang, "fr_FR.UTF-8");
        } else {
            assert!(
                lang.to_ascii_lowercase().replace('-', "").contains("utf8"),
                "LANG={lang}"
            );
        }
        return;
    }
    for case in ["missing", "existing", "prompt"] {
        if case == "prompt" && !Path::new("/bin/zsh").exists() {
            continue;
        }
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "tests::agent::thread_programs_receive_a_utf8_locale",
                "--nocapture",
            ])
            .env("KAGAN_LOCALE_TEST", case)
            .env_remove("LC_ALL")
            .env_remove("LC_CTYPE")
            .env_remove("LANG");
        if case == "existing" {
            command.env("LANG", "fr_FR.UTF-8");
        }
        if case == "prompt" {
            command.env("PROMPT", "➜ demo (main) ✗ ");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "{case}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[cfg(unix)]
fn assert_claude_args(args: &[String], files: &Path, agent: &Path) {
    assert_eq!(args[0], "--mcp-config");
    assert_eq!(args[1], files.join("mcp.json").display().to_string());
    assert_eq!(args[2], "--plugin-dir");
    let plugin = PathBuf::from(&args[3]);
    assert!(plugin.starts_with(agent.join("catalogs")), "{plugin:?}");
    assert_eq!(args[4], "--append-system-prompt-file");
    assert_eq!(args[5], files.join("note.txt").display().to_string());
    assert_eq!(args[6], "--disallowed-tools");
    assert_eq!(args[7], "Task,Agent");
    assert_eq!(args[8], "--session-id");
    assert_eq!(args[10], "--person-arg");
}

#[cfg(unix)]
fn assert_note(note: &str, id: &str) {
    assert!(note.contains(id), "{note}");
    assert!(note.contains("Task, Agent"), "{note}");
    assert!(note.contains("KG_THREAD_ID"), "{note}");
}

#[cfg(unix)]
fn has_pair(args: &[String], left: &str, right: &str) -> bool {
    args.windows(2)
        .any(|pair| pair[0] == left && pair[1] == right)
}

#[cfg(unix)]
async fn other_project(fixture: &Fixture, cx: &mut TestAppContext) -> (ProjectId, PathBuf) {
    let checkout = fixture.dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let OpResult::Project(project) = op(
        Op::Project(ProjectOp::Add {
            path: checkout.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("project")
    };
    (project.id, dunce::canonicalize(&checkout).unwrap())
}

#[cfg(unix)]
async fn started_on(
    project: &ProjectId,
    program: &str,
    cx: &mut TestAppContext,
) -> (ThreadId, super::Reap) {
    let OpResult::Thread(started) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: project.clone(),
            program: program.into(),
            title: None,
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    let pid = super::pid_of(&started.id, cx).expect("the host started the program");
    (started.id, super::Reap(pid))
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_claude_thread_gets_kagan_flags_env_and_skills(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    install_plugin(fixture.dir.path(), true);
    let run = fixture.dir.path().join("first");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, env) = ran(&run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let files = agent.join("threads").join(&id.0);
    assert_claude_args(&args, &files, &agent);
    let mcp = std::fs::read_to_string(files.join("mcp.json")).unwrap();
    let mcp: serde_json::Value = serde_json::from_str(&mcp).unwrap();
    assert_eq!(
        mcp["mcpServers"]["kagan"]["args"],
        json!(["mcp", "--thread", id.0.clone()])
    );
    assert_eq!(
        mcp["mcpServers"]["kagan"]["env"]["KG_SOCKET"],
        "/tmp/kagan-test.sock"
    );
    let note = std::fs::read_to_string(files.join("note.txt")).unwrap();
    assert_note(&note, &id.0);
    let environment = thread(&id, cx).await.environment_id.expect("environment").0;
    let kg = std::env::current_exe().unwrap();
    assert_eq!(env["KG_THREAD_ID"], id.0);
    assert_eq!(env["KG_PROJECT_ID"], "project");
    assert_eq!(env["KG_ENVIRONMENT_ID"], environment);
    assert_eq!(env["KG_SOCKET"], "/tmp/kagan-test.sock");
    assert_eq!(env["KG_CLI"], kg.display().to_string());
    assert!(
        env["PATH"].starts_with(&kg.parent().unwrap().display().to_string()),
        "{}",
        env["PATH"]
    );
    let plugin = Path::new(&args[3]);
    assert!(plugin.join(".claude-plugin/plugin.json").exists());
    assert!(plugin.join("skills/kg-cli/SKILL.md").exists());
    assert!(plugin.join("skills/kg-plugin-port/SKILL.md").exists());
    assert!(
        plugin
            .join("skills/kg-plugin-port/references/mapping.md")
            .exists()
    );
    assert!(plugin.join("skills/demo/notes.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn instructions_join_the_note_and_stop_at_the_cap(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (project, checkout) = other_project(&fixture, cx).await;
    std::fs::write(checkout.join("instructions.md"), "FROM_FOLDER\n").unwrap();
    let mut body = "a".repeat(4096);
    body.push_str("TAIL");
    std::fs::write(fixture.dir.path().join("instructions.md"), body).unwrap();
    let run = fixture.dir.path().join("capped");
    let (id, _reap) = started_on(&project, &claude_line(&run), cx).await;
    ran(&run);
    let note = std::fs::read_to_string(
        dunce::canonicalize(fixture.dir.path())
            .unwrap()
            .join("agent")
            .join("threads")
            .join(&id.0)
            .join("note.txt"),
    )
    .unwrap();
    assert!(
        note.contains("Instructions from your kagan instructions file:\n"),
        "{note}"
    );
    assert!(note.contains(&"a".repeat(4096)), "{note}");
    assert!(!note.contains("TAIL"), "{note}");
    assert!(!note.contains("FROM_FOLDER"), "{note}");
    let OpResult::Tools { instructions, .. } =
        op(Op::Thread(ThreadOp::Tools { id: id.clone() }), cx)
            .await
            .unwrap()
    else {
        panic!("tools")
    };
    let instructions = instructions.unwrap();
    assert!(instructions.contains("Instructions from your kagan instructions file:"));
    assert!(instructions.contains(&"a".repeat(4096)));
    assert!(!instructions.contains("TAIL"));
    assert!(!instructions.contains("FROM_FOLDER"));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn the_next_claude_thread_drops_a_deleted_file_and_a_disabled_plugin(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    install_plugin(fixture.dir.path(), true);
    let (_id, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("one")), cx).await;
    let (args, _env) = ran(&fixture.dir.path().join("one"));
    let first = PathBuf::from(&args[3]).join("skills");
    assert!(first.join("demo/notes.md").exists());
    std::fs::remove_file(fixture.dir.path().join("demo-plugin/skills/demo/notes.md")).unwrap();
    let (_id, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("two")), cx).await;
    let (args, _env) = ran(&fixture.dir.path().join("two"));
    let second = PathBuf::from(&args[3]).join("skills");
    assert!(second.join("demo/SKILL.md").exists());
    assert!(!second.join("demo/notes.md").exists());
    install_plugin(fixture.dir.path(), false);
    let (_id, _reap) = started(
        &fixture,
        &claude_line(&fixture.dir.path().join("three")),
        cx,
    )
    .await;
    let (args, _env) = ran(&fixture.dir.path().join("three"));
    let third = PathBuf::from(&args[3]).join("skills");
    assert!(!third.join("demo").exists());
    assert!(third.join("kg-cli/SKILL.md").exists());
    assert!(third.join("kg-threads/SKILL.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_subcommand_line_gets_no_kagan_flags(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("daemon");
    let program = fake_agent(&run, "claude");
    let (_id, _reap) = started(
        &fixture,
        &format!("{} daemon status", program.display()),
        cx,
    )
    .await;
    let (args, _env) = ran(&run);
    assert_eq!(args, vec!["daemon", "status"]);
    assert!(!fixture.dir.path().join("agent").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_broken_agent_folder_starts_the_program_without_plugin_flags(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("agent"), "not a folder").unwrap();
    let run = fixture.dir.path().join("broken");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, env) = ran(&run);
    assert_eq!(args, vec!["--person-arg"]);
    assert_eq!(env["KG_THREAD_ID"], id.0);
    let recorded = events(&id, cx).await;
    assert!(
        recorded.iter().any(|event| matches!(
            &event.kind,
            EventKind::Notice { severity: NoticeSeverity::Warning, text }
                if text.contains("could not set up the agent integration")
        )),
        "{recorded:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_codex_thread_gets_the_server_and_note_as_overrides(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    assert!(
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(fixture.dir.path())
            .status()
            .unwrap()
            .success()
    );
    let run = fixture.dir.path().join("codex");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    let kg = std::env::current_exe().unwrap().display().to_string();
    assert!(
        args.contains(&format!("mcp_servers.kagan.command='{kg}'")),
        "{args:?}"
    );
    assert!(
        args.contains(&format!(
            "mcp_servers.kagan.args=['mcp', '--thread', '{}']",
            id.0
        )),
        "{args:?}"
    );
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains("developer_instructions")),
        "{args:?}"
    );
    assert_eq!(args.last().map(String::as_str), Some("--person-arg"));
    assert!(has_pair(&args, "--disable", "multi_agent"), "{args:?}");
    assert!(has_pair(&args, "--disable", "multi_agent_v2"), "{args:?}");
    let root = dunce::canonicalize(fixture.dir.path()).unwrap();
    let skill = std::fs::read_to_string(root.join(".agents/skills/kg-threads/SKILL.md")).unwrap();
    assert!(skill.contains("name: kg-threads"), "{skill}");
    assert!(skill.contains("kg thread run"), "{skill}");
    let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
    assert!(exclude.contains(".agents/skills/kg-threads"), "{exclude}");
    assert_eq!(env["KG_THREAD_ID"], id.0);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_cursor_agent_thread_receives_the_threads_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (project, checkout) = other_project(&fixture, cx).await;
    assert!(
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&checkout)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        fixture.dir.path().join("instructions.md"),
        "Cursor standing line.\n",
    )
    .unwrap();
    std::fs::write(checkout.join("instructions.md"), "FROM_FOLDER\n").unwrap();
    let run = fixture.dir.path().join("cursor");
    let program = fake_agent(&run, "cursor-agent");
    let (id, _reap) =
        started_on(&project, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let cursor = agent.join("threads").join(&id.0).join("cursor");
    assert_eq!(args[0], "--plugin-dir");
    assert_eq!(args[1], cursor.display().to_string());
    assert_eq!(args.last().map(String::as_str), Some("--person-arg"));
    let skills = std::fs::read_link(cursor.join("skills")).unwrap();
    assert!(skills.join("kg-threads/SKILL.md").exists());
    assert!(skills.join("kg-cli/SKILL.md").exists());
    let rule = std::fs::read_to_string(cursor.join("rules/kagan.mdc")).unwrap();
    assert!(rule.contains("alwaysApply: true"), "{rule}");
    assert!(rule.contains("kg-threads"), "{rule}");
    assert!(rule.contains("Task, Agent"), "{rule}");
    assert!(rule.contains("Cursor standing line."), "{rule}");
    assert!(!rule.contains("FROM_FOLDER"), "{rule}");
    assert!(!rule.contains(&id.0), "{rule}");
    assert!(!fixture.dir.path().join(".cursor/rules/kagan.mdc").exists());
    let record = thread(&id, cx).await;
    let OpResult::Environment(environment) = op(
        Op::Environment(EnvironmentOp::Get {
            id: record.environment_id.clone().unwrap(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("environment")
    };
    let workspace =
        std::fs::read_to_string(environment.path.join(".cursor/rules/kagan.mdc")).unwrap();
    assert_eq!(workspace, rule);
    let exclude = std::fs::read_to_string(environment.path.join(".git/info/exclude")).unwrap();
    assert!(exclude.contains(".cursor/rules/kagan.mdc"), "{exclude}");
    assert!(cursor.join(".cursor-plugin/plugin.json").exists());
    assert!(!agent.join("rules/kagan.mdc").exists());
    assert_eq!(env["KG_THREAD_ID"], id.0);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn each_cursor_thread_keeps_its_own_rule_file(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let first_run = fixture.dir.path().join("cursor-a");
    let second_run = fixture.dir.path().join("cursor-b");
    let first = fake_agent(&first_run, "cursor-agent");
    let second = fake_agent(&second_run, "cursor-agent");
    let (first_id, _reap_a) = started(&fixture, first.to_str().unwrap(), cx).await;
    ran(&first_run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let first_rule = agent
        .join("threads")
        .join(&first_id.0)
        .join("cursor/rules/kagan.mdc");
    std::fs::write(&first_rule, "keep-first\n").unwrap();
    let (second_id, _reap_b) = started(&fixture, second.to_str().unwrap(), cx).await;
    ran(&second_run);
    let second_rule = agent
        .join("threads")
        .join(&second_id.0)
        .join("cursor/rules/kagan.mdc");
    assert_ne!(first_rule, second_rule);
    assert_eq!(
        std::fs::read_to_string(&first_rule).unwrap(),
        "keep-first\n"
    );
    assert!(
        std::fs::read_to_string(&second_rule)
            .unwrap()
            .contains("alwaysApply: true")
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_non_agent_start_clears_prior_instruct(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("claude-then-clear");
    let program = fake_agent(&run, "claude");
    let (id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    ran(&run);
    cx.update(|cx| {
        let entity = Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .cloned()
            .expect("thread");
        entity.update(cx, |thread, _| {
            thread.instruct = Some("old persona".into());
        });
    });
    op(
        Op::Preference(PreferenceOp::Set {
            key: "agent-integration".into(),
            value: json!(false),
        }),
        cx,
    )
    .await
    .unwrap();
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Active, cx).await;
    let instruct = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .expect("thread")
            .read(cx)
            .instruct
            .clone()
    });
    assert_eq!(instruct, None);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn agent_integration_off_leaves_the_person_line_alone(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(
        Op::Preference(PreferenceOp::Set {
            key: "agent-integration".into(),
            value: json!(false),
        }),
        cx,
    )
    .await
    .unwrap();
    let run = fixture.dir.path().join("off");
    let program = fake_agent(&run, "claude");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    assert_eq!(args, vec!["--person-arg"]);
    assert_eq!(env["KG_THREAD_ID"], id.0);
    assert!(!fixture.dir.path().join("agent").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_codex_resume_line_keeps_the_kagan_server(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("resume");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(
        &fixture,
        &format!("{} resume --last", program.display()),
        cx,
    )
    .await;
    let (args, env) = ran(&run);
    let kg = std::env::current_exe().unwrap().display().to_string();
    assert!(
        args.contains(&format!("mcp_servers.kagan.command='{kg}'")),
        "{args:?}"
    );
    assert_eq!(
        args[args.len() - 2..].to_vec(),
        vec!["resume".to_owned(), "--last".to_owned()]
    );
    assert_eq!(env["KG_THREAD_ID"], id.0);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn archive_and_delete_remove_the_thread_agent_files(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let agent = fixture.dir.path().join("agent");
    let (archived, _reap) =
        started(&fixture, &claude_line(&fixture.dir.path().join("a")), cx).await;
    ran(&fixture.dir.path().join("a"));
    assert!(agent.join("threads").join(&archived.0).exists());
    op(
        Op::Thread(ThreadOp::Archive {
            id: archived.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    assert!(!agent.join("threads").join(&archived.0).exists());

    let (deleted, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("b")), cx).await;
    ran(&fixture.dir.path().join("b"));
    assert!(agent.join("threads").join(&deleted.0).exists());
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: deleted.clone(),
                approval_thread_id: None,
            }),
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
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(!agent.join("threads").join(&deleted.0).exists());
}

#[cfg(unix)]
fn write_skill(root: &Path, name: &str, body: &str) {
    write_skill_as(root, name, name, body);
}

#[cfg(unix)]
fn write_skill_as(root: &Path, folder: &str, name: &str, body: &str) {
    let root = root.join(folder);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {name} skill.\n---\n\n{body}\n"),
    )
    .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
async fn bb_skills_stay_out_of_the_plugin_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(
        &fixture.dir.path().join(".bb/skills"),
        "bb-project",
        "project body",
    );
    let run = fixture.dir.path().join("claude-bb");
    let (_id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, _env) = ran(&run);
    let skills = PathBuf::from(&args[3]).join("skills");
    assert!(!skills.join("bb-project").exists());
    assert!(skills.join("kg-cli/SKILL.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn codex_and_kimi_share_workspace_links_without_touching_person_entries(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    assert!(
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(fixture.dir.path())
            .status()
            .unwrap()
            .success()
    );
    write_skill(
        &fixture.dir.path().join("skills"),
        "user-skill",
        "user body",
    );
    let mine = fixture.dir.path().join(".agents/skills/mine");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(
        mine.join("SKILL.md"),
        "---\nname: mine\ndescription: mine.\n---\n\nmine\n",
    )
    .unwrap();
    let codex_run = fixture.dir.path().join("codex-bb");
    let codex_program = fake_agent(&codex_run, "codex");
    let (codex_id, _codex_reap) = started(&fixture, codex_program.to_str().unwrap(), cx).await;
    ran(&codex_run);
    let root = dunce::canonicalize(fixture.dir.path()).unwrap();
    assert!(root.join(".agents/skills/kg-threads/SKILL.md").exists());
    assert!(root.join(".agents/skills/user-skill/SKILL.md").exists());
    assert!(root.join(".agents/skills/mine/SKILL.md").exists());
    let visible = std::fs::read_to_string(codex_run.join("out/agents-skills")).unwrap();
    assert!(visible.lines().any(|line| line == "kg-cli"), "{visible}");
    assert!(
        visible.lines().any(|line| line == "user-skill"),
        "{visible}"
    );
    let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
    assert!(
        exclude
            .lines()
            .any(|line| line.trim() == ".agents/skills/kg-threads"),
        "{exclude}"
    );
    assert!(
        exclude
            .lines()
            .any(|line| line.trim() == ".agents/skills/user-skill"),
        "{exclude}"
    );
    assert!(
        !exclude.lines().any(|line| line.trim() == ".agents/skills/"),
        "{exclude}"
    );

    let kimi_run = fixture.dir.path().join("kimi-bb");
    let kimi_program = fake_agent(&kimi_run, "kimi");
    let (kimi_id, _kimi_reap) = started(&fixture, kimi_program.to_str().unwrap(), cx).await;
    let (args, _env) = ran(&kimi_run);
    assert_ne!(codex_id, kimi_id);
    let mut pairs = args.chunks(2);
    assert!(
        pairs.any(|pair| pair[0] == "--skills-dir"
            && Path::new(&pair[1]).ends_with("skills")
            && Path::new(&pair[1]).join("user-skill/SKILL.md").exists()),
        "{args:?}"
    );
    assert!(!root.join(".kimi-code").exists());
    assert!(root.join(".agents/skills/mine/SKILL.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn opencode_gets_the_composed_skills_in_its_own_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(
        &fixture.dir.path().join("skills"),
        "user-skill",
        "user body",
    );
    let run = fixture.dir.path().join("opencode-bb");
    let program = fake_agent(&run, "opencode");
    let (_id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, _env) = ran(&run);
    assert_eq!(args[0], "--session");
    assert!(!args[1].is_empty());
    assert_eq!(args[2], "--person-arg");
    let root = dunce::canonicalize(fixture.dir.path()).unwrap();
    let visible = std::fs::read_to_string(run.join("out/opencode-skills")).unwrap();
    assert!(visible.lines().any(|line| line == "kg-cli"), "{visible}");
    assert!(
        visible.lines().any(|line| line == "user-skill"),
        "{visible}"
    );
    assert!(root.join(".opencode/skills/kg-threads/SKILL.md").exists());
    assert!(root.join(".opencode/skills/user-skill/SKILL.md").exists());
    assert!(!root.join(".agents").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn pi_gets_the_composed_skill_root_as_an_explicit_option(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(
        &fixture.dir.path().join("skills"),
        "user-skill",
        "user body",
    );
    let run = fixture.dir.path().join("pi-bb");
    let program = fake_agent(&run, "pi");
    let (project, checkout) = other_project(&fixture, cx).await;
    std::fs::write(fixture.dir.path().join("instructions.md"), "Call me Ada.\n").unwrap();
    std::fs::write(checkout.join("instructions.md"), "FROM_FOLDER\n").unwrap();
    let (id, _reap) =
        started_on(&project, &format!("{} --person-arg", program.display()), cx).await;
    let (args, _env) = ran(&run);
    assert_eq!(args[0], "--skill");
    let skills = PathBuf::from(&args[1]);
    let note = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent")
        .join("threads")
        .join(&id.0)
        .join("note.txt");
    assert_eq!(args[2], "--append-system-prompt");
    assert_eq!(args[3], note.display().to_string());
    assert_eq!(args[4], "--session-id");
    assert!(!args[5].is_empty());
    assert_eq!(args[6], "--person-arg");
    let written = std::fs::read_to_string(&note).unwrap();
    assert!(written.contains(&id.0), "{written}");
    assert!(written.contains("Call me Ada."), "{written}");
    assert!(!written.contains("FROM_FOLDER"), "{written}");
    assert!(skills.join("user-skill/SKILL.md").exists());
    assert!(!fixture.dir.path().join(".agents").exists());
    assert!(!fixture.dir.path().join(".opencode").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_new_program_subcommand_line_gets_no_skill_integration(cx: &mut TestAppContext) {
    for (name, line) in [
        ("pi", "config"),
        ("kimi", "doctor"),
        ("opencode", "debug config"),
    ] {
        let fixture = Fixture::new(cx).await;
        let run = fixture.dir.path().join(format!("{name}-subcommand"));
        let program = fake_agent(&run, name);
        let (_id, _reap) = started(&fixture, &format!("{} {line}", program.display()), cx).await;
        let (args, _env) = ran(&run);
        assert_eq!(args, line.split(' ').map(str::to_owned).collect::<Vec<_>>());
        assert!(!fixture.dir.path().join("agent").exists());
        assert!(!fixture.dir.path().join(".agents").exists());
        assert!(!fixture.dir.path().join(".opencode").exists());
        assert!(!fixture.dir.path().join(".kimi-code").exists());
    }
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_refreshes_workspace_links_and_removes_stale_ones(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    assert!(
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(fixture.dir.path())
            .status()
            .unwrap()
            .success()
    );
    write_skill(&fixture.dir.path().join("skills"), "first", "first body");
    let run = fixture.dir.path().join("codex-restart");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    ran(&run);
    let root = dunce::canonicalize(fixture.dir.path()).unwrap();
    assert!(root.join(".agents/skills/first/SKILL.md").exists());
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Idle, cx).await;
    std::fs::remove_dir_all(fixture.dir.path().join("skills/first")).unwrap();
    write_skill(&fixture.dir.path().join("skills"), "second", "second body");
    std::fs::remove_file(run.join("out/env")).unwrap();
    std::fs::remove_file(run.join("out/args")).unwrap();
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(&id, ThreadStatus::Active, cx).await;
    ran(&run);
    assert!(!root.join(".agents/skills/first").exists());
    assert!(root.join(".agents/skills/second/SKILL.md").exists());
    assert!(root.join(".agents/skills/kg-threads/SKILL.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn kagan_user_and_project_skills_join_the_native_tree_with_decision_24(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("skills"), "from-user", "user body");
    write_skill(&fixture.dir.path().join("skills"), "same", "user body");
    write_skill(
        &fixture.dir.path().join(".kagan/skills"),
        "from-project",
        "project body",
    );
    write_skill(
        &fixture.dir.path().join(".kagan/skills"),
        "same",
        "project body",
    );
    let run = fixture.dir.path().join("claude-kagan-skills");
    let (_id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, _env) = ran(&run);
    let skills = PathBuf::from(&args[3]).join("skills");
    assert!(skills.join("from-user/SKILL.md").exists());
    assert!(skills.join("from-project/SKILL.md").exists());
    let same = std::fs::read_to_string(skills.join("same/SKILL.md")).unwrap();
    assert!(same.contains("project body"), "{same}");
    let cli = std::fs::read_to_string(skills.join("kg-cli/SKILL.md")).unwrap();
    assert!(cli.contains("Find the running kagan app"), "{cli}");
    assert!(!fixture.dir.path().join("skills/kg-cli").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_second_project_keeps_the_first_catalog_and_its_links(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(
        &fixture.dir.path().join(".kagan/skills"),
        "first-skill",
        "first body",
    );
    let first_run = fixture.dir.path().join("codex-first");
    let first_program = fake_agent(&first_run, "codex");
    let (first_id, _first_reap) = started(&fixture, first_program.to_str().unwrap(), cx).await;
    ran(&first_run);
    let first_link = fixture.dir.path().join(".agents/skills/first-skill");
    assert_eq!(
        std::fs::read_to_string(first_link.join("SKILL.md")).unwrap(),
        "---\nname: first-skill\ndescription: first-skill skill.\n---\n\nfirst body\n"
    );

    let second_dir = fixture.dir.path().join("project-b");
    std::fs::create_dir_all(second_dir.join(".kagan/skills/second-skill")).unwrap();
    std::fs::write(
        second_dir.join(".kagan/skills/second-skill/SKILL.md"),
        "---\nname: second-skill\ndescription: second-skill skill.\n---\n\nsecond body\n",
    )
    .unwrap();
    let OpResult::Project(second) = op(
        Op::Project(ProjectOp::Add {
            path: second_dir.clone(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("project");
    };
    let second_run = second_dir.join("codex-second");
    let second_program = fake_agent(&second_run, "codex");
    let OpResult::Thread(second_thread) = op(
        Op::Thread(ThreadOp::Run(Run {
            project_id: second.id.clone(),
            program: second_program.display().to_string(),
            title: None,
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            quiet: false,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread");
    };
    ran(&second_run);
    assert!(
        second_dir
            .join(".agents/skills/second-skill/SKILL.md")
            .exists()
    );
    assert_eq!(
        std::fs::read_to_string(first_link.join("SKILL.md")).unwrap(),
        "---\nname: first-skill\ndescription: first-skill skill.\n---\n\nfirst body\n"
    );
    assert_ne!(first_id, second_thread.id);
    assert!(first_link.is_symlink());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_failed_skill_link_warns_but_starts_the_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join(".agents"), "not a folder").unwrap();
    let run = fixture.dir.path().join("codex-link-failure");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    let (args, _env) = ran(&run);
    let kg = std::env::current_exe().unwrap().display().to_string();
    assert!(
        args.contains(&format!("mcp_servers.kagan.command='{kg}'")),
        "{args:?}"
    );
    let recorded = events(&id, cx).await;
    assert!(
        recorded.iter().any(|event| matches!(
            &event.kind,
            EventKind::Notice { severity: NoticeSeverity::Warning, text }
                if text.contains("could not offer the composed skills")
        )),
        "{recorded:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn kimi_uses_skills_dir_without_a_git_root(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(
        &fixture.dir.path().join("skills"),
        "user-skill",
        "user body",
    );
    let run = fixture.dir.path().join("kimi-no-git");
    let program = fake_agent(&run, "kimi");
    let (_id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    let (args, _env) = ran(&run);
    assert!(
        args.chunks(2).any(|pair| pair[0] == "--skills-dir"
            && Path::new(&pair[1]).join("user-skill/SKILL.md").exists()),
        "{args:?}"
    );
    assert!(!fixture.dir.path().join(".agents").exists());
    assert!(!fixture.dir.path().join(".kimi-code").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn kimi_reads_skills_from_the_folder_its_workspace_names(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = fixture.dir.path().join("kimi-home");
    write_skill(&home.join("skills"), "from-home", "home body");
    let run = fixture.dir.path().join("kimi-home-run");
    let program = fake_agent(&run, "kimi");
    let (_id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    let (args, _env) = ran(&run);
    assert!(
        args.chunks(2)
            .any(|pair| pair[0] == "--skills-dir" && Path::new(&pair[1]) == home.join("skills")),
        "{args:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_start_retires_the_old_agent_skills_links(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    assert!(
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(fixture.dir.path())
            .status()
            .unwrap()
            .success()
    );
    let old = fixture.dir.path().join("agent/skills/kg-threads");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(
        old.join("SKILL.md"),
        "---\nname: kg-threads\ndescription: old\n---\n\nold body\n",
    )
    .unwrap();
    let old = dunce::canonicalize(old).unwrap();
    let link = fixture.dir.path().join(".agents/skills/kg-threads");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&old, &link).unwrap();
    let exclude = fixture.dir.path().join(".git/info/exclude");
    std::fs::write(&exclude, ".agents/skills/kg-threads\n").unwrap();

    let run = fixture.dir.path().join("claude-legacy");
    let (_id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    ran(&run);
    assert!(!link.exists());
    assert!(
        std::fs::read_to_string(&exclude)
            .unwrap()
            .contains(".agents/skills/kg-threads")
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_plugin_persona_and_its_tools_reach_the_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    fixture.workspace.update(cx, |workspace, _| {
        workspace.set_plugin_host(std::rc::Rc::new(|request, _| {
            let result = match request {
                crate::PluginRequest::Instruct { .. } => {
                    Ok(OpResult::Output(Some("PLUGIN_PERSONA".into())))
                }
                crate::PluginRequest::Commands => Ok(OpResult::PluginCommands(Vec::new())),
                crate::PluginRequest::Tools { .. } => Ok(OpResult::Tools {
                    instructions: None,
                    tools: vec![
                        AgentTool {
                            name: "zeta".into(),
                            description: "a plugin tool".into(),
                            input_schema: json!({}),
                        },
                        AgentTool {
                            name: crate::skill::SKILL_LOAD.into(),
                            description: "stale".into(),
                            input_schema: json!({}),
                        },
                    ],
                }),
                _ => Err(OpError::Failed("unexpected plugin request".into())),
            };
            gpui_kit::Task::ready(result)
        }));
    });
    let run = fixture.dir.path().join("persona-run");
    let program = fake_agent(&run, "claude");
    let (id, _reap) = started(&fixture, program.to_str().unwrap(), cx).await;
    let instruct = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .expect("thread")
            .read(cx)
            .instruct
            .clone()
    });
    assert_eq!(instruct.as_deref(), Some("PLUGIN_PERSONA"));
    let OpResult::Tools { tools, .. } = op(Op::Thread(ThreadOp::Tools { id: id.clone() }), cx)
        .await
        .unwrap()
    else {
        panic!("tools returned no list")
    };
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert!(names.contains(&"zeta"), "{names:?}");
    assert_eq!(
        names
            .iter()
            .filter(|name| **name == crate::skill::SKILL_LOAD)
            .count(),
        1,
        "{names:?}"
    );
}
