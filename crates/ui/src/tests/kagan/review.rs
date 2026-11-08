use super::chord;
use crate::kagan::review_read;
use crate::kagan::screen_read;
use std::path::Path;

use gpui_kit::component::{ActiveTheme as _, ThemeMode};
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::{EnvironmentSpec, Op, OpResult, Run, ThreadOp};

use crate::kagan::changes::POLL;

use super::{
    click, committed_repo, git, open, open_lines, painted, repo_thread, run_shell, selected_of,
    shell, shows, value_of, wait_until,
};
use crate::theme::follow_system;

#[gpui_kit::test]
fn a_rust_change_colors_a_keyword(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("main.rs"), "fn main() {}\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-main.rs"));
    click(cx, window, "change-main.rs");
    wait_until(cx, |cx| {
        cx.update(|cx| review_read::review_token(kagan.read(cx), kg_domain::DiffSide::New, 1, 0))
            == Some("keyword".to_owned())
    });
}

#[gpui_kit::test]
fn a_changed_file_opens_as_a_review_page_with_its_change_and_position(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.starts_with("@@"))
    });
    assert!(
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.starts_with("+ "))
    );
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("1 change")
    );
    assert_eq!(value_of(cx, window, "review-notice"), None);
}

// The edits are far apart so git returns two hunks.
pub(in crate::tests) fn two_changes(cx: &mut TestAppContext, window: AnyWindowHandle, repo: &Path) {
    let base: String = (1..=30)
        .map(|n| format!("line {n}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(repo.join("notes.txt"), &base).unwrap();
    git(repo, &["add", "notes.txt"]);
    git(repo, &["commit", "-qm", "base"]);
    let edited = base
        .replace("line 2\n", "two changed\n")
        .replace("line 28\n", "twenty-eight changed\n");
    std::fs::write(repo.join("notes.txt"), edited).unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
}

#[gpui_kit::test]
fn next_and_previous_change_move_between_the_hunks(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    two_changes(cx, window, &repo);
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    wait_until(cx, |cx| {
        value_of(cx, window, "review-position").as_deref() == Some("2 changes")
    });

    cx.simulate_keystrokes(window, &chord("NextHunk"));
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("Change 1 of 2")
    );
    cx.simulate_keystrokes(window, &chord("NextHunk"));
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("Change 2 of 2")
    );
    cx.simulate_keystrokes(window, &chord("PrevHunk"));
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("Change 1 of 2")
    );
}

#[gpui_kit::test]
fn escape_closes_the_review_and_focuses_the_terminal(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));

    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "review"));
    assert!(
        cx.update_window(window, |_, window, cx| screen_read::terminal_focused(
            kagan.read(cx),
            window,
            cx
        ))
        .unwrap()
    );
}

#[gpui_kit::test]
fn escape_with_the_search_open_closes_the_search_before_the_review(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));

    cx.simulate_keystrokes(window, &chord("FindInDiff"));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        shows(cx, window, "review"),
        "the first Escape closed the review"
    );
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "review"));
}

#[gpui_kit::test]
fn a_binary_file_says_so_and_shows_no_text(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("data.bin"), [0u8, 159, 146, 150]).unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-data.bin"));
    click(cx, window, "change-data.bin");
    wait_until(cx, |cx| {
        value_of(cx, window, "review-notice").is_some_and(|text| text.contains("binary"))
    });
}

#[gpui_kit::test]
fn an_unchanged_repository_says_no_changes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("notes.txt"), "one\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "changes-empty"));
}

#[gpui_kit::test]
fn a_folder_that_is_not_git_says_so(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let thread = run_shell(cx);
    click(cx, window, &format!("thread-{}", thread.0));
    wait_until(cx, |cx| shows(cx, window, "changes-not-git"));
    assert!(!shows(cx, window, "changes-error"));
}

#[gpui_kit::test]
fn find_in_the_review_moves_to_a_match(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    two_changes(cx, window, &repo);
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    cx.simulate_keystrokes(window, &chord("FindInDiff"));
    cx.run_until_parked();
    cx.simulate_input(window, "twenty-eight changed");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("NextMatch"));
    wait_until(cx, |cx| {
        cx.update(|cx| review_read::review_matched_text(kagan.read(cx)))
            .is_some_and(|text| text.contains("twenty-eight changed"))
    });
    assert!(shows(cx, window, "review-search"));
}

