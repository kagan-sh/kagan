use super::chord;
use crate::kagan::screen_read;

#[cfg(unix)]
use gpui_kit::Modifiers;
use gpui_kit::{AppContext as _, ClipboardItem, TestAppContext};
#[cfg(unix)]
use gpui_kit::{Entity, px, size};
use kg_core::Workspace;
use kg_domain::{KeyPlatform, Keymap, KeymapOp, Op, ProjectId};
#[cfg(unix)]
use kg_domain::{ThreadId, ThreadOp, ThreadStatus};

#[cfg(unix)]
use crate::kagan::Kagan;
use crate::kagan::{ArchiveThread, StopThread, chord_for};

#[cfg(unix)]
use super::{
    cell_point, host_dimensions, press, release, run_program, selection, shown_row, thread_status,
};
use super::{click, grid_text, open, run_shell, shows, thread_record, wait_until};

#[test]
fn off_macos_the_default_chord_stays_off_the_program_keys() {
    let keymap = Keymap::default();
    assert_eq!(
        chord_for("NewThread", &keymap, KeyPlatform::Mac),
        "secondary-n"
    );
    assert_eq!(
        chord_for("NewThread", &keymap, KeyPlatform::Linux),
        "ctrl-shift-n"
    );
    assert_eq!(
        chord_for("StopThread", &keymap, KeyPlatform::Windows),
        "ctrl-shift-."
    );
    assert_eq!(
        chord_for("ArchiveThread", &keymap, KeyPlatform::Linux),
        "ctrl-shift-backspace"
    );
    let mut saved = Keymap::default();
    saved.set("NewThread".into(), "ctrl-j".into());
    assert_eq!(chord_for("NewThread", &saved, KeyPlatform::Linux), "ctrl-j");
}

#[gpui_kit::test]
fn a_saved_shortcut_replaces_the_default(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Keymap(KeymapOp::Set {
                action: "NewThread".into(),
                key: "secondary-j".into(),
                reset: false,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();

    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );
    cx.simulate_keystrokes(window, "secondary-j");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
}

#[gpui_kit::test]
fn a_shortcut_works_after_stop(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| !shows(cx, window, "stop"));

    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
}

#[gpui_kit::test]
fn a_shortcut_works_after_the_thread_is_archived(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());

    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn ctrl_keys_reach_the_program_and_are_not_window_shortcuts(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_program(cx, "cat");
    click(cx, window, &format!("thread-{}", id.0));

    cx.simulate_keystrokes(window, "ctrl-n");
    // The shell echoes a control key as ^N; it can only show up if the key reached the program.
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("^N"));
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );

    cx.simulate_keystrokes(window, "ctrl-c");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("^C"));
}

#[gpui_kit::test]
fn tab_stays_with_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(
        cx.update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap()
    );

    cx.simulate_keystrokes(window, "tab");
    cx.run_until_parked();
    assert!(
        cx.update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap()
    );
}

#[gpui_kit::test]
fn the_paste_key_sends_the_clipboard(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.write_to_clipboard(ClipboardItem::new_string("keyed-paste".into()));

    let paste = if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    };
    cx.simulate_keystrokes(window, paste);
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("keyed-paste"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_start_again_shortcut_runs_a_stopped_program_again(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let flag = data.path().join("start-again").display().to_string();
    let id = finished_program(
        cx,
        &format!("/bin/sh -c \"if [ -f {flag} ]; then sleep 30; else touch {flag}; fi\""),
    );
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, &chord("StartAgain"));
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Active);
    let _ = std::fs::remove_file(&flag);
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_start_again_button_runs_a_stopped_program_again(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let flag = data.path().join("start-again-button").display().to_string();
    let id = finished_program(
        cx,
        &format!("/bin/sh -c \"if [ -f {flag} ]; then sleep 30; else touch {flag}; fi\""),
    );
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "start-again");
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Active);
    let _ = std::fs::remove_file(&flag);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_started_again_program_takes_the_size_the_open_window_fitted(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let flag = data.path().join("start-again-size").display().to_string();
    let id = run_program(
        cx,
        &format!("/bin/sh -c \"if [ -f {flag} ]; then sleep 30; else touch {flag}; exit 7; fi\""),
    );
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Error);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let before = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));

    // The window is resized while the ended screen is shown; the view fits it to the new size.
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    wait_until(cx, |cx| {
        let fitted = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));
        fitted.is_some() && fitted != before
    });
    cx.run_until_parked();
    let fitted = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));

    let started = cx
        .update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::StartAgain { id: id.clone() }), cx));
    cx.foreground_executor().block_test(started).unwrap();
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Active);
    let _ = std::fs::remove_file(&flag);

    assert_ne!(fitted, Some((80, 24)), "the window did not fit a new size");
    assert_eq!(
        fitted,
        Some(host_dimensions(cx, &id)),
        "the restarted program did not take the size the window fitted"
    );
}

