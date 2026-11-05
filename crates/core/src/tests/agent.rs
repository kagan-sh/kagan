use std::path::Path;

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
#[gpui_kit::test]
async fn a_claude_thread_gets_kagan_flags_env_and_skills(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::SocketLocation("/tmp/kagan-test.sock".into())));
    let fixture = Fixture::new(cx).await;
    install_plugin(fixture.dir.path(), true);
    let run = fixture.dir.path().join("first");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (args, env) = ran(&run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let files = agent.join("threads").join(&id.0);
    assert_eq!(args[0], "--mcp-config");
    assert_eq!(args[1], files.join("mcp.json").display().to_string());
    assert_eq!(args[2], "--plugin-dir");
    assert_eq!(args[3], agent.display().to_string());
    assert_eq!(args[4], "--append-system-prompt-file");
    assert_eq!(args[5], files.join("note.txt").display().to_string());
    assert_eq!(args[6], "--session-id");
    assert_eq!(args[8], "--person-arg");
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
    assert!(note.contains(&id.0), "{note}");
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
    assert!(agent.join(".claude-plugin/plugin.json").exists());
    assert!(agent.join("skills/kg-cli/SKILL.md").exists());
    assert!(agent.join("skills/kg-plugin-port/SKILL.md").exists());
    assert!(agent.join("skills/demo/notes.md").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn the_next_claude_thread_drops_a_deleted_file_and_a_disabled_plugin(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let agent = fixture.dir.path().join("agent");
    install_plugin(fixture.dir.path(), true);
    let (_id, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("one")), cx).await;
    ran(&fixture.dir.path().join("one"));
    std::fs::remove_file(fixture.dir.path().join("demo-plugin/skills/demo/notes.md")).unwrap();
    let (_id, _reap) = started(&fixture, &claude_line(&fixture.dir.path().join("two")), cx).await;
    ran(&fixture.dir.path().join("two"));
    assert!(agent.join("skills/demo/SKILL.md").exists());
    assert!(!agent.join("skills/demo/notes.md").exists());
    install_plugin(fixture.dir.path(), false);
    let (_id, _reap) = started(
        &fixture,
        &claude_line(&fixture.dir.path().join("three")),
        cx,
    )
    .await;
    ran(&fixture.dir.path().join("three"));
    assert!(!agent.join("skills/demo").exists());
    assert!(agent.join("skills/kg-cli/SKILL.md").exists());
    assert!(agent.join("skills/kg-threads/SKILL.md").exists());
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
    let run = fixture.dir.path().join("cursor");
    let program = fake_agent(&run, "cursor-agent");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (args, env) = ran(&run);
    let agent = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let cursor = agent.join("threads").join(&id.0).join("cursor");
    assert_eq!(args[0], "--plugin-dir");
    assert_eq!(args[1], cursor.display().to_string());
    assert_eq!(args.last().map(String::as_str), Some("--person-arg"));
    let skill = std::fs::read_to_string(agent.join("skills/kg-threads/SKILL.md")).unwrap();
    assert!(skill.contains("name: kg-threads"), "{skill}");
    assert_eq!(
        std::fs::read_link(cursor.join("skills")).unwrap(),
        dunce::canonicalize(agent.join("skills")).unwrap()
    );
    let rule = std::fs::read_to_string(cursor.join("rules/kagan.mdc")).unwrap();
    assert!(rule.contains("alwaysApply: true"), "{rule}");
    assert!(rule.contains("kg-threads"), "{rule}");
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