#[gpui_kit::test]
fn a_selection_in_the_review_copies(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    two_changes(cx, window, &repo);
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    wait_until(cx, |cx| !open_lines(cx, &kagan).is_empty());

    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    let copy = if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-shift-c"
    };
    cx.simulate_keystrokes(window, select_all);
    cx.run_until_parked();
    cx.simulate_keystrokes(window, copy);
    wait_until(cx, |cx| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains("twenty-eight changed"))
    });
}

#[gpui_kit::test]
fn switching_scope_selects_a_file_in_the_new_list(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    git(&repo, &["checkout", "-q", "-b", "feat"]);
    std::fs::write(repo.join("notes.txt"), "on branch\n").unwrap();
    git(&repo, &["add", "notes.txt"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "on branch",
        ],
    );
    std::fs::write(repo.join("other.txt"), "only uncommitted\n").unwrap();
    let run = Run {
        project_id: project.id,
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(started).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    click(cx, window, &format!("thread-{}", thread.id.0));
    wait_until(cx, |cx| shows(cx, window, "change-other.txt"));
    click(cx, window, "change-other.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));

    click(cx, window, "review-scope-Branch");
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "review-file-notes.txt"));
    assert!(shows(cx, window, "review"));
}

pub(in crate::tests) fn scope_filled(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &str,
) -> bool {
    let bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(id.to_owned().into()))
                .map(|element| element.bounds)
        })
        .unwrap();
    let Some(bounds) = bounds else {
        return false;
    };
    let color = cx.update(|cx| cx.theme().secondary);
    let scale = cx
        .update_window(window, |_, window, _| window.scale_factor())
        .unwrap();
    let center_x = (bounds.origin.x.as_f32() + bounds.size.width.as_f32() / 2.) * scale;
    let center_y = (bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.) * scale;
    let min_width = bounds.size.width.as_f32() * scale * 0.5;
    painted(cx, window).iter().any(|quad| {
        let origin_x = quad.bounds.origin.x.as_f32();
        let origin_y = quad.bounds.origin.y.as_f32();
        let width = quad.bounds.size.width.as_f32();
        let height = quad.bounds.size.height.as_f32();
        quad.background == color.into()
            && width > min_width
            && origin_x <= center_x
            && center_x <= origin_x + width
            && origin_y <= center_y
            && center_y <= origin_y + height
    })
}

#[gpui_kit::test]
fn the_selected_scope_stays_filled_and_a_zero_count_is_blank(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_id, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("fresh.txt"), "one\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-fresh.txt"));
    assert_eq!(
        selected_of(cx, window, "changes-scope-uncommitted"),
        Some(true)
    );
    assert_eq!(selected_of(cx, window, "changes-scope-branch"), Some(false));
    assert!(scope_filled(cx, window, "changes-scope-uncommitted"));
    assert!(!scope_filled(cx, window, "changes-scope-branch"));
    let fresh = value_of(cx, window, "change-fresh.txt").unwrap_or_default();
    assert!(fresh.starts_with('+'), "{fresh}");
    assert!(!fresh.contains('0'), "{fresh}");
    assert!(!fresh.contains('−'), "{fresh}");
    click(cx, window, "changes-scope-branch");
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    assert_eq!(selected_of(cx, window, "changes-scope-branch"), Some(true));
    assert_eq!(
        selected_of(cx, window, "changes-scope-uncommitted"),
        Some(false)
    );
    assert!(scope_filled(cx, window, "changes-scope-branch"));
    assert!(!scope_filled(cx, window, "changes-scope-uncommitted"));
}

