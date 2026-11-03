use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use kg_domain::*;
use serde_json::json;

use super::{Fixture, claude_line, events, fake_agent, op, ran, started, wait_for_status};

fn claude_config(fixture: &Fixture, cx: &mut TestAppContext) -> PathBuf {
    let config = fixture.dir.path().join("claude");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.claude_config = Some(config.clone());
    });
    config
}

/// What Claude Code leaves once something was said in the conversation.
fn said(config: &Path, conversation: &str) {
    let folder = config.join("projects").join("-some-folder");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{conversation}.jsonl")), "{}\n").unwrap();
}

fn after(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).cloned()
}

async fn stop(id: &ThreadId, cx: &mut TestAppContext) {
    op(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx)
        .await
        .unwrap();
    wait_for_status(id, ThreadStatus::Idle, cx).await;
}

async fn start_again(id: &ThreadId, run: &Path, cx: &mut TestAppContext) -> Vec<String> {
    std::fs::remove_file(run.join("out").join("env")).unwrap();
    op(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx)
        .await
        .unwrap();
    ran(run).0
}

async fn notices(id: &ThreadId, cx: &mut TestAppContext) -> Vec<String> {
    events(id, cx)
        .await
        .into_iter()
        .filter_map(|event| match event.kind {
            EventKind::Notice { severity, text } => {
                assert_eq!(severity, NoticeSeverity::Info, "{text}");
                Some(text)
            }
            _ => None,
        })
        .collect()
}

async fn integration(on: bool, cx: &mut TestAppContext) {
    op(
        Op::Preference(PreferenceOp::Set {
            key: "agent-integration".into(),
            value: json!(on),
        }),
        cx,
    )
    .await
    .unwrap();
}

#[gpui_kit::test]
async fn start_again_resumes_the_conversation_the_first_start_named(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let config = claude_config(&fixture, cx);
    let run = fixture.dir.path().join("claude-run");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let (first, _) = ran(&run);
    let conversation = after(&first, "--session-id").expect("a conversation id");
    assert!(after(&first, "--resume").is_none(), "{first:?}");
    said(&config, &conversation);
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert_eq!(after(&again, "--resume"), Some(conversation), "{again:?}");
    assert!(after(&again, "--session-id").is_none(), "{again:?}");
    assert_eq!(again.last().map(String::as_str), Some("--person-arg"));
    assert!(notices(&id, cx).await.is_empty());
}

#[gpui_kit::test]
async fn start_again_begins_a_new_conversation_when_the_last_one_is_gone(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let config = claude_config(&fixture, cx);
    let run = fixture.dir.path().join("claude-run");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let lost = after(&ran(&run).0, "--session-id").unwrap();
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(after(&again, "--resume").is_none(), "{again:?}");
    let fresh = after(&again, "--session-id").unwrap();
    assert_ne!(fresh, lost);
    assert!(
        notices(&id, cx)
            .await
            .iter()
            .any(|text| text.contains("could not find the agent's last conversation")),
    );
    said(&config, &fresh);
    stop(&id, cx).await;
    let third = start_again(&id, &run, cx).await;
    assert_eq!(after(&third, "--resume"), Some(fresh));
}