#[cfg(unix)]
pub(in crate::tests) fn finished_program(cx: &mut TestAppContext, program: &str) -> ThreadId {
    let id = run_program(cx, program);
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Idle);
    id
}

#[cfg(unix)]
pub(in crate::tests) fn notice_of(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
) -> Option<String> {
    cx.update(|cx| {
        let view = screen_read::terminal_view(kagan.read(cx))?;
        view.read(cx).notice().map(str::to_owned)
    })
}

#[cfg(unix)]
#[gpui_kit::test]
fn typing_into_an_ended_program_says_it_was_not_sent(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = finished_program(cx, "/bin/sh -c \"printf done; exit 0\"");
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, "x");
    wait_until(cx, |cx| {
        notice_of(cx, &kagan).is_some_and(|notice| notice.contains("ended"))
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn pasting_into_an_ended_program_says_it_was_not_sent(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = finished_program(cx, "/bin/sh -c \"printf done; exit 0\"");
    click(cx, window, &format!("thread-{}", id.0));
    cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
    cx.dispatch_action(window, crate::terminal::Paste);
    wait_until(cx, |cx| {
        notice_of(cx, &kagan).is_some_and(|notice| notice.contains("ended"))
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn paging_back_through_an_ended_program_redraws_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = finished_program(
        cx,
        "/bin/sh -c \"i=1; while [ $i -le 200 ]; do echo row$i; i=$((i+1)); done\"",
    );
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("row200"));
    cx.dispatch_action(window, crate::terminal::ScrollPageUp);
    wait_until(cx, |cx| !grid_text(cx, &kagan).contains("row200"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn selecting_text_after_a_program_that_asked_for_the_mouse_exits(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let (kagan, window, _data) = open(cx);
    let script = std::env::temp_dir().join(format!("kg-mouse-{}.sh", std::process::id()));
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '\\033[?1000h'\nprintf picked\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let id = finished_program(cx, script.to_str().unwrap());
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("picked"));
    let (row, _) = shown_row(cx, &kagan, "picked");
    let at = cell_point(cx, window, &kagan, row, 2);
    press(cx, window, at, 1, Modifiers::default());
    release(cx, window, at, 1, Modifiers::default());
    press(cx, window, at, 2, Modifiers::default());
    release(cx, window, at, 2, Modifiers::default());
    wait_until(cx, |cx| !selection(cx, &kagan).is_empty());
    let _ = std::fs::remove_file(&script);
}

#[cfg(unix)]
#[gpui_kit::test]
fn selecting_text_in_an_ended_program_redraws_its_selection(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = finished_program(cx, "/bin/sh -c \"printf picked; exit 0\"");
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("picked"));
    let (row, _) = shown_row(cx, &kagan, "picked");
    let at = cell_point(cx, window, &kagan, row, 2);
    press(cx, window, at, 1, Modifiers::default());
    release(cx, window, at, 1, Modifiers::default());
    press(cx, window, at, 2, Modifiers::default());
    release(cx, window, at, 2, Modifiers::default());
    wait_until(cx, |cx| !selection(cx, &kagan).is_empty());
}