#[gpui_kit::test]
fn a_delayed_list_for_the_old_scope_does_not_replace_the_branch_list(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    git(&repo, &["checkout", "-q", "-b", "feat"]);
    std::fs::write(repo.join("notes.txt"), "on branch\n").unwrap();
    git(&repo, &["add", "notes.txt"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "on branch",
        ],
    );
    let run = Run {
        project_id: project.id,
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(started).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    click(cx, window, &format!("thread-{}", thread.id.0));
    click(cx, window, "changes-scope-branch");
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "changes-scope-uncommitted");
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    std::fs::write(repo.join("other.txt"), "only uncommitted\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-other.txt"));
    click(cx, window, "changes-scope-branch");
    let _ = std::fs::remove_file(repo.join("other.txt"));
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    assert!(!shows(cx, window, "change-other.txt"));

    let stale = cx.update(|cx| {
        Workspace::dispatch(
            Op::Diff(kg_domain::DiffOp::List {
                query: kg_domain::DiffQuery {
                    thread_id: thread.id.clone(),
                    scope: kg_domain::DiffScope::Uncommitted,
                    base: None,
                    rev: None,
                },
            }),
            cx,
        )
    });
    let listed = Ok(cx.foreground_executor().block_test(stale).unwrap());
    cx.update(|cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.finish_list_read_for_test(
                thread.id.clone(),
                kg_domain::DiffScope::Uncommitted,
                listed,
                cx,
            );
        });
    });
    cx.run_until_parked();
    assert!(shows(cx, window, "change-notes.txt"));
    assert!(!shows(cx, window, "change-other.txt"));
}

#[gpui_kit::test]
fn open_review_after_a_scope_switch_opens_a_listed_file(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    git(&repo, &["checkout", "-q", "-b", "feat"]);
    std::fs::write(repo.join("notes.txt"), "on branch\n").unwrap();
    git(&repo, &["add", "notes.txt"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "on branch",
        ],
    );
    std::fs::write(repo.join("other.txt"), "only uncommitted\n").unwrap();
    let run = Run {
        project_id: project.id,
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(started).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    click(cx, window, &format!("thread-{}", thread.id.0));
    wait_until(cx, |cx| shows(cx, window, "change-other.txt"));
    click(cx, window, "change-other.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));

    click(cx, window, "review-scope-Branch");
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "review-file-notes.txt"));
    assert!(shows(cx, window, "review"));
    click(cx, window, "review-file-notes.txt");
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("on branch"))
    });
    assert_eq!(value_of(cx, window, "review-notice"), None);
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("1 change")
    );
}

#[gpui_kit::test]
fn a_live_disk_update_resets_the_change_position(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    two_changes(cx, window, &repo);
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    cx.simulate_keystrokes(window, &chord("NextHunk"));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("NextHunk"));
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("Change 2 of 2")
    );

    let edited = std::fs::read_to_string(repo.join("notes.txt"))
        .unwrap()
        .replace("twenty-eight changed", "twenty-eight touched");
    std::fs::write(repo.join("notes.txt"), edited).unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("twenty-eight touched"))
    });
    assert_eq!(
        value_of(cx, window, "review-position").as_deref(),
        Some("2 changes")
    );
}

#[gpui_kit::test]
fn search_finds_multibyte_text_in_review_rows(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("notes.txt"), "one\n🔥 changed\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    cx.simulate_keystrokes(window, &chord("FindInDiff"));
    cx.run_until_parked();
    cx.simulate_input(window, "🔥 changed");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("NextMatch"));
    wait_until(cx, |cx| {
        cx.update(|cx| review_read::review_matched_text(kagan.read(cx)))
            .is_some_and(|text| text.contains("🔥 changed"))
    });
}

#[gpui_kit::test]
fn changing_theme_repaints_review_rows(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    let before = cx.update(|cx| crate::theme::added(cx).opacity(0.10));
    assert!(
        painted(cx, window)
            .iter()
            .any(|quad| quad.background == before.into())
    );
    cx.update(|cx| follow_system(ThemeMode::Dark, cx));
    cx.run_until_parked();
    let after = cx.update(|cx| crate::theme::added(cx).opacity(0.10));
    assert_ne!(before, after);
    assert!(
        painted(cx, window)
            .iter()
            .any(|quad| quad.background == after.into())
    );
}

#[gpui_kit::test]
fn a_stale_file_read_does_not_replace_the_open_review(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("other.txt"), "only in other\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-other.txt"));

    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review-file-other.txt"));
    click(cx, window, "review-file-other.txt");
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("only in other"))
    });
    wait_until(cx, |cx| {
        !open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("+     2     2 two"))
    });
}

#[gpui_kit::test]
fn a_long_diff_line_scrolls_horizontally(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    let tail = "x".repeat(40);
    let long = format!("{}{tail}", "x".repeat(760));
    std::fs::write(repo.join("notes.txt"), format!("one\n{long}\n")).unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains(&tail))
    });

    wait_until(cx, |cx| {
        cx.update(|cx| {
            review_read::review_horizontal_limit(kagan.read(cx)).is_some_and(|limit| limit > 0.)
        })
    });
}