#[gpui_kit::test]
async fn a_thread_without_a_saved_conversation_starts_a_new_one_and_says_so(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    claude_config(&fixture, cx);
    integration(false, cx).await;
    let run = fixture.dir.path().join("claude-run");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    assert_eq!(ran(&run).0, vec!["--person-arg"]);
    stop(&id, cx).await;
    integration(true, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(after(&again, "--session-id").is_some(), "{again:?}");
    assert!(
        notices(&id, cx)
            .await
            .iter()
            .any(|text| text.contains("no saved conversation to resume")),
    );
}

#[gpui_kit::test]
async fn threads_in_one_folder_resume_their_own_conversations(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let config = claude_config(&fixture, cx);
    let (one, two) = (
        fixture.dir.path().join("one"),
        fixture.dir.path().join("two"),
    );
    let (first, _reap_one) = started(&fixture, &claude_line(&one), cx).await;
    let (second, _reap_two) = started(&fixture, &claude_line(&two), cx).await;
    assert_eq!(
        super::thread(&first, cx).await.environment_id,
        super::thread(&second, cx).await.environment_id
    );
    let first_conversation = after(&ran(&one).0, "--session-id").unwrap();
    let second_conversation = after(&ran(&two).0, "--session-id").unwrap();
    assert_ne!(first_conversation, second_conversation);
    said(&config, &first_conversation);
    said(&config, &second_conversation);
    stop(&first, cx).await;
    stop(&second, cx).await;
    assert_eq!(
        after(&start_again(&second, &two, cx).await, "--resume"),
        Some(second_conversation)
    );
    assert_eq!(
        after(&start_again(&first, &one, cx).await, "--resume"),
        Some(first_conversation)
    );
}

#[gpui_kit::test]
async fn agent_integration_off_adds_no_conversation_on_start_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let config = claude_config(&fixture, cx);
    let run = fixture.dir.path().join("claude-run");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    said(&config, &after(&ran(&run).0, "--session-id").unwrap());
    stop(&id, cx).await;
    integration(false, cx).await;
    assert_eq!(start_again(&id, &run, cx).await, vec!["--person-arg"]);
}

#[gpui_kit::test]
async fn a_restart_waits_for_start_again_and_then_resumes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let config = claude_config(&fixture, cx);
    let run = fixture.dir.path().join("claude-run");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let conversation = after(&ran(&run).0, "--session-id").unwrap();
    said(&config, &conversation);
    let fixture = fixture.restart(cx, |_| {}).await;
    claude_config(&fixture, cx);
    cx.run_until_parked();
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Error);
    assert!(super::pid_of(&id, cx).is_none());
    assert_eq!(
        after(&start_again(&id, &run, cx).await, "--resume"),
        Some(conversation)
    );
}

#[gpui_kit::test]
async fn a_line_that_picks_its_conversation_gets_none_from_kagan(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    claude_config(&fixture, cx);
    codex_home(&fixture, cx);
    cursor_config(&fixture, cx);
    let plugin = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let lines = [
        ("claude", "--resume=abc"),
        ("claude", "-c"),
        ("claude", "-r abc"),
        ("claude", "-p hi"),
        ("codex", "resume --last"),
        ("codex", "fork --last"),
        ("cursor-agent", "--resume abc"),
        ("cursor-agent", "--continue"),
        ("agent", "--new-session-id abc"),
        ("agent", "login"),
    ];
    for (number, (agent, rest)) in lines.into_iter().enumerate() {
        let run = fixture.dir.path().join(format!("run-{number}"));
        let program = fake_agent(&run, agent);
        let (id, _reap) = started(&fixture, &format!("{} {rest}", program.display()), cx).await;
        let (first, _) = ran(&run);
        stop(&id, cx).await;
        let again = start_again(&id, &run, cx).await;
        for args in [&first, &again] {
            assert!(after(args, "--session-id").is_none(), "{rest}: {args:?}");
            if !rest.contains("--new-session-id") {
                assert!(
                    after(args, "--new-session-id").is_none(),
                    "{rest}: {args:?}"
                );
            }
            assert!(args.ends_with(&person(rest)), "{rest}: {args:?}");
            if agent == "cursor-agent" || agent == "agent" {
                if rest == "login" {
                    assert_eq!(args, &person(rest), "{rest}: {args:?}");
                } else {
                    let mut expected =
                        vec!["--plugin-dir".to_owned(), plugin.display().to_string()];
                    expected.extend(person(rest));
                    assert_eq!(args, &expected, "{rest}: {args:?}");
                }
            }
        }
        assert!(notices(&id, cx).await.is_empty(), "{rest}");
    }
}

fn leaves_out(args: &[String], ids: &[&str]) {
    assert!(
        args.iter().all(|arg| !ids.contains(&arg.as_str())),
        "{args:?}"
    );
}

fn person(rest: &str) -> Vec<String> {
    rest.split(' ').map(str::to_owned).collect()
}

fn cursor_config(fixture: &Fixture, cx: &mut TestAppContext) -> PathBuf {
    let config = fixture.dir.path().join("cursor");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.cursor_config = Some(config.clone());
    });
    config
}

