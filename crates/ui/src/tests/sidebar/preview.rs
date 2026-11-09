use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, ThreadOp};

use crate::kagan::Kagan;
use crate::tests::kagan::{
    click, open, present, repo_thread, run_shell, shows, value_of, wait_until,
};

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
fn an_image_preview_shows_its_size_and_caption(cx: &mut TestAppContext) {
    const PIXEL: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let (kagan, window, data) = open(cx);
    let (thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(repo.join("pixel.png"), PIXEL).unwrap();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.show_preview(Some(thread.clone()), "pixel.png".into(), window, cx);
        });
    })
    .unwrap();
    wait_until(cx, |cx| {
        value_of(cx, window, "preview-image-caption").is_some()
    });
    assert_eq!(
        value_of(cx, window, "preview-image-caption").as_deref(),
        Some("1×1 · PNG 70 B")
    );
    assert_eq!(value_of(cx, window, "preview-size"), Some("70 B".into()));
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
                quiet: false,
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
fn a_preview_for_a_named_thread_opens_that_thread_and_its_tab(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(other.clone())
    });

    cx.update(|cx| crate::show_preview(Some(thread.clone()), "notes.txt".into(), cx));

    wait_until(cx, |cx| shows(cx, window, "right-tab-preview"));
    wait_until(cx, |cx| shows(cx, window, "preview-text-rows"));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(thread)
    );
    assert_eq!(value_of(cx, window, "preview-size"), Some("8 B".into()));
}

#[gpui_kit::test]
fn the_add_menu_does_not_offer_preview(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    click(cx, window, "right-tab-add");
    assert!(!present(cx, window, "right-add-preview"));
    assert!(present(cx, window, "right-add-terminal"));
}
