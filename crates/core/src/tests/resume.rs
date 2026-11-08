use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use kg_domain::*;
use serde_json::json;

use super::{Fixture, events, op, started, wait_for_status};
#[cfg(unix)]
use super::{claude_line, fake_agent, ran};
use crate::workspace::recovery::{remember_resume, resume_threads, set_resume_threads};

fn claude_config(fixture: &Fixture, cx: &mut TestAppContext) -> PathBuf {
    let config = fixture.dir.path().join("claude");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.homes.insert("CLAUDE_CONFIG_DIR", config.clone());
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

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
#[gpui_kit::test]
async fn a_restart_resumes_the_conversation_without_start_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let config = claude_config(&fixture, cx);
    let run = fixture.dir.path().join("claude-run");
    let (id, _reap) = started(&fixture, &claude_line(&run), cx).await;
    let conversation = after(&ran(&run).0, "--session-id").unwrap();
    said(&config, &conversation);
    std::fs::remove_file(run.join("out").join("env")).unwrap();
    let _config_dir = EnvVar::set("CLAUDE_CONFIG_DIR", &config);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(super::pid_of(&id, cx).is_some());
    assert_eq!(after(&ran(&run).0, "--resume"), Some(conversation));
    drop(fixture);
}

struct EnvVar {
    name: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVar {
    fn set(name: &'static str, dir: &Path) -> Self {
        let previous = std::env::var_os(name);
        // SAFETY: nextest runs each test in its own process, so this test owns the variable
        // while the guard is alive and restores it on drop.
        unsafe { std::env::set_var(name, dir) };
        Self { name, previous }
    }
}

impl Drop for EnvVar {
    fn drop(&mut self) {
        // SAFETY: see `EnvVar::set`.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_restart_resumes_the_codex_conversation_without_start_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = fixture.dir.path().join("codex-home");
    let run = fixture.dir.path().join("codex-run");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let conversation = began(&home, fixture.dir.path(), later());
    std::fs::remove_file(run.join("out").join("env")).unwrap();
    let _home = EnvVar::set("CODEX_HOME", &home);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(super::pid_of(&id, cx).is_some());
    let args = ran(&run).0;
    assert!(
        args.ends_with(&["resume".to_owned(), conversation]),
        "{args:?}"
    );
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_restart_resumes_the_cursor_conversation_without_start_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("cursor-run");
    let program = fake_agent(&run, "cursor-agent");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    let conversation = after(&ran(&run).0, "--new-session-id").expect("a conversation id");
    let config = fixture.dir.path().join("cursor");
    wrote(
        &config,
        &dunce::canonicalize(fixture.dir.path()).unwrap(),
        &conversation,
    );
    std::fs::remove_file(run.join("out").join("env")).unwrap();
    let _config = EnvVar::set("CURSOR_CONFIG_DIR", &config);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
    assert!(super::pid_of(&id, cx).is_some());
    assert_eq!(after(&ran(&run).0, "--resume"), Some(conversation));
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_line_that_picks_its_conversation_gets_none_from_kagan(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    claude_config(&fixture, cx);
    codex_home(&fixture, cx);
    cursor_config(&fixture, cx);
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
        ("opencode", "--session abc"),
        ("pi", "--continue"),
        ("kimi", "--session abc"),
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
            if agent == "opencode" {
                assert_eq!(args, &person(rest), "{rest}: {args:?}");
            }
            if agent == "cursor-agent" || agent == "agent" {
                if rest == "login" {
                    assert_eq!(args, &person(rest), "{rest}: {args:?}");
                } else {
                    let mut expected = vec![
                        "--plugin-dir".to_owned(),
                        cursor_plugin(&fixture, &id).display().to_string(),
                    ];
                    expected.extend(person(rest));
                    assert_eq!(args, &expected, "{rest}: {args:?}");
                }
            }
        }
        assert!(notices(&id, cx).await.is_empty(), "{rest}");
    }
}

fn cursor_plugin(fixture: &Fixture, id: &ThreadId) -> PathBuf {
    dunce::canonicalize(fixture.dir.path())
        .unwrap()
        .join("agent")
        .join("threads")
        .join(&id.0)
        .join("cursor")
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
        workspace.homes.insert("CURSOR_CONFIG_DIR", config.clone());
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
        workspace.homes.insert("CODEX_HOME", home.clone());
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

#[cfg(unix)]
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

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_resumes_the_cursor_conversation_the_first_start_named(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let config = cursor_config(&fixture, cx);
    let folder = dunce::canonicalize(fixture.dir.path()).unwrap();
    let elsewhere = folder.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let run = fixture.dir.path().join("cursor-run");
    let program = fake_agent(&run, "cursor-agent");
    let (id, _reap) = started(&fixture, &format!("{} --person-arg", program.display()), cx).await;
    let plugin = cursor_plugin(&fixture, &id);
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
            cursor_plugin(&fixture, &id).display().to_string(),
            "--resume".to_owned(),
            conversation,
        ]
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_worktree_name_is_not_a_cursor_subcommand(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    cursor_config(&fixture, cx);
    let lines = [("-w", "agent"), ("--worktree", "login")];
    for (number, (flag, name)) in lines.into_iter().enumerate() {
        let run = fixture.dir.path().join(format!("worktree-{number}"));
        let program = fake_agent(&run, "cursor-agent");
        let (id, _reap) = started(
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
                cursor_plugin(&fixture, &id).display().to_string(),
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

#[cfg(unix)]
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

fn kimi_home(fixture: &Fixture, cx: &mut TestAppContext) -> PathBuf {
    let home = fixture.dir.path().join("kimi-home");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.homes.insert("KIMI_CODE_HOME", home.clone());
    });
    home
}

fn kimi_session(home: &Path, folder: &Path) -> String {
    let conversation = Conversation::fresh().to_string();
    let dir = home
        .join("sessions")
        .join("wd")
        .join(format!("session_{conversation}"));
    std::fs::create_dir_all(&dir).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let state = json!({
        "id": format!("session_{conversation}"),
        "cwd": folder,
        "createdAt": i64::try_from(now).unwrap() + 3_600_000,
    });
    std::fs::write(dir.join("state.json"), format!("{state}\n")).unwrap();
    conversation
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_keeps_the_opencode_and_pi_session_it_named(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let open = fixture.dir.path().join("opencode-run");
    let program = fake_agent(&open, "opencode");
    let (open_id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    let open_session = after(&ran(&open).0, "--session").unwrap();
    stop(&open_id, cx).await;
    assert_eq!(
        after(&start_again(&open_id, &open, cx).await, "--session"),
        Some(open_session.clone())
    );

    let pi = fixture.dir.path().join("pi-run");
    let program = fake_agent(&pi, "pi");
    let (pi_id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    let pi_session = after(&ran(&pi).0, "--session-id").unwrap();
    stop(&pi_id, cx).await;
    assert_eq!(
        after(&start_again(&pi_id, &pi, cx).await, "--session-id"),
        Some(pi_session.clone())
    );

    std::fs::remove_file(open.join("out").join("env")).unwrap();
    std::fs::remove_file(pi.join("out").join("env")).unwrap();
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(after(&ran(&open).0, "--session"), Some(open_session));
    assert_eq!(after(&ran(&pi).0, "--session-id"), Some(pi_session));
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_resumes_the_one_kimi_conversation_in_its_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-run");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let conversation = kimi_session(&home, fixture.dir.path());
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(
        again.ends_with(&["--session".to_owned(), format!("session_{conversation}")]),
        "{again:?}"
    );
    assert!(notices(&id, cx).await.is_empty());
    assert_eq!(remembered(&fixture, &id), Some(conversation.clone()));

    std::fs::remove_file(run.join("out").join("env")).unwrap();
    let _home = EnvVar::set("KIMI_CODE_HOME", &home);
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Active);
    assert_eq!(
        after(&ran(&run).0, "--session"),
        Some(format!("session_{conversation}"))
    );
    drop(fixture);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_on_kimi_starts_a_new_conversation_when_two_match(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-two");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    kimi_session(&home, fixture.dir.path());
    kimi_session(&home, fixture.dir.path());
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(after(&again, "--session").is_none(), "{again:?}");
    assert!(
        notices(&id, cx)
            .await
            .iter()
            .any(|text| text.contains("could not tell which Kimi conversation"))
    );
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_kimi_conversation_is_kept_once_its_file_appears(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-keep");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let conversation = kimi_session(&home, fixture.dir.path());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store.conversation(&id).unwrap().unwrap().to_string(),
        conversation
    );
    let other = kimi_session(&home, fixture.dir.path());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(
        again.ends_with(&["--session".to_owned(), format!("session_{conversation}")]),
        "{again:?}"
    );
    assert!(!again.iter().any(|arg| arg.contains(&other)));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_codex_conversation_is_kept_once_its_file_appears(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = codex_home(&fixture, cx);
    let run = fixture.dir.path().join("codex-keep");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let conversation = began(&home, fixture.dir.path(), later());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store.conversation(&id).unwrap().unwrap().to_string(),
        conversation
    );
    let other = began(&home, fixture.dir.path(), later());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(
        again.ends_with(&["resume".to_owned(), conversation]),
        "{again:?}"
    );
    assert!(!again.iter().any(|arg| arg == &other));
}

fn tick(cx: &mut TestAppContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
}

fn remembered(fixture: &Fixture, id: &ThreadId) -> Option<String> {
    kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3"))
        .unwrap()
        .conversation(id)
        .unwrap()
        .map(|conversation| conversation.to_string())
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_kimi_file_that_appears_after_an_empty_look_is_saved(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-later");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    tick(cx);
    assert_eq!(remembered(&fixture, &id), None);
    let conversation = kimi_session(&home, fixture.dir.path());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(conversation));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn two_kimi_files_before_the_first_look_stay_unsaved(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-both");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    kimi_session(&home, fixture.dir.path());
    kimi_session(&home, fixture.dir.path());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), None);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn two_kimi_threads_in_one_folder_keep_different_conversations(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let one = fixture.dir.path().join("kimi-a");
    let two = fixture.dir.path().join("kimi-b");
    let first_program = fake_agent(&one, "kimi");
    let second_program = fake_agent(&two, "kimi");
    let (first, _one) = started(&fixture, &first_program.display().to_string(), cx).await;
    let (second, _two) = started(&fixture, &second_program.display().to_string(), cx).await;
    ran(&one);
    ran(&two);
    let earlier = kimi_session(&home, fixture.dir.path());
    tick(cx);
    let saved = [remembered(&fixture, &first), remembered(&fixture, &second)];
    assert_eq!(
        saved
            .iter()
            .filter(|id| id.as_deref() == Some(earlier.as_str()))
            .count(),
        1,
        "{saved:?}"
    );
    let later = kimi_session(&home, fixture.dir.path());
    tick(cx);
    let saved = [remembered(&fixture, &first), remembered(&fixture, &second)];
    assert!(saved.contains(&Some(earlier)), "{saved:?}");
    assert!(saved.contains(&Some(later)), "{saved:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_lost_kimi_file_is_replaced_by_the_run_that_follows(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-lost");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let conversation = kimi_session(&home, fixture.dir.path());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(conversation.clone()));
    stop(&id, cx).await;
    std::fs::remove_dir_all(
        home.join("sessions")
            .join("wd")
            .join(format!("session_{conversation}")),
    )
    .unwrap();
    let again = start_again(&id, &run, cx).await;
    assert!(after(&again, "--session").is_none(), "{again:?}");
    let replacement = kimi_session(&home, fixture.dir.path());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(replacement.clone()));
    kimi_session(&home, fixture.dir.path());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(replacement));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn two_codex_threads_in_one_folder_keep_different_conversations(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = codex_home(&fixture, cx);
    let one = fixture.dir.path().join("codex-a");
    let two = fixture.dir.path().join("codex-b");
    let first_program = fake_agent(&one, "codex");
    let second_program = fake_agent(&two, "codex");
    let (first, _one) = started(&fixture, &first_program.display().to_string(), cx).await;
    let (second, _two) = started(&fixture, &second_program.display().to_string(), cx).await;
    ran(&one);
    ran(&two);
    let earlier = began(&home, fixture.dir.path(), later());
    tick(cx);
    let saved = [remembered(&fixture, &first), remembered(&fixture, &second)];
    assert_eq!(
        saved
            .iter()
            .filter(|id| id.as_deref() == Some(earlier.as_str()))
            .count(),
        1,
        "{saved:?}"
    );
    let later = began(&home, fixture.dir.path(), later());
    tick(cx);
    let saved = [remembered(&fixture, &first), remembered(&fixture, &second)];
    assert!(saved.contains(&Some(earlier)), "{saved:?}");
    assert!(saved.contains(&Some(later)), "{saved:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_lost_codex_file_is_replaced_by_the_run_that_follows(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = codex_home(&fixture, cx);
    let run = fixture.dir.path().join("codex-lost");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let conversation = began(&home, fixture.dir.path(), later());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(conversation.clone()));
    stop(&id, cx).await;
    forget(&home, &conversation);
    let again = start_again(&id, &run, cx).await;
    assert!(!again.contains(&conversation), "{again:?}");
    let replacement = began(&home, fixture.dir.path(), later());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(replacement.clone()));
    began(&home, fixture.dir.path(), later());
    tick(cx);
    assert_eq!(remembered(&fixture, &id), Some(replacement));
}

fn left_to_resume(fixture: &Fixture) -> std::collections::BTreeSet<ThreadId> {
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    resume_threads(&store).unwrap()
}

#[test]
fn a_resume_list_reads_back_what_was_set_and_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let store = kg_store::Store::open(dir.path().join("kagan.sqlite3")).unwrap();
    let a = ThreadId("thr_a".into());
    let b = ThreadId("thr_b".into());
    assert!(resume_threads(&store).unwrap().is_empty());
    set_resume_threads(&store, &std::collections::BTreeSet::from([a.clone()])).unwrap();
    assert_eq!(
        resume_threads(&store).unwrap(),
        std::collections::BTreeSet::from([a.clone()])
    );
    remember_resume(&store, std::slice::from_ref(&b)).unwrap();
    assert_eq!(
        resume_threads(&store).unwrap(),
        std::collections::BTreeSet::from([a, b])
    );
}

#[gpui_kit::test]
async fn a_remembered_thread_that_cannot_resume_is_forgotten_at_open(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let gone = ThreadId("thr_gone".into());
    let fixture = fixture
        .restart(cx, |store| {
            set_resume_threads(store, &std::collections::BTreeSet::from([gone.clone()])).unwrap();
        })
        .await;
    cx.run_until_parked();
    assert!(left_to_resume(&fixture).is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn an_idle_thread_whose_folder_is_gone_stays_idle_when_it_cannot_resume(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    stop(&id, cx).await;
    let environment = super::thread(&id, cx).await.environment_id.unwrap();
    let fixture = fixture
        .restart(cx, |store| {
            set_resume_threads(store, &std::collections::BTreeSet::from([id.clone()])).unwrap();
            let mut folder = store.environment(&environment).unwrap().unwrap();
            folder.status = EnvironmentStatus::Error;
            store.put_environment(&folder).unwrap();
        })
        .await;
    cx.run_until_parked();
    assert_eq!(super::thread(&id, cx).await.status, ThreadStatus::Idle);
    assert!(left_to_resume(&fixture).is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_codex_line_that_picks_its_conversation_keeps_no_file_of_kagan(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = codex_home(&fixture, cx);
    let run = fixture.dir.path().join("codex-picked");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(
        &fixture,
        &format!("{} resume --last", program.display()),
        cx,
    )
    .await;
    ran(&run);
    began(&home, fixture.dir.path(), later());
    tick(cx);
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert!(store.conversation(&id).unwrap().is_none());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_kimi_line_that_picks_its_conversation_keeps_no_file_of_kagan(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-picked");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(
        &fixture,
        &format!("{} --session abc", program.display()),
        cx,
    )
    .await;
    ran(&run);
    kimi_session(&home, fixture.dir.path());
    tick(cx);
    let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
    assert!(store.conversation(&id).unwrap().is_none());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn start_again_on_kimi_ignores_a_conversation_begun_in_another_folder(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let home = kimi_home(&fixture, cx);
    let run = fixture.dir.path().join("kimi-elsewhere");
    let program = fake_agent(&run, "kimi");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    let elsewhere = fixture.dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    kimi_session(&home, &elsewhere);
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(after(&again, "--session").is_none(), "{again:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_first_start_of_opencode_says_nothing_about_a_conversation(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("opencode-first");
    let program = fake_agent(&run, "opencode");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    assert!(notices(&id, cx).await.is_empty());
}