/// What Cursor Agent leaves for one conversation: `chats/<hash>/<id>/meta.json`, whose `cwd` is the folder.
fn wrote(config: &Path, folder: &Path, conversation: &str) {
    let dir = config.join("chats").join("bucket").join(conversation);
    std::fs::create_dir_all(&dir).unwrap();
    let meta = json!({
        "schemaVersion": 1,
        "createdAtMs": 1,
        "hasConversation": true,
        "cwd": folder,
    });
    std::fs::write(dir.join("meta.json"), format!("{meta}\n")).unwrap();
}

fn codex_home(fixture: &Fixture, cx: &mut TestAppContext) -> PathBuf {
    let home = fixture.dir.path().join("codex-home");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.codex_home = Some(home.clone());
    });
    home
}

/// What Codex writes when a conversation begins: its first line names the folder and the UTC start.
fn began(home: &Path, folder: &Path, at: chrono::DateTime<chrono::Utc>) -> String {
    let conversation = Conversation::fresh().to_string();
    let day = home
        .join("sessions")
        .join(at.format("%Y").to_string())
        .join(at.format("%m").to_string())
        .join(at.format("%d").to_string());
    std::fs::create_dir_all(&day).unwrap();
    let line = json!({
        "timestamp": at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "type": "session_meta",
        "payload": {
            "id": conversation,
            "cwd": folder,
            "timestamp": at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        },
    });
    std::fs::write(
        day.join(format!("rollout-2026-01-01T00-00-00-{conversation}.jsonl")),
        format!("{line}\n{{}}\n"),
    )
    .unwrap();
    conversation
}

fn later() -> chrono::DateTime<chrono::Utc> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    chrono::DateTime::from_timestamp_millis(i64::try_from(now.as_millis()).unwrap()).unwrap()
        + chrono::Duration::hours(1)
}

