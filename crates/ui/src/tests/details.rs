use gpui_kit::TestAppContext;

use super::kagan::{click, open, present, repo_thread, run_shell, shows, value_of, wait_until};
use crate::kagan::changes::POLL;

fn head(repo: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[gpui_kit::test]
fn details_shows_the_folder_program_and_branch_of_the_thread(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_, repo) = repo_thread(cx, window, data.path());
    click(cx, window, "details-header");
    wait_until(cx, |cx| present(cx, window, "details-branch"));
    let branch = head(&repo);
    assert_eq!(value_of(cx, window, "details-branch"), Some(branch.clone()));
    assert_eq!(
        value_of(cx, window, "details-base"),
        Some(format!("{branch} · up to date"))
    );
    let folder = value_of(cx, window, "details-folder").unwrap();
    assert!(folder.ends_with("/repo"), "{folder}");
    assert_eq!(
        value_of(cx, window, "details-environment").as_deref(),
        Some("Project folder")
    );
    assert_eq!(
        value_of(cx, window, "details-program").as_deref(),
        Some("/bin/sh")
    );
    assert!(present(cx, window, "details-commits"));

    click(cx, window, "details-copy-folder");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, value_of(cx, window, "details-folder"));

    std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["checkout", "-qb", "topic"])
        .status()
        .unwrap();
    wait_until(cx, |cx| {
        cx.executor().advance_clock(POLL);
        value_of(cx, window, "details-branch").as_deref() == Some("topic")
    });
}

#[gpui_kit::test]
fn details_says_when_the_folder_is_not_git(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "details-header");
    wait_until(cx, |cx| present(cx, window, "details-not-git"));
    assert!(shows(cx, window, "details"));
}
