use super::chord;
use crate::kagan::review_read;
#[cfg(unix)]
use crate::kagan::screen_read;
#[cfg(unix)]
use std::time::Duration;

use gpui_kit::point;
use gpui_kit::{AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
#[cfg(unix)]
use kg_domain::ThreadStatus;
use kg_domain::{EnvironmentSpec, Op, OpResult, ProjectId, Run, ThreadOp, Visibility};

use crate::kagan::changes::POLL;

#[cfg(unix)]
use super::notice_of;
use super::{
    add_project, approvals_waiting, click, focused_of, hover_to, label, open, present, repo_thread,
    run_shell, selected_of, set_preference, shell, shows, two_changes, value_of, wait_for_screen,
    wait_until,
};
#[cfg(unix)]
use super::{grid_text, line, run_program, thread_status};

#[gpui_kit::test]
fn remove_project_focuses_cancel_in_the_narrow_questions_panel(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("project-to-forget");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Remove { id: project.id }),
            cx,
        )
    });
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    let asked = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .clone()
    });
    assert_eq!(asked.options[0].label, "Cancel");
    assert_eq!(asked.options[1].label, "Remove");
    let cancel = format!("answer-{}-decline", asked.id);
    let remove = format!("answer-{}-approve", asked.id);
    set_preference(cx, "main.right-sidebar", serde_json::json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    assert_eq!(focused_of(cx, window, &cancel), Some(true));
    assert_ne!(focused_of(cx, window, &remove), Some(true));
    let answered =
        cx.update(|cx| Workspace::answer_standalone(asked.id.clone(), "decline".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    assert!(cx.foreground_executor().block_test(removing).is_err());
}

#[gpui_kit::test]
fn review_tree_filter_fold_and_viewed(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    two_changes(cx, window, &repo);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/other.txt"), "other\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-src/other.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review-file-src/other.txt"));
    assert!(shows(cx, window, "review-folder-src"));
    let fold = (0..40)
        .map(|n| format!("review-fold-{n}"))
        .find(|id| shows(cx, window, id))
        .expect("a folded run");
    click(cx, window, &fold);
    assert!(!shows(cx, window, &fold));
    wait_until(cx, |cx| {
        selected_of(cx, window, "review-viewed") == Some(false)
    });
    click(cx, window, "review-viewed");
    assert_eq!(selected_of(cx, window, "review-viewed"), Some(true));
    std::fs::write(repo.join("notes.txt"), "changed again\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        selected_of(cx, window, "review-viewed") == Some(false)
    });
    click(cx, window, "review-filter");
    cx.simulate_input(window, "other");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "review-filter").as_deref(),
        Some("other")
    );
    assert!(shows(cx, window, "review-file-src/other.txt"));
    assert!(!shows(cx, window, "review-file-notes.txt"));
    click(cx, window, "review-path");
    assert!(!shows(cx, window, "review-folder-src"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn review_tree_keyboard_follows_visible_rows(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/other.txt"), "other\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-src/other.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review-folder-src"));
    cx.simulate_keystrokes(window, "k");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "review-file-src/other.txt"),
        Some(true)
    );
    cx.simulate_keystrokes(window, "j");
    cx.run_until_parked();
    assert_eq!(selected_of(cx, window, "review-file-notes.txt"), Some(true));
    click(cx, window, "review-file-src/other.txt");
    cx.simulate_keystrokes(window, "j");
    cx.run_until_parked();
    assert_eq!(selected_of(cx, window, "review-file-notes.txt"), Some(true));
    click(cx, window, "review-folder-src");
    cx.simulate_keystrokes(window, "right");
    cx.run_until_parked();
    assert!(shows(cx, window, "review-file-src/other.txt"));
    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    assert_eq!(
        focused_of(cx, window, "review-file-src/other.txt"),
        Some(true)
    );
    cx.simulate_keystrokes(window, "end enter");
    cx.run_until_parked();
    assert_eq!(selected_of(cx, window, "review-file-notes.txt"), Some(true));
}

#[gpui_kit::test]
fn open_review_takes_the_keyboard_so_j_moves_at_once(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("other.txt"), "other\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    cx.simulate_window_resize(window, size(px(480.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "change-other.txt"));
    click(cx, window, "changes-open-review");
    cx.run_until_parked();
    let first = label(cx, window, "review");
    cx.simulate_keystrokes(window, "j");
    cx.run_until_parked();
    assert_ne!(label(cx, window, "review"), first);
}

