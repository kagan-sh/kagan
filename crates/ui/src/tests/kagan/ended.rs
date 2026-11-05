use std::time::Duration;

use gpui_kit::{AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::ThreadStatus;

use super::{
    click, open, row_top, run_program, run_shell, shows, thread_status, value_of, wait_for_mark,
    wait_until,
};

#[gpui_kit::test]
fn a_clean_exit_marks_the_row_ended_and_names_the_code(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let line = if cfg!(windows) {
        "cmd.exe /c exit 0"
    } else {
        "/bin/sh -c \"exit 0\""
    };
    let id = run_program(cx, line);
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Idle);
    let mark = format!("thread-mark-{}", id.0);
    wait_for_mark(cx, window, &mark, "done");
    click(cx, window, &format!("thread-{}", id.0));
    wait_for_mark(cx, window, &mark, "ended");
    assert_eq!(
        value_of(cx, window, &format!("thread-{}", id.0)).as_deref(),
        Some("ended")
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "ended").is_some_and(|value| value.contains("Exited with code 0."))
    });
    assert!(!shows(cx, window, "save-diagnostics-ended"));
    assert!(
        row_top(cx, window, "ended") > row_top(cx, window, "terminal"),
        "the ended line sits under the terminal"
    );
}

#[gpui_kit::test]
fn an_omission_shows_on_the_ended_line_when_the_screen_settles(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let line = if cfg!(windows) {
        "cmd.exe /c ping -n 3 127.0.0.1 >nul"
    } else {
        "/bin/sh -c \"sleep 2\""
    };
    let id = run_program(cx, line);
    wait_until(cx, |cx| {
        cx.update(|cx| {
            Workspace::global(cx).update(cx, |workspace, _| workspace.set_snapshot_bound(&id, 1))
        })
    });
    click(cx, window, &format!("thread-{}", id.0));
    let note = "The screen was outside the geometry or the 4 MiB bound.";
    wait_until(cx, |cx| {
        value_of(cx, window, "ended").is_some_and(|value| value.contains(note))
    });
}

#[gpui_kit::test]
fn a_failed_exit_keeps_the_failure_mark_and_names_the_code(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let line = if cfg!(windows) {
        "cmd.exe /c exit 1"
    } else {
        "/bin/sh -c \"exit 1\""
    };
    let id = run_program(cx, line);
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Error);
    click(cx, window, &format!("thread-{}", id.0));
    let mark = format!("thread-mark-{}", id.0);
    wait_for_mark(cx, window, &mark, "failed");
    assert_eq!(
        value_of(cx, window, &format!("thread-{}", id.0)).as_deref(),
        Some("failed")
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "ended").is_some_and(|value| value.contains("Exited with code 1."))
    });
    assert!(shows(cx, window, "save-diagnostics-ended"));
}

#[gpui_kit::test]
fn save_diagnostics_is_in_the_thread_menu(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let path = data
        .path()
        .join(if cfg!(windows) { "quiet.cmd" } else { "quiet" });
    let body = if cfg!(windows) {
        "@echo MENU-MARKER\r\n"
    } else {
        "#!/bin/sh\nprintf 'MENU-MARKER\\n'\n"
    };
    std::fs::write(&path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let id = run_program(cx, &path.display().to_string());
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Idle);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "thread-actions-trigger");
    assert!(shows(cx, window, "save-diagnostics"));
    click(cx, window, "save-diagnostics");
    let dir = data.path().join("diagnostics");
    wait_until(cx, |_cx| {
        std::fs::read_dir(&dir).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        })
    });
    let text = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .unwrap_or_default();
    assert!(text.contains(&id.0), "{text}");
    assert!(!text.contains("MENU-MARKER"), "{text}");
}

#[gpui_kit::test]
fn a_saved_diagnostics_notice_keeps_dismiss_on_screen(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let id = run_shell(cx);
    let row = format!("thread-{}", id.0);
    wait_until(cx, |cx| shows(cx, window, &row));
    click(cx, window, &row);
    let title_y = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("thread-title".into()))
                .map(|element| element.bounds.origin.y)
        })
        .unwrap()
        .expect("thread title")
    };
    let before = title_y(cx);
    click(cx, window, "thread-actions-trigger");
    click(cx, window, "save-diagnostics");
    wait_until(cx, |cx| shows(cx, window, "toast-dismiss"));
    assert_eq!(title_y(cx), before, "notice moved the thread");
    let width = cx
        .update_window(window, |_, window, _| {
            f32::from(window.viewport_size().width)
        })
        .unwrap();
    let (dismiss_x, dismiss_w) =
        crate::tests::sidebar::layout::span(cx, window, "toast-dismiss").expect("dismiss");
    assert!(
        dismiss_x >= 0. && dismiss_x + dismiss_w <= width + 0.5,
        "dismiss {dismiss_x}+{dismiss_w} window {width}"
    );
    let message = value_of(cx, window, "toast-message").unwrap_or_default();
    // Windows can hand out the temp folder by its short 8.3 name; kagan shows the full one.
    let saved = dunce::canonicalize(data.path())
        .unwrap()
        .join("diagnostics");
    assert!(
        message.starts_with("Saved diagnostics to ")
            && message.contains(saved.to_str().unwrap_or("")),
        "{message}"
    );
    let (message_x, message_w) =
        crate::tests::sidebar::layout::span(cx, window, "toast-message").expect("message");
    assert!(
        message_x + message_w <= dismiss_x + 0.5,
        "message {message_x}+{message_w} dismiss {dismiss_x}"
    );
    click(cx, window, "toast-dismiss");
    assert!(!shows(cx, window, "toast"));

    click(cx, window, "thread-actions-trigger");
    click(cx, window, "save-diagnostics");
    wait_until(cx, |cx| shows(cx, window, "toast"));
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    assert!(!shows(cx, window, "toast"));
}
