use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, ThreadOp};

use crate::kagan::Kagan;
use crate::tests::kagan::{click, open, present, repo_thread, shows, value_of, wait_until};

fn with_sh(cx: &mut TestAppContext) {
    let path = std::env::var_os("PATH").unwrap_or_default();
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(
                std::path::Path::new(&path),
                std::path::Path::new("/bin/sh"),
            );
        });
    });
}

fn review_open(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let (kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    (kagan, window, data)
}

#[gpui_kit::test]
fn the_review_previews_the_file_on_screen(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = review_open(cx);
    click(cx, window, "review-preview");
    wait_until(cx, |cx| shows(cx, window, "right-tab-preview"));
    wait_until(cx, |cx| shows(cx, window, "preview-text-rows"));
    assert!(shows(cx, window, "right-overlay-panel"));
    assert!(present(cx, window, "preview-copy"));
    assert_eq!(value_of(cx, window, "preview-size"), Some("8 B".into()));
}

#[gpui_kit::test]
fn copy_path_puts_the_previewed_path_on_the_clipboard(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = review_open(cx);
    click(cx, window, "review-preview");
    wait_until(cx, |cx| present(cx, window, "preview-copy"));
    click(cx, window, "preview-copy");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("notes.txt"));
}

#[gpui_kit::test]
fn a_path_outside_the_thread_keeps_the_tab_and_says_why(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    std::fs::write(data.path().join("outside.txt"), "no\n").unwrap();
    let path = data.path().join("outside.txt").display().to_string();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.show_preview(Some(thread.clone()), path, window, cx);
        });
    })
    .unwrap();
    wait_until(cx, |cx| shows(cx, window, "right-tab-preview"));
    wait_until(cx, |cx| {
        value_of(cx, window, "preview-notice")
            .is_some_and(|notice| notice.contains("outside this thread's folder"))
    });
}

#[gpui_kit::test]
fn a_thread_switch_closes_the_preview(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = review_open(cx);
    with_sh(cx);
    click(cx, window, "review-preview");
    wait_until(cx, |cx| shows(cx, window, "right-tab-preview"));

    let started = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Run(kg_domain::Run {
                project_id: kg_domain::ProjectId("prj_kagan".into()),
                program: "/bin/sh".into(),
                title: None,
                environment: kg_domain::EnvironmentSpec::Personal,
                visibility: None,
                parent_thread_id: None,
                lifecycle_owner_thread_id: None,
                metadata: None,
            })),
            cx,
        )
    });
    let kg_domain::OpResult::Thread(thread) = cx.foreground_executor().block_test(started).unwrap()
    else {
        panic!("thread")
    };
    cx.run_until_parked();
    click(cx, window, &format!("thread-{}", thread.id.0));
    wait_until(cx, |cx| !present(cx, window, "right-tab-preview"));
}

#[gpui_kit::test]
fn the_add_menu_does_not_offer_preview(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    click(cx, window, "right-tab-add");
    assert!(!present(cx, window, "right-add-preview"));
    assert!(present(cx, window, "right-add-terminal"));
}