#[cfg(unix)]
#[gpui_kit::test]
fn review_comments_are_pasted_without_submitting(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    cx.simulate_input(window, "Please check this");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "review-comment-input").as_deref(),
        Some("Please check this")
    );
    assert_eq!(
        crate::tests::plugin::panels::disabled_of(cx, window, "review-send"),
        Some(true)
    );
    click(cx, window, "review-add-comment");
    assert!(!shows(cx, window, "review-add-comment"));
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
    assert_eq!(
        crate::tests::plugin::panels::disabled_of(cx, window, "review-send"),
        Some(false)
    );
    click(cx, window, "comment-New-2");
    cx.simulate_keystrokes(
        window,
        if cfg!(target_os = "macos") {
            "cmd-enter"
        } else {
            "ctrl-shift-enter"
        },
    );
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    assert!(!shows(cx, window, "review"));
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    // The terminal wraps the pasted line at its width.
    let screen = grid_text(cx, &kagan).replace('\n', "");
    assert!(screen.contains("Please check this"), "{screen}");
    assert!(!screen.contains("command not found"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn sending_review_adds_the_focused_draft(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    cx.simulate_input(window, "draft sent directly");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("SendReview"));
    wait_until(cx, |cx| !shows(cx, window, "review"));
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("directly"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn review_discard_clears_saved_comments(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    cx.simulate_input(window, "A note");
    click(cx, window, "review-add-comment");
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
    click(cx, window, "review-discard");
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        0
    );
    assert_eq!(
        crate::tests::plugin::panels::disabled_of(cx, window, "review-send"),
        Some(true)
    );
    click(cx, window, "review-undo-discard");
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_comment_box_fits_the_minimum_window(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    wait_until(cx, |cx| shows(cx, window, "review-comment-input"));
    let (x, width) =
        crate::tests::sidebar::layout::span(cx, window, "review-comment-input").expect("the box");
    assert!(
        x >= 0. && x + width <= 528.5,
        "box {x}+{width} leaves the window"
    );
}

#[gpui_kit::test]
fn review_comment_keys_add_and_cancel(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    cx.simulate_input(window, "first");
    cx.simulate_keystrokes(window, "shift-enter");
    cx.simulate_input(window, "second");
    assert_eq!(
        value_of(cx, window, "review-comment-input").as_deref(),
        Some("first\nsecond")
    );
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
    click(cx, window, "comment-New-2");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "review-comment-input"));
    assert!(shows(cx, window, "review"));
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
    click(cx, window, "review-remove-0");
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        0
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn review_draft_keeps_its_file_and_survives_closing(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("other.txt"), "other\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        shows(cx, window, "change-notes.txt") && shows(cx, window, "change-other.txt")
    });
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    cx.simulate_input(window, "keep this draft");
    cx.run_until_parked();
    click(cx, window, "review-file-other.txt");
    assert!(!shows(cx, window, "review-add-comment"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review-comment-input"));
    assert_eq!(
        value_of(cx, window, "review-comment-input").as_deref(),
        Some("keep this draft")
    );
    click(cx, window, "review-add-comment");
    click(cx, window, "review-send");
    wait_until(cx, |cx| !shows(cx, window, "review"));
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains("keep this draft")
    });
    let screen = grid_text(cx, &kagan);
    assert!(screen.contains("file: notes.txt"), "{screen}");
    assert!(screen.contains("scope: uncommitted"), "{screen}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_review_the_program_cannot_take_keeps_its_notes(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    click(cx, window, "comment-New-2");
    cx.simulate_input(window, "kept");
    click(cx, window, "review-add-comment");
    let exit = ThreadOp::Input {
        id: thread.clone(),
        text: line("exit"),
        terminal: None,
    };
    let exited = cx.update(|cx| Workspace::dispatch(Op::Thread(exit), cx));
    cx.foreground_executor().block_test(exited).unwrap();
    wait_until(cx, |cx| thread_status(cx, &thread) == ThreadStatus::Idle);
    click(cx, window, "review-send");
    wait_until(cx, |cx| {
        notice_of(cx, &kagan).is_some_and(|notice| notice.contains("ended"))
    });
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_held_paste_reaches_the_program_after_its_new_size(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_program(
        cx,
        "/bin/sh -c \"stty -icanon -echo min 1; echo ready; dd bs=1 count=1 2>/dev/null >/dev/null; echo size $(stty size) end; exec sleep 60\"",
    );
    click(cx, window, &format!("thread-{}", id.0));
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("ready"));
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    let view = cx
        .update(|cx| screen_read::terminal_view(kagan.read(cx)))
        .unwrap();
    let sent = view.update(cx, |view, cx| view.paste_after_fit("x".into(), cx));
    cx.simulate_window_resize(window, size(px(900.), px(720.)));
    // Long enough for the program to answer a paste that was not held.
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(
        !grid_text(cx, &kagan).contains("size"),
        "pasted before the terminal was drawn"
    );
    cx.simulate_keystrokes(window, "escape");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("end"));
    assert!(cx.foreground_executor().block_test(sent));
    let cols = cx
        .update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
        .unwrap()
        .lines[0]
        .len();
    let screen = grid_text(cx, &kagan);
    assert!(screen.contains(&format!(" {cols} end")), "{cols}: {screen}");
}