async fn codex_thread(
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (ThreadId, PathBuf, super::Reap) {
    let run = fixture.dir.path().join("codex-run");
    let program = fake_agent(&run, "codex");
    let (id, reap) = started(fixture, &program.display().to_string(), cx).await;
    ran(&run);
    stop(&id, cx).await;
    (id, run, reap)
}

#[gpui_kit::test]
async fn start_again_resumes_the_one_codex_conversation_begun_in_its_folder(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let home = codex_home(&fixture, cx);
    let (id, run, _reap) = codex_thread(&fixture, cx).await;
    began(
        &home,
        fixture.dir.path(),
        later() - chrono::Duration::days(1),
    );
    began(&home, &fixture.dir.path().join("elsewhere"), later());
    std::fs::create_dir_all(fixture.dir.path().join("elsewhere")).unwrap();
    let conversation = began(&home, fixture.dir.path(), later());
    let again = start_again(&id, &run, cx).await;
    assert!(
        again.ends_with(&["resume".to_owned(), conversation.clone()]),
        "{again:?}"
    );
    assert!(notices(&id, cx).await.is_empty());
    let next = began(&home, fixture.dir.path(), later());
    stop(&id, cx).await;
    let third = start_again(&id, &run, cx).await;
    assert!(
        third.ends_with(&["resume".to_owned(), conversation.clone()]),
        "{third:?}"
    );
    forget(&home, &conversation);
    stop(&id, cx).await;
    let fourth = start_again(&id, &run, cx).await;
    assert!(!fourth.contains(&conversation), "{fourth:?}");
    assert!(fourth.ends_with(&["resume".to_owned(), next]), "{fourth:?}");
}

#[gpui_kit::test]
async fn start_again_resumes_the_cursor_conversation_the_first_start_named(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let config = cursor_config(&fixture, cx);
    let folder = dunce::canonicalize(fixture.dir.path()).unwrap();
    let plugin = folder.join("agent");
    let elsewhere = folder.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let run = fixture.dir.path().join("cursor-run");
    let program = fake_agent(&run, "cursor-agent");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let (first, _) = ran(&run);
    let conversation = after(&first, "--new-session-id").expect("a conversation id");
    assert_eq!(
        first,
        [
            "--plugin-dir".to_owned(),
            plugin.display().to_string(),
            "--new-session-id".to_owned(),
            conversation.clone(),
            "--person-arg".to_owned()
        ]
    );

    let stranger = Conversation::fresh().to_string();
    let other = Conversation::fresh().to_string();
    wrote(&config, &folder, &stranger);
    wrote(&config, &elsewhere, &other);
    wrote(&config, &folder, &conversation);
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert_eq!(
        again,
        [
            "--plugin-dir".to_owned(),
            plugin.display().to_string(),
            "--resume".to_owned(),
            conversation.clone(),
            "--person-arg".to_owned()
        ]
    );
    assert!(after(&again, "--new-session-id").is_none(), "{again:?}");
    leaves_out(&again, &[&stranger, &other]);
    assert!(notices(&id, cx).await.is_empty());

    std::fs::remove_dir_all(config.join("chats").join("bucket").join(&conversation)).unwrap();
    stop(&id, cx).await;
    let missing = start_again(&id, &run, cx).await;
    assert!(after(&missing, "--resume").is_none(), "{missing:?}");
    let fresh = after(&missing, "--new-session-id").expect("a new conversation");
    assert_ne!(fresh, conversation);
    leaves_out(&missing, &[&stranger, &other]);
    assert!(
        notices(&id, cx)
            .await
            .iter()
            .any(|text| text.contains("could not find the agent's last conversation")),
    );

    wrote(&config, &elsewhere, &fresh);
    stop(&id, cx).await;
    let wrong_folder = start_again(&id, &run, cx).await;
    assert!(
        after(&wrong_folder, "--resume").is_none(),
        "{wrong_folder:?}"
    );
    assert_ne!(
        after(&wrong_folder, "--new-session-id").as_deref(),
        Some(fresh.as_str())
    );
    leaves_out(&wrong_folder, &[&stranger, &other, &fresh]);

    let run = fixture.dir.path().join("agent-run");
    let program = fake_agent(&run, "agent");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    let conversation = after(&ran(&run).0, "--new-session-id").unwrap();
    wrote(&config, &folder, &conversation);
    stop(&id, cx).await;
    assert_eq!(
        start_again(&id, &run, cx).await,
        [
            "--plugin-dir".to_owned(),
            plugin.display().to_string(),
            "--resume".to_owned(),
            conversation,
        ]
    );
}

#[gpui_kit::test]
async fn a_worktree_name_is_not_a_cursor_subcommand(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    cursor_config(&fixture, cx);
    let plugin = dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent");
    let lines = [("-w", "agent"), ("--worktree", "login")];
    for (number, (flag, name)) in lines.into_iter().enumerate() {
        let run = fixture.dir.path().join(format!("worktree-{number}"));
        let program = fake_agent(&run, "cursor-agent");
        let (_id, _reap) = started(
            &fixture,
            &format!("{} {flag} {name}", program.display()),
            cx,
        )
        .await;
        let (named, _) = ran(&run);
        let stored = after(&named, "--new-session-id").expect("a conversation id");
        assert_eq!(
            named,
            [
                "--plugin-dir".to_owned(),
                plugin.display().to_string(),
                "--new-session-id".to_owned(),
                stored,
                flag.to_owned(),
                name.to_owned(),
            ]
        );
    }
}

fn forget(home: &Path, conversation: &str) {
    let end = format!("-{conversation}.jsonl");
    let mut folders = vec![home.join("sessions")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.to_string_lossy().ends_with(&end) {
                std::fs::remove_file(path).unwrap();
                return;
            }
        }
    }
    panic!("no file for {conversation}");
}

#[gpui_kit::test]
async fn start_again_on_codex_starts_a_new_conversation_when_none_or_two_match(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let home = codex_home(&fixture, cx);
    let (id, run, _reap) = codex_thread(&fixture, cx).await;
    for _ in 0..2 {
        let again = start_again(&id, &run, cx).await;
        assert!(!again.iter().any(|arg| arg == "resume"), "{again:?}");
        stop(&id, cx).await;
        began(&home, fixture.dir.path(), later());
        began(&home, fixture.dir.path(), later());
    }
    let notices = notices(&id, cx).await;
    assert!(
        notices
            .iter()
            .all(|text| text.contains("could not tell which Codex conversation")),
        "{notices:?}"
    );
    assert_eq!(notices.len(), 2);
}
