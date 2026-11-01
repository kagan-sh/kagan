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
            EventKind::Notice { text, .. } => Some(text),
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
    let run = fixture.dir.path().join("claude-run");
    let program = fake_agent(&run, "claude");
    let (id, _reap) = started(&fixture, &format!("{} --resume=abc", program.display()), cx).await;
    let (args, _) = ran(&run);
    assert!(after(&args, "--session-id").is_none(), "{args:?}");
    assert_eq!(args.last().map(String::as_str), Some("--resume=abc"));
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert_eq!(again.last().map(String::as_str), Some("--resume=abc"));
    assert!(notices(&id, cx).await.is_empty());
}

#[gpui_kit::test]
async fn start_again_on_codex_starts_a_new_conversation_and_says_so(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let run = fixture.dir.path().join("codex-run");
    let program = fake_agent(&run, "codex");
    let (id, _reap) = started(&fixture, &program.display().to_string(), cx).await;
    ran(&run);
    assert!(notices(&id, cx).await.is_empty());
    stop(&id, cx).await;
    let again = start_again(&id, &run, cx).await;
    assert!(!again.iter().any(|arg| arg == "resume"), "{again:?}");
    assert!(
        notices(&id, cx)
            .await
            .iter()
            .any(|text| text.contains("cannot resume a Codex conversation")),
    );
}