#[gpui_kit::test]
fn narrow_review_switches_between_file_and_tree(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    cx.simulate_window_resize(window, size(px(789.), px(720.)));
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "review-toggle-files"));
    assert!(!shows(cx, window, "review-file-notes.txt"));
    for id in [
        "review-toggle-files",
        "review-viewed",
        "review-prev-file",
        "review-next-file",
        "review-previous",
        "review-next",
        "review-send",
        "review-close",
        "review-find",
    ] {
        let (x, width) = crate::tests::sidebar::layout::span(cx, window, id).expect(id);
        assert!(x >= 0. && x + width <= 789.5, "{id}: {x}+{width}");
    }
    let (_, width) =
        crate::tests::sidebar::layout::span(cx, window, "review-title").expect("title");
    assert!(width > 40., "title width {width}");
    cx.simulate_window_resize(window, size(px(560.), px(720.)));
    cx.run_until_parked();
    for id in ["review-close", "review-find", "review-viewed"] {
        let (x, width) = crate::tests::sidebar::layout::span(cx, window, id).expect(id);
        assert!(x >= 0. && x + width <= 560.5, "{id} at 560: {x}+{width}");
    }
    cx.simulate_window_resize(window, size(px(789.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "review-toggle-files");
    assert!(shows(cx, window, "review-file-notes.txt"));
    assert!(!shows(cx, window, "comment-New-2"));
    click(cx, window, "review-file-notes.txt");
    assert!(shows(cx, window, "comment-New-2"));
    assert!(!shows(cx, window, "review-file-notes.txt"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn review_closes_the_overlay_and_restores_navigation(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("other.txt"), "other\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "change-other.txt"));
    click(cx, window, "changes-open-review");
    wait_until(cx, |cx| shows(cx, window, "review"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(label(cx, window, "review-viewed-count"), "0 of 2 viewed");
    assert_eq!(
        label(cx, window, "review-comment-hint"),
        "Hover a line and click + to comment. Drag line numbers to pick several."
    );
    for id in [
        "review-viewed-count",
        "review-comment-hint",
        "review-send",
        "review-discard",
    ] {
        let (x, width) = crate::tests::sidebar::layout::span(cx, window, id).expect(id);
        assert!(x >= 0. && x + width <= 528.5, "{id}: {x}+{width}");
    }
    assert_eq!(label(cx, window, "review-send"), "Send review to terminal");
    assert_eq!(label(cx, window, "review-discard"), "Discard comments");
    let first = label(cx, window, "review");
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "right-overlay-panel"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert!(shows(cx, window, "review"));
    cx.simulate_keystrokes(window, "j");
    cx.run_until_parked();
    let second = label(cx, window, "review");
    assert_ne!(second, first);
    cx.simulate_keystrokes(window, "k");
    cx.run_until_parked();
    assert_eq!(label(cx, window, "review"), first);
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "right-overlay-panel"));
    click(cx, window, "right-sidebar-toggle");
    assert!(!shows(cx, window, "right-overlay-panel"));
    cx.simulate_keystrokes(window, "j");
    cx.run_until_parked();
    assert_eq!(label(cx, window, "review"), second);
}

#[gpui_kit::test]
fn sidebar_controls_follow_hover_and_focus(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let thread = run_shell(cx);
    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    assert!(shows(cx, window, "project-new-prj_kagan"));
    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("thread-archive-{thread}")));
    let row = format!("thread-{thread}");
    let bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(row.into()))
                .unwrap()
                .bounds
        })
        .unwrap();
    hover_to(
        cx,
        window,
        point(bounds.origin.x + px(5.), bounds.origin.y + px(5.)),
    );
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("thread-actions-{thread}")));
}

#[gpui_kit::test]
fn sidebar_nests_and_collapses_child_threads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let parent = run_shell(cx);
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Visible),
        parent_thread_id: Some(parent.clone()),
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let task = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(child) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    let parent_row = format!("thread-{parent}");
    let child_row = format!("thread-{}", child.id);
    let bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(parent_row.clone().into()))
                .unwrap()
                .bounds
        })
        .unwrap();
    let child_bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(child_row.clone().into()))
                .unwrap()
                .bounds
        })
        .unwrap();
    assert!(child_bounds.origin.x >= bounds.origin.x + px(24.));
    hover_to(
        cx,
        window,
        point(bounds.origin.x + px(5.), bounds.origin.y + px(5.)),
    );
    cx.run_until_parked();
    click(cx, window, &format!("thread-disclose-{parent}"));
    assert!(!present(cx, window, &child_row));
    click(cx, window, &format!("thread-disclose-{parent}"));
    assert!(shows(cx, window, &child_row));
}
