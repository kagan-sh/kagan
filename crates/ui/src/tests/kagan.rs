use crate::kagan::review_read;
use crate::kagan::screen_read;
use std::cell::Cell;
use std::fmt::Write as _;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::component::{ActiveTheme as _, ThemeMode, WindowExt as _};
use gpui_kit::{
    Action, AnyWindowHandle, AppContext as _, ClipboardItem, ElementId, Entity, NoAction,
    TestAppContext, WindowBounds, px, size,
};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, point,
};
use kg_core::{Color, Grid, Workspace, WorkspaceEvent};
use kg_domain::{
    EnvironmentOp, EnvironmentProvider, EnvironmentSpec, EnvironmentStatus, InteractionOption,
    KeyPlatform, Keymap, KeymapOp, Op, OpError, OpResult, Plugin, PluginId, PluginOp, PreferenceOp,
    Project, ProjectId, ProjectKind, Run, ThreadId, ThreadOp, ThreadStatus, UI_PERSON_ONLY, UiOp,
    Visibility,
};

use kg_plugins::{HostUi, PluginHost};

use crate::kagan::changes::POLL;
use crate::kagan::{
    ActualSize, AddProject, ArchiveThread, Bigger, FocusSidebar, Kagan, NewThread, OpenSettings,
    QUIET, RestoreThread, RetryBranches, RetryPrograms, Smaller, StartAgain, StopThread,
    ThreadActions, ToggleCustomCommand, TogglePinned, ToggleRightSidebar, ToggleSidebar,
    UseProjectFolder, UseWorktreeFolder, chord_for, is_chord,
};

use crate::terminal::TerminalView;
use crate::theme::follow_system;

/// The chord the window answers to for an action on this machine, as the keyboard sends it.
pub(super) fn chord(action: &str) -> String {
    chord_for(action, &Keymap::default(), KeyPlatform::host())
}

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
fn finished_program(cx: &mut TestAppContext, program: &str) -> ThreadId {
    let id = run_program(cx, program);
    wait_until(cx, |cx| thread_status(cx, &id) == ThreadStatus::Idle);
    id
}

#[cfg(unix)]
fn notice_of(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Option<String> {
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

#[gpui_kit::test]
fn the_restore_shortcut_brings_an_archived_thread_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    click(cx, window, &format!("thread-{}", id.0));

    cx.simulate_keystrokes(window, &chord("RestoreThread"));
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_none());
}

#[gpui_kit::test]
fn the_sidebar_selects_a_project_and_opens_a_thread_from_the_keyboard(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    cx.run_until_parked();

    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| {
        let records = cx.update(|cx| Workspace::global(cx).read(cx).thread_records(cx));
        records
            .iter()
            .any(|t| t.project_id.0 == "prj_kagan" && t.id != id)
    });
    assert_eq!(selected_of(cx, window, "project-prj_kagan"), Some(false));

    super::sidebars::focus_row(cx, window, &format!("thread-{}", id.0));
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn focus_sidebar_lands_on_the_open_thread_and_opens_nothing(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let view = cx.update(|cx| screen_read::terminal_view(kagan.read(cx)));

    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    assert_eq!(
        focused_of(cx, window, &format!("thread-{}", id.0)),
        Some(true)
    );
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );
    assert!(!shows(cx, window, "start-prompt"));
    assert!(shows(cx, window, "terminal"));
}

#[gpui_kit::test]
fn focus_sidebar_from_hidden_sidebar_selects_the_thread_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    wait_until(cx, |cx| {
        focused_of(cx, window, &format!("thread-{}", id.0)) == Some(true)
    });
}

#[gpui_kit::test]
fn space_on_the_project_selects_it_and_opens_nothing(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    run_shell(cx);
    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );
    cx.simulate_keystrokes(window, "space");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(kg_domain::ProjectId("prj_kagan".into()))
    );
    assert_eq!(selected_of(cx, window, "project-prj_kagan"), Some(true));
    assert!(shows(cx, window, "start-prompt"));
}

#[gpui_kit::test]
fn a_second_action_does_not_cancel_the_first_follow_up(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.dispatch_action(window, TogglePinned);
    cx.dispatch_action(window, ToggleSidebar);
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
}

#[gpui_kit::test]
fn a_program_that_ends_keeps_its_last_output_on_screen(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&id, &line(&split_command("ke", "yed"))), cx).detach();
    });
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("keyed"));

    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| !shows(cx, window, "stop"));
    assert!(
        shows(cx, window, "terminal"),
        "the terminal left the window"
    );
    assert!(shows(cx, window, "ended"));
    assert!(grid_text(cx, &kagan).contains("keyed"));
}

#[test]
fn a_saved_chord_that_does_not_parse_falls_back_to_the_default() {
    let mut keymap = Keymap::default();
    keymap.set("NewThread".into(), "ctrl-n-x".into());
    assert_eq!(
        chord_for("NewThread", &keymap, KeyPlatform::Linux),
        "ctrl-shift-n"
    );
}

#[test]
fn the_window_rebinds_its_chords() {
    let chords: [&dyn Action; 20] = [
        &Bigger,
        &Smaller,
        &ActualSize,
        &NewThread,
        &StopThread,
        &ArchiveThread,
        &TogglePinned,
        &AddProject,
        &StartAgain,
        &FocusSidebar,
        &RestoreThread,
        &ToggleSidebar,
        &ToggleRightSidebar,
        &OpenSettings,
        &UseProjectFolder,
        &UseWorktreeFolder,
        &RetryBranches,
        &RetryPrograms,
        &ToggleCustomCommand,
        &ThreadActions,
    ];
    for chord in chords {
        assert!(is_chord(chord));
    }
    assert!(!is_chord(&NoAction));
}

#[gpui_kit::test]
fn a_running_program_has_no_not_running_note(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(!shows(cx, window, "ended"));
}

#[gpui_kit::test]
fn choosing_a_thread_shows_its_terminal(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );

    cx.update(|cx| {
        Workspace::dispatch(input(&id, &line(&split_command("shown", "-out"))), cx).detach();
    });
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("shown-out"));
}

#[gpui_kit::test]
fn a_key_reaches_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    // The echo shows the split command, so "keyed" can only appear once Enter runs it.
    cx.simulate_input(window, &split_command("ke", "yed"));
    wait_until(cx, |cx| {
        grid_text(cx, &kagan).contains(&split_command("ke", "yed"))
    });
    assert!(!grid_text(cx, &kagan).contains("keyed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("keyed"));
}

#[gpui_kit::test]
fn paste_sends_the_clipboard_to_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.write_to_clipboard(ClipboardItem::new_string("pasted-words".into()));
    cx.dispatch_action(window, crate::terminal::Paste);
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("pasted-words"));
}

#[gpui_kit::test]
fn shift_page_up_scrolls_back_and_the_program_gets_no_keys(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let rows = if cfg!(windows) {
        "for /l %i in (1,1,200) do @echo row%i"
    } else {
        "for i in $(seq 1 200); do echo row$i; done"
    };
    cx.simulate_input(window, &line(rows));
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("row200"));

    cx.simulate_keystrokes(window, "shift-pageup");
    wait_until(cx, |cx| !grid_text(cx, &kagan).contains("row200"));
    assert!(!grid_text(cx, &kagan).contains("5;2~"));
}

#[gpui_kit::test]
fn the_copy_key_copies_the_selected_word(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("picked"));

    let grid = cx
        .update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
        .unwrap();
    let (row, line) = grid
        .lines
        .iter()
        .enumerate()
        .find(|(_, line)| {
            line.iter()
                .map(|cell| cell.c)
                .collect::<String>()
                .contains("picked")
        })
        .unwrap();
    let text: String = line.iter().map(|cell| cell.c).collect();
    let column = text.find("picked").unwrap() + 1;
    let select = ThreadOp::Select {
        id: id.clone(),
        mode: kg_domain::SelectMode::Word,
        column: u16::try_from(column).unwrap(),
        row: u16::try_from(row).unwrap(),
        right: false,
        terminal: None,
    };
    cx.update(|cx| Workspace::dispatch(Op::Thread(select), cx))
        .detach();
    cx.run_until_parked();

    let copy = if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-shift-c"
    };
    cx.simulate_keystrokes(window, copy);
    wait_until(cx, |cx| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .is_some_and(|text| text == "picked")
    });
}

#[gpui_kit::test]
fn a_double_click_selects_a_word_that_copies(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("picked"));

    let (row, _) = shown_row(cx, &kagan, "picked");
    let at = cell_point(cx, window, &kagan, row, 2);
    press(cx, window, at, 1, Modifiers::default());
    release(cx, window, at, 1, Modifiers::default());
    press(cx, window, at, 2, Modifiers::default());
    release(cx, window, at, 2, Modifiers::default());

    let copy = if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-shift-c"
    };
    cx.simulate_keystrokes(window, copy);
    wait_until(cx, |cx| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .is_some_and(|text| text == "picked")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_program_that_asks_for_mouse_gets_the_click_and_shift_selects_instead(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, "printf '\\033[?1000h\\033[?1006h'");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
            .is_some_and(|grid| grid.mouse.is_some())
    });

    // The program asked for the mouse, so a plain click goes to it and selects nothing.
    let plain = cell_point(cx, window, &kagan, 6, 10);
    press(cx, window, plain, 1, Modifiers::default());
    release(cx, window, plain, 1, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(selection(cx, &kagan).is_empty());

    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let shifted = cell_point(cx, window, &kagan, 6, 20);
    press(cx, window, shifted, 1, shift);
    let dragged = cell_point(cx, window, &kagan, 6, 24);
    drag_to(cx, window, dragged);
    release(cx, window, dragged, 1, shift);
    wait_until(cx, |cx| !selection(cx, &kagan).is_empty());
}

#[gpui_kit::test]
fn a_press_in_the_left_half_and_a_drag_into_the_right_half_selects_the_character(
    cx: &mut TestAppContext,
) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("picked"));

    // The character at column 2 of "picked" is "c".
    let (row, _) = shown_row(cx, &kagan, "picked");
    let left = cell_point_at(cx, window, &kagan, row, 2, 0.2);
    press(cx, window, left, 1, Modifiers::default());
    let right = cell_point_at(cx, window, &kagan, row, 2, 0.8);
    drag_to(cx, window, right);
    release(cx, window, right, 1, Modifiers::default());
    wait_until(cx, |cx| !selection(cx, &kagan).is_empty());
    assert_eq!(selection(cx, &kagan), vec![(row, 2..3)]);
}

#[gpui_kit::test]
fn the_release_is_where_a_selection_ends(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("picked"));

    let (row, _) = shown_row(cx, &kagan, "picked");
    let start = cell_point_at(cx, window, &kagan, row, 0, 0.3);
    press(cx, window, start, 1, Modifiers::default());
    let middle = cell_point(cx, window, &kagan, row, 2);
    drag_to(cx, window, middle);
    // The last move is to column 2; the release is in the right half of column 4, so the
    // selection ends with that character.
    let end = cell_point_at(cx, window, &kagan, row, 4, 0.8);
    release(cx, window, end, 1, Modifiers::default());
    wait_until(cx, |cx| selection(cx, &kagan) == vec![(row, 0..5)]);
}

#[gpui_kit::test]
fn a_release_outside_the_terminal_ends_the_selection(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("one two", " three"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("one two three"));

    let (row, _) = shown_row(cx, &kagan, "one two three");
    let start = cell_point(cx, window, &kagan, row, 0);
    press(cx, window, start, 1, Modifiers::default());
    let end = cell_point(cx, window, &kagan, row, 4);
    drag_to(cx, window, end);
    wait_until(cx, |cx| !selection(cx, &kagan).is_empty());
    let before = selection(cx, &kagan);

    // The release is lost, so the pointer comes back with no button down.
    hover_to(cx, window, point(px(2.), px(2.)));
    let back = cell_point(cx, window, &kagan, row, 10);
    hover_to(cx, window, back);
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(selection(cx, &kagan), before);
}

fn selection(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
) -> Vec<(usize, std::ops::Range<usize>)> {
    cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
        .map_or_else(Vec::new, |grid| grid.selection)
}

/// The row and column of the first line that is exactly `text`, on the shown terminal.
pub(super) fn shown_row(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
    text: &str,
) -> (usize, usize) {
    let grid = cx
        .update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
        .expect("a grid");
    grid.lines
        .iter()
        .enumerate()
        .find_map(|(row, line)| {
            let line: String = line.iter().map(|cell| cell.c).collect();
            line.trim_end().find(text).map(|column| (row, column))
        })
        .expect("the line on screen")
}

/// The pixel at the middle of a cell of the shown terminal, from its bounds and its grid's size.
pub(super) fn cell_point(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
    row: usize,
    column: usize,
) -> Point<Pixels> {
    cell_point_at(cx, window, kagan, row, column, 0.5)
}

/// A pixel across a cell, `across` of the way from its left edge to its right edge.
fn cell_point_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
    row: usize,
    column: usize,
    across: f32,
) -> Point<Pixels> {
    let grid = cx
        .update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
        .expect("a grid");
    let rows = f32::from(u16::try_from(grid.lines.len()).unwrap());
    let cols = f32::from(u16::try_from(grid.lines.first().map_or(1, Vec::len)).unwrap());
    let row = f32::from(u16::try_from(row).unwrap());
    let column = f32::from(u16::try_from(column).unwrap());
    let bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("terminal-grid".into()))
                .expect("the terminal grid")
                .bounds()
        })
        .unwrap();
    point(
        bounds.origin.x + bounds.size.width * ((column + across) / cols),
        bounds.origin.y + bounds.size.height * ((row + 0.5) / rows),
    )
}

fn send(cx: &mut TestAppContext, window: AnyWindowHandle, event: gpui_kit::PlatformInput) {
    cx.update_window(window, |_, window, cx| window.dispatch_event(event, cx))
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn press(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    at: Point<Pixels>,
    clicks: usize,
    modifiers: Modifiers,
) {
    let event = MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: clicks,
        first_mouse: false,
    };
    send(cx, window, event.to_platform_input());
}

pub(super) fn release(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    at: Point<Pixels>,
    clicks: usize,
    modifiers: Modifiers,
) {
    let event = MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: clicks,
    };
    send(cx, window, event.to_platform_input());
}

pub(super) fn drag_to(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    let event = MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    };
    send(cx, window, event.to_platform_input());
}

pub(super) fn hover_to(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    let event = MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::default(),
    };
    send(cx, window, event.to_platform_input());
}

/// The program every test thread runs: a shell on each platform.
fn shell() -> &'static str {
    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

/// One line as typed into that shell.
fn line(command: &str) -> String {
    if cfg!(windows) {
        format!("{command}\r\n")
    } else {
        format!("{command}\n")
    }
}

/// An echo command as typed. Its output is `left` + `right`, but the typed text is not, so
/// output can be told from the echo of the command.
fn split_command(left: &str, right: &str) -> String {
    if cfg!(windows) {
        format!("echo {left}^{right}")
    } else {
        format!("echo {left}''{right}")
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn cells_paint_their_background_where_they_are(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    // A blue cell on row 2, a two-column red span on row 3, and a green cell four columns right.
    let script = "printf '\\033[2;2H\\033[44m \\033[0m\\033[3;2H\\033[41m  \\033[0m\\033[3;6H\\033[42m \\033[0m'\n";
    cx.update(|cx| Workspace::dispatch(input(&id, script), cx).detach());
    let red = cx.update(|cx| crate::theme::terminal_color(Color::Ansi(kg_core::Ansi::Red), cx));
    let green = cx.update(|cx| crate::theme::terminal_color(Color::Ansi(kg_core::Ansi::Green), cx));
    let blue = cx.update(|cx| crate::theme::terminal_color(Color::Ansi(kg_core::Ansi::Blue), cx));
    wait_until(cx, |cx| {
        let quads = painted(cx, window);
        [red, green, blue]
            .iter()
            .all(|color| quads.iter().any(|quad| quad.background == (*color).into()))
    });
    let quads = painted(cx, window);
    let place = |color: gpui_kit::Hsla| {
        let quad = quads
            .iter()
            .find(|quad| quad.background == color.into())
            .expect("a painted cell");
        (
            quad.bounds.origin.x.as_f32(),
            quad.bounds.origin.y.as_f32(),
            quad.bounds.size.width.as_f32(),
        )
    };
    let (red_x, red_y, span) = place(red);
    let (green_x, green_y, cell) = place(green);
    let (_, blue_y, _) = place(blue);
    // A two-column span is twice as wide as one cell.
    assert!(span > 1.5 * cell, "span {span} cell {cell}");
    // Quads snap to whole pixels, so each of the four columns may be a pixel off.
    assert!(
        (green_x - red_x - 4. * cell).abs() <= 4.,
        "red {red_x} green {green_x} cell {cell}"
    );
    assert!((green_y - red_y).abs() < 0.01);
    // Row three is below row two.
    assert!(red_y > blue_y, "red {red_y} blue {blue_y}");
}

pub(super) fn present(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        !matches!(
            crate::ui_registry::locate(window, &ElementId::Name(name.to_owned().into()), None),
            crate::ui_registry::Located::Missing
        )
    })
    .unwrap()
}

pub(super) fn shows(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .is_some_and(|element| element.visible())
    })
    .unwrap()
}

/// The value `kg ui` reports for an element, if it has one.
pub(super) fn value_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.value().map(str::to_owned))
    })
    .unwrap()
}

/// Whether `kg ui` reports an element as focused, if it reports focus at all.
pub(super) fn focused_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.focused())
    })
    .unwrap()
}

/// The role `kg ui` reports for an element, if it has one.
pub(super) fn role_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<gpui_kit::Role> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.role())
    })
    .unwrap()
}

/// Whether `kg ui` reports an element as selected, if it reports selection at all.
pub(super) fn selected_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.selected())
    })
    .unwrap()
}

pub(super) fn label(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> String {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.label().map(str::to_owned))
            .unwrap_or_default()
    })
    .unwrap()
}

fn row_top(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .expect("a row")
            .bounds()
            .origin
            .y
            .as_f32()
    })
    .unwrap()
}

/// Whether a quad of `color` paints at the top edge of the named row.
fn quad_at_row(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    color: gpui_kit::Hsla,
) -> bool {
    let (top, scale) = cx
        .update_window(window, |_, window, _| {
            let row = crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
                .expect("a row");
            (row.bounds().origin.y.as_f32(), window.scale_factor())
        })
        .unwrap();
    painted(cx, window)
        .iter()
        .filter(|quad| quad.background == color.into())
        .any(|quad| (quad.bounds.origin.y.as_f32() - top * scale).abs() < scale)
}

fn highlighted_at(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> bool {
    let accent = cx.update(|cx| cx.theme().sidebar_accent);
    quad_at_row(cx, window, name, accent)
}

/// Moves the pointer over the named element, the way a person's pointer would.
fn point_at(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.update_window(window, |_, window, cx| {
        let target = crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .expect("an element to point at")
            .bounds()
            .center();
        window.dispatch_event(
            MouseMoveEvent {
                position: target,
                pressed_button: None,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
}

fn painted(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<gpui_kit::Quad> {
    cx.update_window(window, |_, window, _| window.painted_quads())
        .unwrap()
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_light_fjord_terminal_paints_its_dark_background(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        Workspace::dispatch(
            kg_domain::Op::Theme(kg_domain::ThemeOp::Set {
                theme: "fjord".into(),
                mode: kg_domain::Appearance::Light,
            }),
            cx,
        )
        .detach();
    });
    let terminal: gpui_kit::Hsla = gpui_kit::rgb(0x1a_1d24).into();
    wait_until(cx, |cx| {
        painted(cx, window)
            .iter()
            .any(|quad| quad.background == terminal.into())
    });
}

#[gpui_kit::test]
fn a_screen_for_another_thread_does_not_repaint_the_shown_one(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let shown = run_shell(cx);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", shown.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let view = cx
        .update(|cx| screen_read::terminal_view(kagan.read(cx)))
        .expect("a thread is shown");
    let (repaints, _subscription) = count_repaints(cx, &view);
    let reads = cx.update(|cx| view.read(cx).reads());

    cx.update(|cx| {
        Workspace::global(cx).update(cx, |_, cx| {
            cx.emit(WorkspaceEvent::Screen(other.clone(), None));
        });
    });
    cx.run_until_parked();

    assert_eq!(
        cx.update(|cx| view.read(cx).reads()),
        reads,
        "the shown view read a grid it does not show"
    );
    assert_eq!(
        repaints.get(),
        0,
        "the shown view repainted for another thread's screen"
    );
}

#[gpui_kit::test]
fn a_screen_for_the_shown_thread_reads_and_repaints_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let view = cx
        .update(|cx| screen_read::terminal_view(kagan.read(cx)))
        .expect("a thread is shown");
    let (repaints, _subscription) = count_repaints(cx, &view);
    let reads = cx.update(|cx| view.read(cx).reads());

    // Typed text changes the host's screen, which sends the shown thread a screen event.
    cx.simulate_input(window, &split_command("ke", "yed"));
    wait_until(cx, |cx| {
        grid_text(cx, &kagan).contains(&split_command("ke", "yed"))
    });
    assert!(
        cx.update(|cx| view.read(cx).reads()) > reads,
        "the shown view never read its screen"
    );
    assert!(repaints.get() > 0, "the shown view never repainted");
}

#[gpui_kit::test]
fn a_resize_reaches_the_host_at_the_size_the_view_fitted(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let before = host_dimensions(cx, &id);

    // The user's resize: the platform reports the new size, and the window lays out again.
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    // The host may pass through an intermediate size first, so wait for the fitted size itself.
    wait_until(cx, |cx| {
        let fitted = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));
        let now = host_dimensions(cx, &id);
        now != before && fitted == Some(now)
    });
    let fitted = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));
    assert_eq!(Some(host_dimensions(cx, &id)), fitted);
    let after = host_dimensions(cx, &id);
    assert!(
        after.0 > before.0 && after.1 > before.1,
        "{before:?} -> {after:?}"
    );
}

#[gpui_kit::test]
fn terminal_text_size_keys_save_and_refit_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let before = host_dimensions(cx, &id);
    cx.simulate_keystrokes(window, &chord("Bigger"));
    wait_until(cx, |cx| host_dimensions(cx, &id) != before);
    let larger = host_dimensions(cx, &id);
    assert!(
        larger.0 < before.0 && larger.1 < before.1,
        "{before:?} -> {larger:?}"
    );
    let saved = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: "terminal-font-size".into(),
            }),
            cx,
        )
    });
    assert_eq!(
        cx.foreground_executor().block_test(saved).unwrap(),
        OpResult::Setting(serde_json::json!(13))
    );
    cx.simulate_keystrokes(window, &chord("Smaller"));
    wait_until(cx, |cx| host_dimensions(cx, &id) == before);
    if cfg!(target_os = "macos") {
        cx.simulate_keystrokes(window, "cmd-+");
        wait_until(cx, |cx| host_dimensions(cx, &id) == larger);
    }
    set_preference(cx, "terminal-font-size", serde_json::json!(20));
    wait_until(cx, |cx| host_dimensions(cx, &id).0 < larger.0);
    cx.simulate_keystrokes(window, &chord("ActualSize"));
    wait_until(cx, |cx| host_dimensions(cx, &id) == before);
}

fn host_dimensions(cx: &mut TestAppContext, id: &ThreadId) -> (u16, u16) {
    let grid = host_grid(cx, id);
    (
        u16::try_from(grid.lines[0].len()).unwrap(),
        u16::try_from(grid.lines.len()).unwrap(),
    )
}

#[gpui_kit::test]
fn switching_threads_shows_each_threads_own_screen(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&first, &line(&split_command("fir", "st-out"))), cx).detach();
    });
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("first-out"));

    click(cx, window, &format!("thread-{}", second.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&second, &line(&split_command("bra", "vo-out"))), cx).detach();
    });
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("bravo-out"));
    assert!(!grid_text(cx, &kagan).contains("first-out"));

    click(cx, window, &format!("thread-{}", first.0));
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("first-out"));
    assert!(!grid_text(cx, &kagan).contains("bravo-out"));
}

#[gpui_kit::test]
fn a_thread_keeps_its_screen_when_the_window_closes_and_reopens(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&id, &line(&split_command("kee", "p-out"))), cx).detach();
    });
    wait_until(cx, |cx| {
        host_grid(cx, &id).lines.iter().any(|row| {
            row.iter()
                .map(|cell| cell.c)
                .collect::<String>()
                .contains("keep-out")
        })
    });

    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let reopened = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| grid_text(cx, &reopened).contains("keep-out"));
}

#[gpui_kit::test]
fn stop_ends_the_program(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(&id))
            .is_some()
    );

    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| {
        cx.update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .terminal_process_id(&id)
                .is_none()
        })
    });
    assert_eq!(thread_status(cx, &id), ThreadStatus::Idle);
    wait_until(cx, |cx| shows(cx, window, "ended"));
    assert!(!shows(cx, window, "stop"));
}

#[gpui_kit::test]
fn the_public_entry_point_opens_one_window(cx: &mut TestAppContext) {
    let _data = setup(cx);
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn the_window_is_titled_kagan_and_opens_at_its_size(cx: &mut TestAppContext) {
    let options = cx.update(crate::kagan::window_options);
    let title = options.titlebar.and_then(|bar| bar.title);
    assert_eq!(title.as_deref(), Some("Kagan"));
    let Some(WindowBounds::Windowed(bounds)) = options.window_bounds else {
        panic!("the window opens windowed");
    };
    assert_eq!(bounds.size, size(px(1120.), px(720.)));
}

#[gpui_kit::test]
fn the_new_thread_shortcut_asks_for_a_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
}

#[gpui_kit::test]
fn new_thread_asks_for_a_program_and_starts_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.dispatch_action(window, NewThread);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    assert_eq!(
        label(cx, window, "start-prompt"),
        "New thread in Kagan project"
    );
    assert_eq!(selected_of(cx, window, "project-prj_kagan"), Some(true));
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| picker.use_custom(shell(), "", window, cx));
    })
    .unwrap();
    click(cx, window, "start");
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)).is_some()
    });
    let started = cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap();
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(&started))
            .is_some()
    );
}

#[gpui_kit::test]
fn a_folder_that_is_not_git_starts_a_thread_in_itself(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let folder = data.path().join("plain");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("No git commit to start a worktree from")
    });
    assert!(!shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&folder).unwrap());
}

#[gpui_kit::test]
fn a_git_folder_starts_a_worktree_by_default(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
}

#[gpui_kit::test]
fn the_project_folder_choice_runs_in_the_git_folder(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    click(cx, window, "folder-project");
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&repo).unwrap());
}

/// Whether the element named `name` is checked, and whether it has focus.
fn choice_checked(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        gpui_kit::base::test_support::find(window, &[], &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.checked())
    })
    .unwrap()
}

fn choice_focused(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        gpui_kit::base::test_support::find(window, &[], &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.focused())
    })
    .unwrap()
}

/// Presses and releases a key the way the platform does: key down, then key up. A button
/// clicks from the key up, so a key down alone does nothing.
pub(super) fn press_key(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        let keystroke = kg_core::parse_key(key).unwrap();
        window.dispatch_event(
            gpui_kit::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(gpui_kit::KeyUpEvent { keystroke }.to_platform_input(), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_folder_choices_are_named_and_show_which_one_a_start_uses(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    assert_eq!(label(cx, window, "folder-project"), "Project folder");
    assert!(label(cx, window, "folder-worktree").starts_with("New worktree from "));
    assert_eq!(selected_of(cx, window, "folder-worktree"), Some(true));
    assert_eq!(selected_of(cx, window, "folder-project"), Some(false));
    assert!(shows(cx, window, "worktree-hint"));
    assert_eq!(
        choice_checked(cx, window, "folder-worktree-button"),
        Some(true)
    );
    click(cx, window, "folder-project");
    wait_until(cx, |cx| {
        choice_checked(cx, window, "folder-project-button") == Some(true)
    });
    assert_eq!(
        choice_checked(cx, window, "folder-worktree-button"),
        Some(false)
    );
    assert_eq!(selected_of(cx, window, "folder-worktree"), Some(false));
    assert_eq!(selected_of(cx, window, "folder-project"), Some(true));
}

#[gpui_kit::test]
fn the_folder_choice_takes_the_keyboard(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    for _ in 0..40 {
        if choice_focused(cx, window, "folder-project-button") == Some(true) {
            break;
        }
        cx.update_window(window, |_, window, cx| window.focus_next(cx))
            .unwrap();
        cx.run_until_parked();
    }
    assert_eq!(
        choice_focused(cx, window, "folder-project-button"),
        Some(true),
        "tab reaches the Project folder choice"
    );
    press_key(cx, window, "enter");
    assert_eq!(
        choice_checked(cx, window, "folder-project-button"),
        Some(true)
    );
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&repo).unwrap());
}

#[gpui_kit::test]
fn a_second_start_while_the_first_is_preparing_makes_no_second_thread(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let folder = data.path().join("plain");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("No git commit to start a worktree from")
    });
    set_program(cx, window, &kagan, shell());
    cx.simulate_keystrokes(window, "enter enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let listed = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::List {
                project_id: Some(project.id.clone()),
                archived: None,
                limit: None,
                before: None,
            }),
            cx,
        )
    });
    let OpResult::Threads(threads) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("threads");
    };
    assert_eq!(threads.len(), 1);
}

pub(super) fn shown(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Option<ThreadId> {
    cx.update(|cx| kagan.read(cx).shown_thread(cx))
}

fn set_program(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
    program: &str,
) {
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| picker.use_custom(program, "", window, cx));
    })
    .unwrap();
}

pub(super) fn environment_of(cx: &mut TestAppContext, thread: &ThreadId) -> kg_domain::Environment {
    let id = thread_record(cx, thread)
        .environment_id
        .expect("an environment");
    let task = cx.update(|cx| Workspace::dispatch(Op::Environment(EnvironmentOp::Get { id }), cx));
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Environment(environment) => environment,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
fn a_failed_branch_read_shows_its_error_and_starts_nothing(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    std::fs::remove_dir_all(&repo).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("read the git branches")
    });
    assert!(shows(cx, window, "folder-retry"));
    assert!(!shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    cx.update_window(window, |_, window, cx| {
        _ = crate::ui_dispatch::click(window, &ElementId::Name("start".into()), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(threads_in(cx, &project.id).is_empty());
}

#[gpui_kit::test]
fn a_worktree_starts_from_the_branch_on_screen_after_a_switch(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    let pinned = head_commit(&repo);
    git(&repo, &["checkout", "-q", "-b", "elsewhere"]);
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.base_branch.as_deref(), Some(pinned.as_str()));
    assert_eq!(head_commit(&environment.path), pinned);
}

#[gpui_kit::test]
fn closing_the_window_before_a_worktree_is_prepared_still_finishes_it(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    // Press Start without letting the executor run: the start is queued, not yet prepared. Then
    // close the window and drop the view, so the start has no window left to report to.
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(window, &ElementId::Name("start".into()), cx).unwrap();
    })
    .unwrap();
    let view = kagan.downgrade();
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    drop(kagan);
    cx.run_until_parked();
    assert!(
        view.upgrade().is_none(),
        "the closed window's view is released"
    );
    wait_until(cx, |cx| threads_in(cx, &project.id).len() == 1);
    let thread = threads_in(cx, &project.id).remove(0);
    let environment = environment_of(cx, &thread.id);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.status, EnvironmentStatus::Ready);
    assert_eq!(
        std::fs::read_dir(data.path().join("worktrees"))
            .unwrap()
            .count(),
        1
    );
}

#[gpui_kit::test]
fn a_detached_head_offers_a_worktree_from_its_commit(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let branch = current_branch(&repo);
    git(&repo, &["checkout", "-q", "--detach"]);
    // No local branch is left: the worktree must start from the commit HEAD points at.
    git(&repo, &["branch", "-D", &branch]);
    let head = head_commit(&repo);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.base_branch.as_deref(), Some(head.as_str()));
}

#[gpui_kit::test]
fn retry_recovers_a_failed_branch_read_through_its_action(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    std::fs::remove_dir_all(&repo).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("read the git branches")
    });
    init_repo(data.path());
    cx.simulate_keystrokes(window, &chord("RetryBranches"));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(
        environment_of(cx, &started).provider,
        EnvironmentProvider::Worktree
    );
}

fn head_commit(repo: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[gpui_kit::test]
fn the_folder_actions_pick_the_folder_from_the_keyboard(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    cx.simulate_keystrokes(window, &chord("UseProjectFolder"));
    cx.simulate_keystrokes(window, &chord("UseWorktreeFolder"));
    cx.simulate_keystrokes(window, &chord("UseProjectFolder"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(
        environment_of(cx, &started).provider,
        EnvironmentProvider::Project
    );
}

fn current_branch(repo: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub(super) fn threads_in(cx: &mut TestAppContext, project: &ProjectId) -> Vec<kg_domain::Thread> {
    let listed = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::List {
                project_id: Some(project.clone()),
                archived: None,
                limit: None,
                before: None,
            }),
            cx,
        )
    });
    let OpResult::Threads(threads) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("threads");
    };
    threads
}

#[gpui_kit::test]
fn the_title_bar_names_the_thread_and_the_terminal_starts_below_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "terminal-grid"));
    let title = thread_record(cx, &id).title;
    let native = gpui_kit::VisualTestContext::from_window(window, cx).window_title();
    assert_eq!(native, Some(format!("{title} — Kagan project")));
    let (heading_x, heading_w) =
        super::sidebars::span(cx, window, "thread-heading").expect("heading");
    let (left_x, left_w) =
        super::sidebars::span(cx, window, "left-sidebar-toggle").expect("left toggle");
    let (right_x, _) =
        super::sidebars::span(cx, window, "right-sidebar-toggle").expect("right toggle");
    assert!(
        heading_x + 0.5 >= left_x + left_w && heading_x + heading_w <= right_x + 0.5,
        "heading {heading_x}+{heading_w} between left {} and right {right_x}",
        left_x + left_w
    );
    cx.update_window(window, |_, window, _| {
        let grid = crate::ui_registry::find(window, &ElementId::Name("terminal-grid".into()))
            .expect("grid");
        let bar = f32::from(gpui_kit::component::TITLE_BAR_HEIGHT);
        let inset = 8.;
        assert!((f32::from(grid.bounds().origin.y) - (bar + inset)).abs() < 1.);
        let title = crate::ui_registry::find(window, &ElementId::Name("thread-title".into()))
            .expect("title");
        assert!(f32::from(title.bounds().origin.y + title.bounds().size.height) <= bar);
    })
    .unwrap();
}

#[gpui_kit::test]
fn archive_selects_another_thread_and_undo_brings_it_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let older = run_shell(cx);
    let newer = run_shell(cx);
    click(cx, window, &format!("thread-{}", newer.0));
    let title = thread_record(cx, &newer).title;
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &newer).archived_at.is_some());
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(older));
    let message = value_of(cx, window, "toast-message").unwrap_or_default();
    assert!(message.contains(&title), "{message}");
    assert!(message.starts_with("Archived “"), "{message}");
    assert_eq!(
        role_of(cx, window, "toast-message"),
        Some(gpui_kit::Role::Status)
    );
    assert_eq!(label(cx, window, "toast-message"), message);
    click(cx, window, "toast-undo");
    wait_until(cx, |cx| thread_record(cx, &newer).archived_at.is_none());
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(newer));
}

#[gpui_kit::test]
fn archive_skips_an_owned_thread_and_undo_restores_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let survivor = run_shell(cx);
    let owner = run_shell(cx);
    click(cx, window, &format!("thread-{}", owner.0));
    cx.dispatch_action(window, TogglePinned);
    wait_until(cx, |cx| thread_record(cx, &owner).pinned);
    let dependent = run_owned(cx, &owner);
    click(cx, window, &format!("thread-{}", owner.0));
    assert!(
        row_top(cx, window, &format!("thread-{}", owner.0))
            < row_top(cx, window, &format!("thread-{}", dependent.0)),
        "the owned thread is the next row"
    );
    assert!(
        row_top(cx, window, &format!("thread-{}", dependent.0))
            < row_top(cx, window, &format!("thread-{}", survivor.0))
    );
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| {
        thread_record(cx, &owner).archived_at.is_some()
            && thread_record(cx, &dependent).archived_at.is_some()
    });
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(survivor)
    );
    click(cx, window, "toast-undo");
    wait_until(cx, |cx| {
        thread_record(cx, &owner).archived_at.is_none()
            && thread_record(cx, &dependent).archived_at.is_none()
    });
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(owner));
}

fn run_owned(cx: &mut TestAppContext, owner: &ThreadId) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Visible),
        parent_thread_id: None,
        lifecycle_owner_thread_id: Some(owner.clone()),
        metadata: None,
    };
    let task = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    thread.id
}

#[gpui_kit::test]
fn a_narrow_delete_focuses_cancel(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: id.clone(),
                thread_id: Some(id.clone()),
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| {
            kagan
                .read(cx)
                .question_titles(cx)
                .iter()
                .any(|title| title.starts_with("Delete “"))
        })
    });
    let listed = cx.update(|cx| {
        Workspace::dispatch(Op::Thread(ThreadOp::Interactions { id: id.clone() }), cx)
    });
    let OpResult::Interactions(pending) = cx.foreground_executor().block_test(listed).unwrap()
    else {
        panic!("interactions");
    };
    let asked = pending.into_iter().next().expect("question");
    assert_eq!(asked.options[0].label, "Cancel");
    assert_eq!(asked.options[1].label, "Delete");
    let cancel = format!("answer-{}-decline", asked.id.0);
    let delete = format!("answer-{}-approve", asked.id.0);
    set_preference(cx, "main.right-sidebar", serde_json::json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    assert_eq!(focused_of(cx, window, &cancel), Some(true));
    assert_ne!(focused_of(cx, window, &delete), Some(true));
    let answered = cx.update(|cx| Workspace::answer(&id, asked.id, "decline".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    let error = cx.foreground_executor().block_test(deleting).unwrap_err();
    assert!(error.to_string().contains("declined"), "{error}");
}

#[gpui_kit::test]
fn archive_takes_the_thread_off_without_asking_by_default(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    assert!(
        !cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))
            .unwrap()
    );
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), None);
}

#[gpui_kit::test]
fn restore_brings_an_archived_thread_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());

    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    click(cx, window, &format!("thread-{}", id.0));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );
    assert!(shows(cx, window, "restore"));
    assert!(!shows(cx, window, "archive"));
    click(cx, window, "restore");
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_none());
}

#[gpui_kit::test]
fn archived_closes_from_search_and_close_control(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, "archived-search"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "archived-search"));
    wait_until(cx, |cx| focused_of(cx, window, "archived") == Some(true));
    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, "archived-close"));
    click(cx, window, "archived-close");
    wait_until(cx, |cx| !shows(cx, window, "archived-search"));
    wait_until(cx, |cx| focused_of(cx, window, "archived") == Some(true));
}

#[gpui_kit::test]
fn long_default_thread_title_ends_with_an_ellipsis(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let program = format!("{} {}", shell(), "x".repeat(100));
    let id = run_program(cx, &program);
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    cx.update_window(window, |_, window, _| {
        let row = crate::ui_registry::find(window, &ElementId::Name(format!("thread-{id}").into()))
            .expect("thread row");
        assert!(row.label().is_some_and(|label| label.contains('…')));
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_question_on_the_open_thread_appears_in_the_window(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    };
    let _asked = cx.update(|cx| Workspace::ask(&id, "Ship the change?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship the change?"]
    });
}

#[gpui_kit::test]
fn the_open_thread_is_the_highlighted_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    point_at(cx, window, "terminal");
    assert!(highlighted_at(cx, window, &format!("thread-{}", first.0)));
    assert!(!highlighted_at(cx, window, &format!("thread-{}", second.0)));
}

#[gpui_kit::test]
fn a_selected_row_keeps_its_highlight_under_the_pointer(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    point_at(cx, window, &format!("thread-{}", first.0));
    let hover = cx.update(|cx| cx.theme().list_hover);
    assert!(!quad_at_row(
        cx,
        window,
        &format!("thread-{}", first.0),
        hover
    ));
    point_at(cx, window, &format!("thread-{}", second.0));
    assert!(quad_at_row(
        cx,
        window,
        &format!("thread-{}", second.0),
        hover
    ));
}

#[test]
fn the_archive_text_says_what_goes_with_the_thread() {
    use kg_domain::ThreadStatus::{Active, Idle};
    let leaves = "It leaves the sidebar. Restore it from Archived.";
    assert_eq!(crate::kagan::description(Idle, 0), leaves);
    assert_eq!(
        crate::kagan::description(Active, 0),
        format!("This will stop current work. {leaves}")
    );
    assert_eq!(
        crate::kagan::description(Idle, 1),
        format!("1 thread that lives with this one will be archived with it. {leaves}")
    );
    assert_eq!(
        crate::kagan::description(Active, 3),
        format!(
            "This will stop current work. 3 threads that live with this one will be archived with it. {leaves}"
        )
    );
}

#[gpui_kit::test]
fn a_thread_is_listed_under_its_own_project(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    cx.run_until_parked();
    let thread = row_top(cx, window, &format!("thread-{}", id.0));
    assert!(row_top(cx, window, "project-prj_kagan") < thread);
    assert!(thread < row_top(cx, window, "project-personal"));
}

#[gpui_kit::test]
fn a_pinned_thread_sorts_above_the_others(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let older = run_shell(cx);
    let newer = run_shell(cx);
    cx.run_until_parked();
    click(cx, window, &format!("thread-{}", older.0));
    cx.dispatch_action(window, TogglePinned);
    wait_until(cx, |cx| thread_record(cx, &older).pinned);
    cx.run_until_parked();
    assert!(
        row_top(cx, window, &format!("thread-{}", older.0))
            < row_top(cx, window, &format!("thread-{}", newer.0))
    );
}

#[gpui_kit::test]
fn pin_marks_the_thread_pinned(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.dispatch_action(window, TogglePinned);
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
}

#[gpui_kit::test]
fn the_thread_actions_button_opens_its_menu_on_click(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    click(cx, window, "thread-actions-trigger");
    cx.run_until_parked();
    assert!(shows(cx, window, "popup-menu"), "the menu did not open");
    cx.simulate_keystrokes(window, "down enter");
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
}

#[gpui_kit::test]
fn the_thread_actions_items_are_named_and_activate_through_kg_ui(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    click(cx, window, "thread-actions-trigger");
    cx.run_until_parked();
    assert_eq!(label(cx, window, "thread-pin"), "Pin");
    assert_eq!(
        role_of(cx, window, "thread-pin"),
        Some(gpui_kit::Role::MenuItem)
    );
    assert_eq!(label(cx, window, "thread-archive"), "Archive");

    click(cx, window, "thread-pin");
    wait_until(cx, |cx| thread_record(cx, &id).pinned);

    // The menu says what the next press does: the same item now unpins.
    click(cx, window, "thread-actions-trigger");
    cx.run_until_parked();
    assert_eq!(label(cx, window, "thread-pin"), "Unpin");
}

#[gpui_kit::test]
fn the_plugin_folder_field_reports_its_text_and_focus(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, false);
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-install-open");
    click(cx, window, "plugin-source");
    assert_eq!(focused_of(cx, window, "plugin-source"), Some(true));

    cx.simulate_input(window, "/tmp/some-plugin");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "plugin-source"),
        Some("/tmp/some-plugin".into())
    );
}

#[gpui_kit::test]
fn archive_from_the_thread_actions_menu_archives_the_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    click(cx, window, "thread-actions-trigger");
    cx.simulate_keystrokes(window, "down down enter");
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
}

#[gpui_kit::test]
fn the_thread_actions_shortcut_opens_the_menu_with_focus_in_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.simulate_keystrokes(window, &chord("ThreadActions"));
    cx.run_until_parked();
    assert!(
        shows(cx, window, "popup-menu"),
        "the shortcut did not open the menu"
    );
    // Only the menu answers these keys, so Pin being chosen shows focus was inside it.
    cx.simulate_keystrokes(window, "down enter");
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
}

#[gpui_kit::test]
fn escape_from_the_thread_actions_menu_returns_focus_to_the_terminal(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.simulate_keystrokes(window, &chord("ThreadActions"));
    cx.run_until_parked();
    assert!(
        shows(cx, window, "popup-menu"),
        "the shortcut did not open the menu"
    );
    let inside = cx
        .update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap();
    assert!(!inside, "the terminal kept focus while the menu was open");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "popup-menu"),
        "escape kept the menu open"
    );
    let focused = cx
        .update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap();
    assert!(focused, "focus did not return to the terminal");
    assert!(!thread_record(cx, &id).pinned);
}

#[gpui_kit::test]
fn the_thread_actions_shortcut_does_nothing_under_settings(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"), "settings did not open");

    cx.simulate_keystrokes(window, &chord("ThreadActions"));
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "popup-menu"),
        "a menu was made under settings"
    );
    assert!(shows(cx, window, "settings-page"), "settings closed");
    let inside = cx
        .update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap();
    assert!(!inside, "focus moved to the covered terminal");
}

#[gpui_kit::test]
fn opening_settings_closes_an_open_thread_actions_menu(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, &chord("ThreadActions"));
    cx.run_until_parked();
    assert!(
        shows(cx, window, "popup-menu"),
        "the shortcut did not open the menu"
    );

    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "popup-menu"),
        "the menu stayed open under settings"
    );
    assert!(shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn escape_closes_the_thread_actions_menu_without_pinning(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    click(cx, window, "thread-actions-trigger");
    cx.run_until_parked();
    assert!(shows(cx, window, "popup-menu"), "the menu did not open");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "popup-menu"),
        "escape kept the menu open"
    );
    assert!(!thread_record(cx, &id).pinned);
    click(cx, window, "thread-actions-trigger");
    cx.run_until_parked();
    assert!(
        shows(cx, window, "popup-menu"),
        "the button did not open again"
    );
}

#[gpui_kit::test]
fn archive_asks_first_when_the_preference_is_on(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: "confirm-archive".into(),
                value: serde_json::Value::Bool(true),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();

    cx.dispatch_action(window, ArchiveThread);
    cx.run_until_parked();
    assert!(
        cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))
            .unwrap()
    );
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );
    assert!(thread_record(cx, &id).archived_at.is_none());
}

#[gpui_kit::test]
fn add_project_opens_the_picker_and_lists_the_folder(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("Fresh folder");
    std::fs::create_dir(&folder).unwrap();

    cx.dispatch_action(window, AddProject);
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.directories && !options.files);
        Some(vec![folder])
    });
    cx.run_until_parked();
    let listed = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .sidebar(cx)
            .projects
            .iter()
            .any(|project| project.name == "Fresh folder")
    });
    assert!(listed);
}

#[gpui_kit::test]
fn synthetic_input_cannot_answer_an_approval(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id()
            .0
            .clone()
    });
    let name = format!("answer-{approval}-yes");
    {
        let _armed = crate::synthetic::arm();
        click(cx, window, &name);
        assert!(crate::synthetic::take_person_blocked());
    }
    // A click made while a `kg ui` dispatch runs is synthetic too.
    crate::synthetic::with_active(|| click(cx, window, &name));
    assert!(crate::synthetic::take_person_blocked());
    let still_waiting = cx.update(|cx| Workspace::global(cx).read(cx).approvals().count());
    assert_eq!(still_waiting, 1);
    click(cx, window, &name);
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
}

#[gpui_kit::test]
fn unrelated_input_succeeds_once_while_an_approval_waits(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.run_until_parked();
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let _asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let name = approval_answer(cx);

    // Pinning flips the thread. A retry after a false failure would unpin it, so one success must
    // mean exactly one change.
    assert_eq!(
        ui(
            cx,
            UiOp::Click {
                element_id: format!("thread-{}", id.0),
                within: None,
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(
        ui(
            cx,
            UiOp::Key {
                keystroke: chord("TogglePinned"),
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
    cx.run_until_parked();
    assert!(thread_record(cx, &id).pinned);
    assert_eq!(
        ui(
            cx,
            UiOp::Key {
                keystroke: chord("ToggleSidebar"),
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(approvals_waiting(cx), 1);
    click(cx, window, &name);
}

#[gpui_kit::test]
fn an_answer_attempt_fails_and_leaves_the_approval_waiting(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let name = approval_answer(cx);

    let clicked = ui(
        cx,
        UiOp::Click {
            element_id: name.clone(),
            within: None,
            window: 0,
        },
    );
    assert!(matches!(clicked, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY));
    assert_eq!(approvals_waiting(cx), 1);

    // Tab until the answer button reports focus, then press Enter. Enter must be refused.
    let mut refused_by_keyboard = false;
    for _ in 0..40 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of(cx, window, &name) != Some(true) {
            continue;
        }
        let keyed = ui(
            cx,
            UiOp::Key {
                keystroke: "enter".into(),
                window: 0,
            },
        );
        refused_by_keyboard =
            matches!(keyed, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY);
        break;
    }
    assert!(
        refused_by_keyboard,
        "Enter never reached the approval button"
    );
    assert_eq!(approvals_waiting(cx), 1);

    click(cx, window, &name);
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
}

fn approval_answer(cx: &mut TestAppContext) -> String {
    let id = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id()
            .0
            .clone()
    });
    format!("answer-{id}-yes")
}

fn approvals_waiting(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).approvals().count())
}

fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    cx.foreground_executor().block_test(task)
}

#[gpui_kit::test]
fn an_approval_can_be_answered(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![
        InteractionOption {
            id: "yes".into(),
            label: "Yes".into(),
        },
        InteractionOption {
            id: "no".into(),
            label: "No".into(),
        },
    ];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id()
            .0
            .clone()
    });
    click(cx, window, &format!("answer-{approval}-yes"));
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
}

#[gpui_kit::test]
fn opening_again_brings_the_same_window_forward(cx: &mut TestAppContext) {
    let _data = setup(cx);
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn the_right_sidebar_opens_and_closes_from_its_shortcut(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    assert!(cx.update(|cx| kagan.read(cx).right_open()));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(!cx.update(|cx| kagan.read(cx).right_open()));
    assert!(!shows(cx, window, "needs-you"));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(cx.update(|cx| kagan.read(cx).right_open()));
    assert!(shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn a_question_waits_while_the_person_types(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, "x");
    cx.run_until_parked();

    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    };
    let _asked = cx.update(|cx| Workspace::ask(&id, "Ship the change?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship the change?"]
    });
    let question = pending(cx, &id).remove(0).id.0;
    assert!(shows(cx, window, "needs-held"));
    assert!(!shows(cx, window, &format!("answer-{question}-yes")));

    cx.executor().advance_clock(QUIET);
    cx.run_until_parked();
    assert!(!shows(cx, window, "needs-held"));
    assert!(shows(cx, window, &format!("answer-{question}-yes")));
}

#[gpui_kit::test]
fn dock_count_tracks_distinct_threads_after_the_window_closes(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", second.0));
    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    };
    let _one = cx.update(|cx| Workspace::ask(&first, "First?".into(), vec![option.clone()], cx));
    let _two = cx.update(|cx| Workspace::ask(&first, "Second?".into(), vec![option.clone()], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            let attention = crate::kagan::attention::global(cx);
            let attention = attention.read(cx);
            attention.questions.len() == 2 && attention.count(cx) == 1
        })
    });
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let _three = cx.update(|cx| Workspace::ask(&second, "Third?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            let attention = crate::kagan::attention::global(cx);
            let attention = attention.read(cx);
            attention.questions.len() == 3 && attention.count(cx) == 2
        })
    });
}

#[gpui_kit::test]
fn questions_from_every_thread_wait_in_needs_you(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    };
    let _one = cx.update(|cx| Workspace::ask(&first, "Ship one?".into(), vec![option.clone()], cx));
    let _two = cx.update(|cx| Workspace::ask(&second, "Ship two?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)).len() == 2
    });
    let one = pending(cx, &first).remove(0).id.0;
    let two = pending(cx, &second).remove(0).id.0;
    assert!(shows(cx, window, &format!("answer-{one}-yes")));
    assert!(shows(cx, window, &format!("answer-{two}-yes")));
}

#[gpui_kit::test]
fn changes_lists_the_files_the_shown_thread_changed(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));

    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    wait_until(cx, |cx| shows(cx, window, "changes-not-git"));
    assert!(!shows(cx, window, "change-notes.txt"));
    click(cx, window, &format!("thread-{}", thread.0));
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
}

#[gpui_kit::test]
fn an_open_diff_follows_the_disk_without_a_click(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("two"))
    });

    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\n").unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("three"))
    });
}

#[gpui_kit::test]
fn disabling_a_plugin_takes_its_panel_away(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins(cx, true);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    open_surface(cx, &PluginId(PAGE.into()), "page");
    wait_until(cx, |cx| shows(cx, window, "page-button"));

    let off = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Disable {
                id: PluginId(PAGE.into()),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(off).unwrap();
    wait_until(cx, |cx| !shows(cx, window, "page-button"));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).panel_titles()),
        ["Questions", "Changes", "Details"]
    );
}

#[gpui_kit::test]
fn the_right_sidebar_keeps_its_width_when_collapsed(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_right(px(420.), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    let dragged = cx.update(|cx| kagan.read(cx).right_width_now(cx));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(!cx.update(|cx| kagan.read(cx).right_open()));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    let reopened = cx.update(|cx| kagan.read(cx).right_width_now(cx));
    assert!(
        (reopened - dragged).abs() < 1.,
        "dragged {dragged}, reopened {reopened}"
    );
}

#[gpui_kit::test]
fn the_right_sidebar_open_state_is_saved(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    let saved = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: "main.right-sidebar".into(),
            }),
            cx,
        )
    });
    let OpResult::Setting(value) = cx.foreground_executor().block_test(saved).unwrap() else {
        panic!("setting");
    };
    assert_eq!(value, serde_json::json!(false));
}

#[gpui_kit::test]
fn a_question_on_a_hidden_thread_waits_in_needs_you(cx: &mut TestAppContext) {
    let (kagan, _window, _data) = open(cx);
    let id = run_hidden_shell(cx);
    let option = InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    };
    let _asked = cx.update(|cx| Workspace::ask(&id, "Hidden ask?".into(), vec![option], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            let attention = crate::kagan::attention::global(cx);
            let attention = attention.read(cx);
            kagan.read(cx).question_titles(cx) == ["Hidden ask?"] && attention.count(cx) == 0
        })
    });
}

#[gpui_kit::test]
fn a_question_already_on_screen_stays_up_while_the_person_types(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let option = || InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    };
    let _first = cx.update(|cx| Workspace::ask(&id, "Ship one?".into(), vec![option()], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship one?"]
    });
    let first = pending(cx, &id).remove(0).id.0;
    assert!(shows(cx, window, &format!("answer-{first}-yes")));

    cx.simulate_keystrokes(window, "x");
    cx.run_until_parked();
    let _second = cx.update(|cx| Workspace::ask(&id, "Ship two?".into(), vec![option()], cx));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)).len() == 2
    });
    let second = pending(cx, &id)
        .into_iter()
        .find(|asked| asked.title == "Ship two?")
        .expect("second question")
        .id
        .0;
    assert!(shows(cx, window, &format!("answer-{first}-yes")));
    assert!(!shows(cx, window, &format!("answer-{second}-yes")));
    assert!(shows(cx, window, "needs-held"));

    cx.executor().advance_clock(QUIET);
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("answer-{second}-yes")));
}

#[gpui_kit::test]
fn a_plugin_panel_is_told_the_shown_thread_and_project(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, true);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.run_until_parked();
    let shown = cx.update(|cx| {
        HostUi::global(cx).and_then(|host| host.read(cx).window_thread(window.window_id()))
    });
    assert_eq!(shown, Some((Some(id), Some(ProjectId("prj_kagan".into())))));
}

#[gpui_kit::test]
fn tab_reaches_a_changed_file_and_enter_or_space_opens_it(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    let row = cx
        .update(|cx| kagan.read(cx).change_focus("notes.txt"))
        .expect("a focus handle for the row");

    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    for _ in 0..60 {
        if cx
            .update_window(window, |_, window, _| row.is_focused(window))
            .unwrap()
        {
            break;
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    assert!(
        cx.update_window(window, |_, window, _| row.is_focused(window))
            .unwrap(),
        "Tab did not reach the row"
    );

    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\n").unwrap();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("three")),
        "Enter did not open the file"
    );

    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.focus(&row, cx))
        .unwrap();
    cx.simulate_keystrokes(window, "space");
    cx.run_until_parked();
    assert!(
        open_lines(cx, &kagan)
            .iter()
            .any(|line| line.contains("four")),
        "Space did not open the file"
    );
}

#[gpui_kit::test]
fn the_right_sidebar_draws_from_its_registry(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.run_until_parked();
    assert!(shows(cx, window, "changes"));
    cx.update(|cx| {
        kagan.update(cx, |kagan, cx| kagan.drop_panel_for_test("changes", cx));
    });
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "changes"),
        "a panel dropped from the registry is still drawn"
    );
    assert!(shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn a_question_answered_elsewhere_leaves_needs_you(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let option = |id: &str| InteractionOption {
        id: id.into(),
        label: id.into(),
    };
    let _asked = cx.update(|cx| {
        Workspace::ask(
            &id,
            "Ship it?".into(),
            vec![option("yes"), option("no")],
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship it?"]
    });
    let question = pending(cx, &id).remove(0).id;
    assert!(shows(cx, window, &format!("answer-{}-yes", question.0)));

    let answered = cx.update(|cx| Workspace::answer(&id, question.clone(), "no".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx))
            .is_empty()
    });
    assert!(!shows(cx, window, &format!("answer-{}-yes", question.0)));
}

#[gpui_kit::test]
fn reloading_a_plugin_mounts_its_panel_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, true);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    let id = PluginId(PAGE.into());
    let opened = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(kg_domain::SurfaceOp::Open {
                plugin: id.clone(),
                id: "page".into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
    wait_until(cx, |cx| shows(cx, window, "page-button"));
    let before = cx
        .update(|cx| PluginHost::global(cx).read(cx).generation(&id))
        .expect("the page plugin has loaded");

    let reload = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Reload {
                id: id.clone(),
                thread_id: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(reload).unwrap();
    cx.run_until_parked();

    let after = cx
        .update(|cx| PluginHost::global(cx).read(cx).generation(&id))
        .expect("the page plugin is still loaded");
    assert!(after > before, "the reload did not load the plugin again");
    wait_until(cx, |cx| shows(cx, window, "page-button"));
    assert!(!shows(cx, window, PAGE_FRAME));
}

/// Installs a plugin whose only contribution is the overlay `Cover`, exported by `source`.
pub(super) fn install_overlay(data: &Path, id: &str, source: &str) {
    let root = data.join(id);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": id,
        "name": "Cover",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Draws over the terminal.",
        "powers": {},
        "ui": { "overlay": "Cover" }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), source).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
            error: None,
        })
        .unwrap();
}

/// Draws one box, plus whatever an attack adds to it.
pub(super) fn overlay_source(attack: &str) -> String {
    format!(
        r##"import {{ View, div, image }} from "gpui-kit";
import {{ InputState, Popover }} from "gpui-base";
export default class Plugin extends View {{
  render() {{ return ""; }}
}}
export class Cover extends View {{
  init(_props, cx) {{
    this.focus = cx.focus_handle();
    {attack}
  }}
  render() {{
    return div().size_full().child(div().absolute().left(8).top(8).w(40).h(40).bg("#ff00ff")){attack_child};
  }}
}}
"##,
        attack = if attack.starts_with("cx.") {
            attack
        } else {
            ""
        },
        attack_child = if attack.starts_with(".child") {
            attack
        } else {
            ""
        },
    )
}

fn grid_focus_and_selection_survive(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
    window: AnyWindowHandle,
) {
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, kagan).contains("picked"));
    let (row, _) = shown_row(cx, kagan, "picked");
    let start = cell_point(cx, window, kagan, row, 0);
    press(cx, window, start, 1, Modifiers::default());
    let end = cell_point(cx, window, kagan, row, 4);
    drag_to(cx, window, end);
    release(cx, window, end, 1, Modifiers::default());
    wait_until(cx, |cx| !selection(cx, kagan).is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_overlay_that_reaches_for_the_keyboard_is_refused_and_typing_goes_on(cx: &mut TestAppContext) {
    let plugin = "dev.test.grabbing";
    let attack =
        "cx.timer.every(20, () => { this.focus.focus(); this.input = InputState.new(); });";
    let (kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &overlay_source(attack));
    });
    cx.update_window(window, |_, window, _| window.activate_window())
        .unwrap();
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{plugin}");
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("cannot use the window")
    });
    wait_until(cx, |cx| !present(cx, window, &overlay));
    assert!(
        cx.update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap()
    );
    // Typed in the same tick as the timer: no frame runs in between.
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.simulate_input(window, "echo still''here");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("stillhere"));
    grid_focus_and_selection_survive(cx, &kagan, window);
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_overlay_with_a_press_outside_handler_is_refused_and_the_terminal_takes_presses(
    cx: &mut TestAppContext,
) {
    let plugin = "dev.test.outside";
    let attack = ".child(div().w(10).h(10).on_mouse_down_out((_e, cx) => cx.stop_propagation()))";
    let (kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &overlay_source(attack));
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("on_mouse_down_out")
    });
    assert!(!present(cx, window, &format!("plugin-overlay-{plugin}")));
    grid_focus_and_selection_survive(cx, &kagan, window);
}

/// A plugin with two right panels, each a `KgButton` named `count`. Both panels count clicks in
/// their label, so the same local id appears twice.
fn install_twin(data: &Path) {
    let root = data.join("plugins").join("twin");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.twin",
        "name": "Twin",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Two panels with one button id.",
        "powers": {},
        "ui": { "panels": [
            { "id": "a", "title": "A", "view": "PanelA", "placement": "left" },
            { "id": "b", "title": "B", "view": "PanelB", "placement": "center" }
        ] }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class Plugin extends View {
  render() { return ""; }
}
export class PanelA extends View {
  init(_props, _cx) { this.count = 0; }
  render() {
    return div().child(new KgButton("count").label(`A ${this.count}`)
      .on_click((_event, cx) => { this.count++; cx.notify(); }));
  }
}
export class PanelB extends View {
  init(_props, _cx) { this.count = 0; }
  render() {
    return div().child(new KgButton("count").label(`B ${this.count}`)
      .on_click((_event, cx) => { this.count++; cx.notify(); }));
  }
}
"#,
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.twin".into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({ "powers": {}, "downloads": {}, "folders": {} }),
            error: None,
        })
        .unwrap();
}

/// The label a control shows, as `kg ui snapshot` reports it.
fn observed_label(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.label().map(str::to_owned))
    })
    .unwrap()
}

/// A click through the driver, as `kg ui click` sends it.
fn click_through_driver(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    within: Option<&str>,
) -> Result<(), OpError> {
    let within = within.map(|within| ElementId::Name(within.to_owned().into()));
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click_within(
            window,
            &ElementId::Name(name.to_owned().into()),
            within.as_ref(),
            cx,
        )
    })
    .unwrap()
}

fn run_op(cx: &mut TestAppContext, op: Op) -> OpResult {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task).unwrap()
}

fn open_surface(cx: &mut TestAppContext, plugin: &PluginId, id: &str) {
    let opened = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(kg_domain::SurfaceOp::Open {
                plugin: plugin.clone(),
                id: id.into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
}

#[gpui_kit::test]
fn a_generated_plugin_button_is_installed_found_clicked_and_checked_after_a_reload(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |_| {});
    cx.run_until_parked();
    let folder = data.path().join("counter-lab");
    let id = PluginId("dev.kagan.counter-lab".into());

    // `kg plugin new` writes the starter, which has the panel and its KgButton.
    let created = run_op(
        cx,
        Op::Plugin(PluginOp::New {
            name: "counter-lab".into(),
            dir: Some(folder.clone()),
        }),
    );
    let OpResult::Files(files) = created else {
        panic!("the starter lists its files");
    };
    // The declarations it type-checks against include KgButton, so the starter can import it.
    let declared = files.iter().any(|file| {
        file.extension().is_some_and(|extension| extension == "ts")
            && std::fs::read_to_string(file).is_ok_and(|text| text.contains("KgButton"))
    });
    assert!(declared, "{files:?}");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("kagan.json")).unwrap()).unwrap();
    assert_eq!(manifest["commands"][0]["id"], "hello");
    assert!(manifest["commands"][0].get("key").is_none(), "{manifest}");

    // `kg plugin install` waits for the person. The test answers as the person would.
    let install = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source: folder.display().to_string(),
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("an approval waiting for the person")
            .id()
            .clone()
    });
    let approved = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(approved).unwrap();
    cx.foreground_executor().block_test(install).unwrap();
    cx.run_until_parked();

    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    open_surface(cx, &id, "hello");
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 0")
    });

    assert_eq!(click_through_driver(cx, window, "count", None), Ok(()));
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 1")
    });

    let reload = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Reload {
                id: id.clone(),
                thread_id: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(reload).unwrap();
    cx.run_until_parked();
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 0")
    });
    assert_eq!(click_through_driver(cx, window, "count", None), Ok(()));
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 1")
    });

    // The starter's command runs from the shell, and writes to the log `kg plugin logs` reads.
    let ran = run_op(
        cx,
        Op::Plugin(PluginOp::Command {
            id: id.clone(),
            command: "hello".into(),
            thread_id: None,
            project_id: None,
        }),
    );
    assert_eq!(ran, OpResult::Done);
    let logs = run_op(
        cx,
        Op::Plugin(PluginOp::Logs {
            id,
            follow: false,
            after: None,
        }),
    );
    let OpResult::Logs { text, .. } = logs else {
        panic!("the logs of the starter")
    };
    assert!(text.contains("hello from the starter plugin"), "{text}");

    starter_key_runs_hello(cx, window);
}

/// The starter's `hello` has no key, so a second starter cannot lose one. Bound in Settings, the
/// key runs it from the window, with no command line.
fn starter_key_runs_hello(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let hellos = |cx: &mut TestAppContext| {
        plugin_log(cx, "dev.kagan.counter-lab")
            .matches("hello from the starter plugin")
            .count()
    };
    let row = "shortcut-plugin-dev.kagan.counter-lab-hello";
    click(cx, window, "settings-gear");
    click(cx, window, "setting-row-plugin-dev.kagan.counter-lab-hello");
    assert_eq!(observed_label(cx, window, row).as_deref(), Some("Not set"));
    click(cx, window, &format!("{row}-key"));
    cx.simulate_keystrokes(window, "secondary-shift-h");
    wait_until(cx, |cx| {
        observed_label(cx, window, row)
            .is_some_and(|label| label != "Not set" && label != "Press a key")
    });
    click(cx, window, "settings-close");
    let before = hellos(cx);
    cx.simulate_keystrokes(window, "secondary-shift-h");
    wait_until(cx, |cx| hellos(cx) > before);
}

/// Every observed element with this id, as its label, in the latest frame, sorted.
fn labels_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Vec<Option<String>> {
    let mut labels: Vec<Option<String>> = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::snapshots(window)
                .iter()
                .filter(|element| {
                    element.path().last() == Some(&ElementId::Name(name.to_owned().into()))
                })
                .map(|element| element.label().map(str::to_owned))
                .collect()
        })
        .unwrap();
    labels.sort();
    labels
}

#[gpui_kit::test]
fn two_panels_with_one_local_id_are_ambiguous_until_a_scope_names_one(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_twin);
    cx.simulate_window_resize(window, size(px(1400.), px(1800.)));
    cx.run_until_parked();
    let open = |id: &str| {
        let task = cx.update(|cx| {
            Workspace::dispatch(
                Op::Surface(kg_domain::SurfaceOp::Open {
                    plugin: PluginId("dev.kagan.twin".into()),
                    id: id.into(),
                }),
                cx,
            )
        });
        cx.foreground_executor().block_test(task).unwrap();
    };
    open("a");
    open("b");
    wait_until(cx, |cx| labels_of(cx, window, "count").len() == 2);
    assert_eq!(
        labels_of(cx, window, "count"),
        [Some("A 0".into()), Some("B 0".into())]
    );

    // Without a scope the click is refused and nothing changes; it does not panic.
    let refused = click_through_driver(cx, window, "count", None);
    assert!(
        matches!(&refused, Err(OpError::Invalid(message)) if message.contains("2 elements")),
        "{refused:?}"
    );
    cx.run_until_parked();
    assert_eq!(
        labels_of(cx, window, "count"),
        [Some("A 0".into()), Some("B 0".into())]
    );

    // Scoped to panel B, the click reaches only that panel.
    let scoped = click_through_driver(cx, window, "count", Some("surface-page"));
    assert_eq!(scoped, Ok(()));
    wait_until(cx, |cx| {
        labels_of(cx, window, "count").contains(&Some("B 1".into()))
    });
    assert_eq!(
        labels_of(cx, window, "count"),
        [Some("A 0".into()), Some("B 1".into())]
    );
}

#[gpui_kit::test]
fn the_switch_and_the_custom_command_input_state_their_values(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let value = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("agent-integration".into()))
                .and_then(|element| element.value().map(str::to_owned))
        })
        .unwrap()
    };
    let before = value(cx);
    assert!(
        before == Some("on".into()) || before == Some("off".into()),
        "{before:?}"
    );
    click(cx, window, "agent-integration");
    cx.run_until_parked();
    assert_ne!(value(cx), before, "the switch reports its new value");
    click(cx, window, "settings-close");

    click(cx, window, "new-personal-thread");
    cx.run_until_parked();
    click(cx, window, "custom-command-toggle");
    click(cx, window, "custom-program");
    cx.simulate_input(window, "zsh");
    cx.run_until_parked();
    let program = cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name("custom-program".into()))
            .and_then(|element| element.value().map(str::to_owned))
    });
    assert_eq!(program.unwrap(), Some("zsh".into()));
}

#[gpui_kit::test]
fn installing_from_the_plugins_box_asks_for_approval_in_needs_you(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, false);
    // A small plugin whose clipboard power needs the person's approval.
    let plugin = tempfile::tempdir().unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.echo",
        "name": "Echo",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Echo things back.",
        "powers": { "clipboard": { "write": true } },
        "ui": { "panels": [] }
    });
    std::fs::write(plugin.path().join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class Plugin {}\n",
    )
    .unwrap();
    let root = dunce::canonicalize(plugin.path()).unwrap();

    // Type the folder into the install form on the Installed page and press its Install button.
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-install-open");
    click(cx, window, "plugin-source");
    cx.simulate_input(window, &root.display().to_string());
    cx.run_until_parked();
    click(cx, window, "plugin-install");
    cx.run_until_parked();

    let approval = cx
        .update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .approvals()
                .next()
                .map(|approval| approval.id().clone())
        })
        .expect("an approval waiting for the person");
    assert!(shows(cx, window, &format!("answer-{}-decline", approval.0)));
    assert_eq!(
        label(cx, window, &format!("answer-{}-approve", approval.0)),
        "Install"
    );
    assert_eq!(
        label(cx, window, &format!("answer-{}-decline", approval.0)),
        "Cancel"
    );

    assert!(!shows(cx, window, "plugin-install-open"));
    assert!(!shows(cx, window, "plugin-install-form"));
    assert!(!shows(cx, window, "plugin-install"));
    assert!(!shows(cx, window, "plugin-source"));
    click(cx, window, "setting-row-install-waiting");
    assert!(shows(cx, window, "setting-row-install-waiting"));

    // Only the person answers an install; the test answers through the same core call.
    let accepted =
        cx.update(|cx| Workspace::answer_standalone(approval.clone(), "approve".into(), cx));
    cx.foreground_executor().block_test(accepted).unwrap();
    wait_until(cx, |cx| shows(cx, window, "plugin-row-dev.kagan.echo"));
    assert!(!shows(cx, window, "plugin-install-form"));
    assert_eq!(
        focused_of(cx, window, "plugin-row-dev.kagan.echo"),
        Some(true)
    );
    click(cx, window, "plugin-install-open");
    assert_eq!(value_of(cx, window, "plugin-source"), Some(String::new()));
    assert!(!shows(
        cx,
        window,
        &format!("answer-{}-decline", approval.0)
    ));
}

#[gpui_kit::test]
fn choosing_a_plugin_folder_fills_the_install_form(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins(cx, false);
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-install-open");
    click(cx, window, "plugin-choose-folder");
    assert!(cx.did_prompt_for_paths());
    let path = data.path().to_path_buf();
    let expected = path.to_string_lossy().into_owned();
    cx.simulate_path_prompt_response(move |_| Some(vec![path]));
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, "plugin-source"), Some(expected));
}

/// The open diff's lines as the window holds them.
fn open_lines(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Vec<String> {
    cx.update(|cx| review_read::open_diff_lines(kagan.read(cx)))
}

/// A git repo with one commit, added as a project. Returns its folder and the project.
fn committed_repo(cx: &mut TestAppContext, data: &Path) -> (std::path::PathBuf, Project) {
    let repo = init_repo(data);
    let project = add_project(cx, &repo);
    (repo, project)
}

/// A git folder with one commit, not yet a project.
fn init_repo(data: &Path) -> std::path::PathBuf {
    let repo = data.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("notes.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "start",
        ],
    );
    repo
}

pub(super) fn add_project(cx: &mut TestAppContext, path: &Path) -> Project {
    let added = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Add {
                path: path.to_path_buf(),
            }),
            cx,
        )
    });
    let OpResult::Project(project) = cx.foreground_executor().block_test(added).unwrap() else {
        panic!("project");
    };
    project
}

/// A git repo with one committed file, added as a project, and a thread running in it. The file
/// is changed on disk after the commit, and the thread is shown.
pub(super) fn repo_thread(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    data: &Path,
) -> (ThreadId, std::path::PathBuf) {
    let (repo, project) = committed_repo(cx, data);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\n").unwrap();
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
    (thread.id, repo)
}

/// A running shell thread that no sidebar row lists.
pub(super) fn run_hidden_shell(cx: &mut TestAppContext) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Hidden),
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let task = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    thread.id
}

#[gpui_kit::test]
fn a_plugin_panel_shows_beside_the_built_in_panels(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins(cx, true);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.run_until_parked();
    assert!(shows(cx, window, "needs-you"));
    assert!(shows(cx, window, "changes"));
    assert!(!shows(cx, window, "plugins"));
    assert!(shows(cx, window, "plugins-manage"));
    assert!(!shows(cx, window, PAGE_FRAME));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).panel_titles()),
        ["Questions", "Changes", "Details"]
    );
}

pub(super) const PAGE: &str = "dev.test.page";
pub(super) const PAGE_FRAME: &str = "panel-dev.test.page-page";

/// Writes a plugin with one center page that follows the thread, installed and enabled.
pub(super) fn install_page(data: &Path) {
    let root = data.join("page");
    std::fs::create_dir(&root).unwrap();
    let manifest = serde_json::json!({
        "id": PAGE,
        "name": "Page",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A test page.",
        "powers": {},
        "ui": {"panels": [{"id": "page", "title": "Page", "view": "Page", "placement": "center", "follows": "thread"}]}
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class Plugin extends View {}
export class Page extends View {
  render() { return div().child(new KgButton("page-button").label("Page")); }
}
"#,
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(PAGE.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
            error: None,
        })
        .unwrap();
}

/// Opens the window with the plugin host running. `page` also installs a plugin with a center page.
pub(super) fn open_with_plugins(
    cx: &mut TestAppContext,
    page: bool,
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    open_with_plugins_then(cx, |data| {
        if page {
            install_page(data);
        }
    })
}

/// Opens the window with the plugin host running, after `install` has written the plugins.
pub(super) fn open_with_plugins_then(
    cx: &mut TestAppContext,
    install: impl FnOnce(&Path),
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let data = host_then(cx, install);
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    (kagan, window, data)
}

pub(super) fn host(cx: &mut TestAppContext) -> tempfile::TempDir {
    host_then(cx, |_| {})
}

fn host_then(cx: &mut TestAppContext, install: impl FnOnce(&Path)) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    seed(data.path());
    install(data.path());
    let opened =
        cx.update(|cx| Workspace::open(data.path().into(), std::env::current_exe().unwrap(), cx));
    cx.foreground_executor().block_test(opened).unwrap();
    let started = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_shell::init(cx);
        crate::init(cx);
        crate::theme::init_in(data.path().join("themes"), cx);
        let mut catalog = gpui_shell::components::registry().unwrap();
        crate::register_components(&mut catalog).unwrap();
        kg_plugins::init(cx, catalog.freeze().unwrap())
    });
    cx.foreground_executor().block_test(started).unwrap();
    data
}

fn git(path: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args([
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn pending(
    cx: &mut TestAppContext,
    id: &ThreadId,
) -> Vec<kg_domain::PendingInteraction> {
    let task = cx.update(|cx| {
        Workspace::dispatch(Op::Thread(ThreadOp::Interactions { id: id.clone() }), cx)
    });
    let OpResult::Interactions(list) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("interactions");
    };
    list
}

pub(super) fn open(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let data = setup(cx);
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    (kagan, window, data)
}

pub(super) fn setup(cx: &mut TestAppContext) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    seed(data.path());
    let opened =
        cx.update(|cx| Workspace::open(data.path().into(), std::env::current_exe().unwrap(), cx));
    cx.foreground_executor().block_test(opened).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::init(cx);
        crate::theme::init_in(data.path().join("themes"), cx);
    });
    data
}

fn seed(dir: &Path) {
    let store = kg_store::Store::open(dir.join("kagan.sqlite3")).unwrap();
    store
        .put_project(&Project {
            id: ProjectId("prj_kagan".into()),
            kind: ProjectKind::Personal,
            name: "Kagan project".into(),
            path: dir.to_path_buf(),
            git_remote_url: None,
            sort: 0,
        })
        .unwrap();
}

pub(super) fn run_shell(cx: &mut TestAppContext) -> ThreadId {
    run_program(cx, shell())
}

pub(super) fn run_program(cx: &mut TestAppContext, program: &str) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: program.into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let task = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    thread.id
}

/// A thread run under `parent`, shown in the sidebar under it.
pub(super) fn run_child(cx: &mut TestAppContext, parent: &ThreadId) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Visible),
        parent_thread_id: Some(parent.clone()),
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let task = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    thread.id
}

fn input(id: &ThreadId, text: &str) -> Op {
    Op::Thread(ThreadOp::Input {
        id: id.clone(),
        text: text.into(),
        terminal: None,
    })
}

/// Writes one preference through the same Op `kg preference set` runs.
pub(super) fn set_preference(cx: &mut TestAppContext, key: &str, value: serde_json::Value) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: key.into(),
                value,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

pub(super) fn click(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.update_window(window, |_, window, cx| {
        let id = ElementId::Name(name.to_owned().into());
        crate::ui_dispatch::click(window, &id, cx)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    })
    .unwrap();
    cx.run_until_parked();
}

/// Counts how often the view asks to be drawn. Keep the subscription alive as long as the count.
fn count_repaints(
    cx: &mut TestAppContext,
    view: &Entity<TerminalView>,
) -> (Rc<Cell<usize>>, gpui_kit::Subscription) {
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let subscription = cx.update(|cx| cx.observe(view, move |_, _| seen.set(seen.get() + 1)));
    (count, subscription)
}

fn host_grid(cx: &mut TestAppContext, id: &ThreadId) -> Grid {
    cx.update(|cx| Workspace::global(cx).read(cx).grid(id, None))
        .expect("the host holds a screen")
}

pub(super) fn grid_text(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> String {
    let grid = cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx));
    grid.map_or_else(String::new, |grid| {
        grid.lines
            .iter()
            .map(|line| line.iter().map(|cell| cell.c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(super) fn thread_record(cx: &mut TestAppContext, id: &ThreadId) -> kg_domain::Thread {
    let task =
        cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Get { id: id.clone() }), cx));
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Thread(thread) => thread,
        other => panic!("{other:?}"),
    }
}

fn thread_status(cx: &mut TestAppContext, id: &ThreadId) -> ThreadStatus {
    thread_record(cx, id).status
}

pub(super) fn wait_until(
    cx: &mut TestAppContext,
    mut done: impl FnMut(&mut TestAppContext) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the screen"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_thread_marks_busy_then_done_then_idle_once_read(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (_kagan, window, _data) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    let program = folder.path().join("agent");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf '\\033]2;\\342\\227\\220 work\\007'; sleep 1; printf '\\033]2;\\342\\234\\263 work\\007'; sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let thread = run_program(cx, &program.display().to_string());
    let other = run_shell(cx);
    let mark = format!("thread-mark-{}", thread.0);
    click(cx, window, &format!("thread-{}", thread.0));
    wait_for_mark(cx, window, &mark, "busy");
    click(cx, window, &format!("thread-{}", other.0));
    wait_for_mark(cx, window, &mark, "done");
    let asking = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: thread.clone(),
                title: "Keep going?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
            }),
            cx,
        )
    });
    wait_for_mark(cx, window, &mark, "needs you");
    click(cx, window, &format!("thread-{}", thread.0));
    wait_for_mark(cx, window, &mark, "needs you");
    let question = pending(cx, &thread).into_iter().next().expect("question");
    click(cx, window, &format!("answer-{}-yes", question.id.0));
    cx.foreground_executor().block_test(asking).unwrap();
    wait_for_mark(cx, window, &mark, "idle");
}

#[gpui_kit::test]
fn a_pending_question_stays_needs_you_after_the_thread_is_opened(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    let other = run_shell(cx);
    let row = format!("thread-{}", id.0);
    let mark = format!("thread-mark-{}", id.0);
    wait_until(cx, |cx| shows(cx, window, &row));
    let asking = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Keep going?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
            }),
            cx,
        )
    });
    wait_for_mark(cx, window, &mark, "needs you");
    click(cx, window, &row);
    click(cx, window, &format!("thread-{}", other.0));
    assert_eq!(label(cx, window, &mark), "needs you");
    assert_eq!(value_of(cx, window, &row).as_deref(), Some("needs you"));
    let question = pending(cx, &id).into_iter().next().expect("question");
    click(cx, window, &row);
    click(cx, window, &format!("answer-{}-yes", question.id.0));
    cx.foreground_executor().block_test(asking).unwrap();
    wait_for_mark(cx, window, &mark, "idle");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_shown_thread_that_goes_idle_is_never_needs_you(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (_kagan, window, _data) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    let program = folder.path().join("agent");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf '\\033]2;\\342\\227\\220 work\\007'; sleep 1; printf '\\033]2;\\342\\234\\263 work\\007'; sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let thread = run_program(cx, &program.display().to_string());
    let mark = format!("thread-mark-{}", thread.0);
    click(cx, window, &format!("thread-{}", thread.0));
    let mut seen = wait_for_mark(cx, window, &mark, "busy");
    seen.extend(wait_for_mark(cx, window, &mark, "idle"));
    assert!(
        !seen.iter().any(|word| word == "needs you"),
        "a shown thread went idle without needing the person: {seen:?}"
    );
}

fn wait_for_mark(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    word: &str,
) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let text = label(cx, window, name);
        if seen.last() != Some(&text) {
            seen.push(text.clone());
        }
        if text == word {
            return seen;
        }
        assert!(Instant::now() < deadline, "{name} never read {word:?}");
        std::thread::yield_now();
    }
}

/// A plugin with a key command that opens another thread and a panel, and a probe command that
/// asks for a panel that does not exist and logs the answer it gets back.
const NAVIGATOR_SOURCE: &str = r#"import { View, div } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log, openPanel, openThread } from "kagan/host";
import { list } from "kagan/threads";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then(
          (value) => reply(job.id, value),
          (error) => fail(job.id, String(error)),
        );
      }
    });
  }
  render() { return ""; }
}
export class NotesPanel extends View {
  render() { return div().child("Notes"); }
}
async function handle(job) {
  if (job.kind !== "command") throw new Error(`Unknown job ${job.kind}`);
  if (job.command === "show") {
    const other = (await list({ limit: 20 })).find((thread) => thread.id !== job.threadId);
    if (!other) throw new Error("no other thread");
    await openThread(other.id);
    await openPanel("notes", { from: job.threadId });
    log("info", "show ran");
    return null;
  }
  if (job.command === "panel") {
    try {
      await openPanel("notes", {});
      log("info", "panel ran");
    } catch (error) {
      log("info", `panel refused: ${String(error)}`);
    }
    return null;
  }
  if (job.command === "project") {
    log("info", "project ran");
    return null;
  }
  if (job.command === "clash") {
    log("info", "clash ran");
    return null;
  }
  if (job.command === "chord") {
    log("info", "chord ran");
    return null;
  }
  if (job.command === "prefixed") {
    log("info", "prefixed ran");
    return null;
  }
  if (job.command === "probe") {
    try {
      await openPanel("missing", {});
      log("info", "probe opened");
    } catch (error) {
      log("info", `probe refused: ${String(error)}`);
    }
    return null;
  }
  throw new Error(`Unknown command ${job.command}`);
}
"#;

pub(super) fn install_navigator(data: &Path) {
    let root = data.join("plugins").join("navigator");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.navigator",
        "name": "Navigator",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Opens threads and panels.",
        "powers": { "areas": ["threads"] },
        "commands": [
            { "id": "show", "title": "Show", "key": "secondary-shift-y", "when": "thread" },
            { "id": "probe", "title": "Probe", "when": "always" },
            { "id": "panel", "title": "Panel", "when": "always" },
            { "id": "project", "title": "Project", "key": "secondary-shift-j", "when": "project" },
            { "id": "clash", "title": "Clash", "key": chord("NewThread"), "when": "always" },
            { "id": "chord", "title": "Chord", "key": format!("{} x", chord("NewThread")), "when": "always" },
            { "id": "prefixed", "title": "Prefixed", "key": "secondary-shift-k z", "when": "always" }
        ],
        "ui": { "panels": [
            { "id": "notes", "title": "Notes", "view": "NotesPanel", "placement": "left" }
        ] }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), NAVIGATOR_SOURCE).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.navigator".into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({
                "powers": { "areas": ["threads"] }, "downloads": {}, "folders": {}
            }),
            error: None,
        })
        .unwrap();
}

/// The plugin's log, as its own output reports it.
pub(super) fn plugin_log(cx: &mut TestAppContext, id: &str) -> String {
    let logs = run_op(
        cx,
        Op::Plugin(PluginOp::Logs {
            id: PluginId(id.into()),
            follow: false,
            after: None,
        }),
    );
    match logs {
        OpResult::Logs { text, .. } => text,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
fn a_plugin_key_command_opens_a_thread_and_a_panel_and_runs(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    let first = run_shell(cx);
    let _second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(first.clone())
    });
    open_surface(cx, &PluginId(NAVIGATOR.into()), "notes");
    wait_until(cx, |cx| {
        shows(cx, window, "panel-dev.kagan.navigator-notes-body")
    });
    click(cx, window, &format!("thread-{}", first.0));

    // The key is bound in the thread context, so it runs while a thread is on screen.
    cx.simulate_keystrokes(
        window,
        if cfg!(target_os = "macos") {
            "cmd-shift-y"
        } else {
            "ctrl-shift-y"
        },
    );
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx))
            .is_some_and(|shown| shown != first)
    });
    assert!(plugin_log(cx, "dev.kagan.navigator").contains("show ran"));
}

#[gpui_kit::test]
fn a_plugin_command_gets_the_answer_when_its_panel_is_missing(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let host = cx.update(|cx| kg_plugins::PluginHost::global(cx));
    let plugin = PluginId("dev.kagan.navigator".into());
    let task = cx.update(|cx| {
        kg_plugins::PluginHost::dispatch_command(&host, &plugin, "probe", None, None, cx)
    });
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
    assert!(
        plugin_log(cx, "dev.kagan.navigator").contains("probe refused"),
        "the plugin never got an answer"
    );
}

const PASTER_SOURCE: &str = r#"import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log, pasteIntoThread } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then(
          (value) => reply(job.id, value),
          (error) => fail(job.id, String(error)),
        );
      }
    });
  }
  render() { return ""; }
}
async function handle(job) {
  if (job.kind !== "command") throw new Error(`Unknown job ${job.kind}`);
  if (job.command === "paste") {
    await pasteIntoThread(job.threadId, "pasted-from-plugin");
    log("info", "paste ran");
    return null;
  }
  throw new Error(`Unknown command ${job.command}`);
}
"#;

fn install_paster(data: &Path) {
    let root = data.join("plugins").join("paster");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.paster",
        "name": "Paster",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Pastes into a thread.",
        "commands": [
            { "id": "paste", "title": "Paste", "when": "thread" }
        ]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), PASTER_SOURCE).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.paster".into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({
                "powers": {}, "downloads": {}, "folders": {}
            }),
            error: None,
        })
        .unwrap();
}

#[gpui_kit::test]
fn paste_into_thread_puts_text_in_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_paster);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(id.clone())
    });
    let host = cx.update(|cx| kg_plugins::PluginHost::global(cx));
    let plugin = PluginId("dev.kagan.paster".into());
    let task = cx.update(|cx| {
        kg_plugins::PluginHost::dispatch_command(
            &host,
            &plugin,
            "paste",
            Some(id.clone()),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| {
        grid_text(cx, &kagan).contains("pasted-from-plugin")
    });
    assert!(plugin_log(cx, "dev.kagan.paster").contains("paste ran"));
}

pub(super) const NAVIGATOR: &str = "dev.kagan.navigator";

/// The shortcut for `keys` on this platform: `shift-j` is cmd-shift-j on macOS, ctrl-shift-j elsewhere.
fn secondary(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{keys}")
    } else {
        format!("ctrl-{keys}")
    }
}

/// Runs one of the navigator's commands the way a call from outside a key does.
pub(super) fn run_navigator(cx: &mut TestAppContext, command: &str) {
    let host = cx.update(|cx| kg_plugins::PluginHost::global(cx));
    let plugin = PluginId(NAVIGATOR.into());
    let task = cx.update(|cx| {
        kg_plugins::PluginHost::dispatch_command(&host, &plugin, command, None, None, cx)
    });
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_plugin_key_that_a_window_shortcut_takes_is_not_bound(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    // The plugin's clash key is New Thread's default, so New Thread runs and the plugin is told why.
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    let log = plugin_log(cx, NAVIGATOR);
    assert!(!log.contains("clash ran"), "{log}");
    assert!(
        log.contains(&format!(
            "command clash is not bound to {}",
            kg_core::parse_key(&chord("NewThread"))
                .unwrap()
                .unparse(kg_core::host_os())
        )),
        "{log}"
    );
}

/// Windows reads a run-time key through the keyboard layout, unlike the window's own keys.
#[cfg(windows)]
#[gpui_kit::test]
fn on_windows_a_plugin_key_that_bigger_takes_is_not_bound(cx: &mut TestAppContext) {
    let plugin = "dev.kagan.grow";
    let (_kagan, _window, _data) = open_with_plugins_then(cx, |data| {
        let root = data.join("grow");
        std::fs::create_dir_all(&root).unwrap();
        let manifest = serde_json::json!({
            "id": plugin, "name": "Grow", "version": "1.0.0", "kagan": ">=0.1.0",
            "entry": "main.js", "instructions": "Clashes with Bigger.", "powers": {},
            "commands": [{ "id": "grow", "title": "Grow", "key": chord("Bigger"), "when": "always" }]
        });
        std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
        std::fs::write(
            root.join("main.js"),
            "import { View } from \"gpui-kit\";\nexport default class Plugin extends View { render() { return \"\"; } }\n",
        )
        .unwrap();
        kg_store::Store::open(data.join("kagan.sqlite3"))
            .unwrap()
            .insert_plugin(&Plugin {
                id: PluginId(plugin.into()),
                source: root.display().to_string(),
                root,
                commit: None,
                enabled: true,
                approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
                error: None,
            })
            .unwrap();
    });
    cx.run_until_parked();
    let log = plugin_log(cx, plugin);
    assert!(log.contains("command grow is not bound to"), "{log}");
    assert!(log.contains("takes that key"), "{log}");
}

#[gpui_kit::test]
fn plugin_keys_follow_a_reload_and_a_changed_window_shortcut(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let thread = run_shell(cx);
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(NAVIGATOR.into()),
            thread_id: Some(thread),
        }),
    );
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert!(
        !plugin_log(cx, NAVIGATOR).contains("clash ran"),
        "reload kept the window's key"
    );

    // New Thread moves onto the project key, and frees its old key for the plugin.
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Keymap(KeymapOp::Set {
                action: "NewThread".into(),
                key: secondary("shift-j"),
                reset: false,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &secondary("shift-j"));
    cx.run_until_parked();
    let log = plugin_log(cx, NAVIGATOR);
    assert!(!log.contains("project ran"), "{log}");
    assert!(
        log.contains(&format!(
            "command project is not bound to {}",
            secondary("shift-j")
        )),
        "{log}"
    );
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );

    // The key New Thread gave up is the plugin's now.
    cx.simulate_keystrokes(window, &chord("NewThread"));
    wait_until(cx, |cx| plugin_log(cx, NAVIGATOR).contains("clash ran"));
}

#[gpui_kit::test]
fn a_plugin_panel_opens_from_a_closed_sidebar_without_any_key(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    if cx.update(|cx| kagan.read(cx).left_open()) {
        click(cx, window, "left-sidebar-toggle");
    }
    wait_until(cx, |cx| !cx.update(|cx| kagan.read(cx).left_open()));
    assert!(!shows(cx, window, "panel-dev.kagan.navigator-notes-body"));

    run_navigator(cx, "panel");
    wait_until(cx, |cx| {
        shows(cx, window, "panel-dev.kagan.navigator-notes-body")
    });
    assert!(cx.update(|cx| kagan.read(cx).left_open()));
    assert!(plugin_log(cx, NAVIGATOR).contains("panel ran"));
}

#[gpui_kit::test]
fn a_plugin_panel_leaves_settings_and_is_on_screen_when_its_call_returns(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));

    run_navigator(cx, "panel");
    assert!(
        !shows(cx, window, "settings-page"),
        "settings still covers the panel"
    );
    assert!(shows(cx, window, "panel-dev.kagan.navigator-notes-body"));
    assert!(plugin_log(cx, NAVIGATOR).contains("panel ran"));
}

#[gpui_kit::test]
fn the_project_key_is_off_while_settings_covers_the_window(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let folder = data.path().join("plain");
    std::fs::create_dir_all(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx))) == Some(project.id.clone())
    });
    let runs = |cx: &mut TestAppContext| plugin_log(cx, NAVIGATOR).matches("project ran").count();

    cx.simulate_keystrokes(window, &secondary("shift-j"));
    wait_until(cx, |cx| runs(cx) == 1);

    click(cx, window, "settings-gear");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &secondary("shift-j"));
    cx.run_until_parked();
    assert_eq!(
        runs(cx),
        1,
        "the project key ran while settings covered the window"
    );

    click(cx, window, "settings-close");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &secondary("shift-j"));
    wait_until(cx, |cx| runs(cx) == 2);
}

#[gpui_kit::test]
fn a_plugin_sequence_that_starts_with_a_window_shortcut_is_not_bound(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let log = plugin_log(cx, NAVIGATOR);
    assert!(log.contains("command chord is not bound to"), "{log}");

    // The first key of the refused sequence is New Thread, which runs on its own.
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    assert!(!plugin_log(cx, NAVIGATOR).contains("chord ran"));
}

#[gpui_kit::test]
fn a_saved_shortcut_that_starts_a_plugin_sequence_takes_it_on_change(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    assert!(!plugin_log(cx, NAVIGATOR).contains("command prefixed is not bound"));

    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Keymap(KeymapOp::Set {
                action: "NewThread".into(),
                key: secondary("shift-k"),
                reset: false,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
    assert!(plugin_log(cx, NAVIGATOR).contains("command prefixed is not bound to"));

    cx.simulate_keystrokes(window, &secondary("shift-k"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    assert!(!plugin_log(cx, NAVIGATOR).contains("prefixed ran"));
}

#[gpui_kit::test]
fn a_saved_plugin_key_is_refused_where_the_window_refuses_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    cx.run_until_parked();
    let set = |cx: &mut TestAppContext, command: &str, key: &str| {
        run_op(
            cx,
            Op::Keymap(KeymapOp::Set {
                action: format!("{NAVIGATOR}:{command}"),
                key: key.into(),
                reset: false,
            }),
        )
    };
    // A key that starts with New Thread's key is refused.
    assert_eq!(
        set(cx, "panel", &format!("{} x", chord("NewThread"))),
        OpResult::Setting(serde_json::json!({ "ok": false, "conflict": "NewThread" }))
    );
    // A key that overlaps another plugin command is refused until that command's key is removed.
    assert_eq!(
        set(cx, "panel", "secondary-shift-k z"),
        OpResult::Setting(
            serde_json::json!({ "ok": false, "conflict": format!("{NAVIGATOR}:prefixed") })
        )
    );
    run_op(
        cx,
        Op::Keymap(KeymapOp::Remove {
            action: format!("{NAVIGATOR}:prefixed"),
        }),
    );
    assert_eq!(
        set(cx, "panel", "secondary-shift-k"),
        OpResult::Setting(serde_json::json!({ "ok": true }))
    );
    cx.run_until_parked();

    // The window follows the saved binding: the key runs panel, and prefixed lost its key.
    cx.simulate_keystrokes(window, &secondary("shift-k"));
    wait_until(cx, |cx| plugin_log(cx, NAVIGATOR).contains("\"panel ran\""));
    assert!(!plugin_log(cx, NAVIGATOR).contains("prefixed ran"));
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
fn two_changes(cx: &mut TestAppContext, window: AnyWindowHandle, repo: &Path) {
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

fn scope_filled(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> bool {
    let bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(id.to_owned().into()))
                .map(|element| element.bounds())
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

#[gpui_kit::test]
fn wrap_lines_fits_the_review_and_survives_reopening(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (thread, repo) = repo_thread(cx, window, data.path());
    let long = "x".repeat(800);
    std::fs::write(repo.join("notes.txt"), format!("one\n{long}\nlast\n")).unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-3"));
    assert_eq!(selected_of(cx, window, "review-wrap"), Some(false));
    let before = row_top(cx, window, "comment-New-3") - row_top(cx, window, "comment-New-2");
    click(cx, window, "review-wrap");
    wait_until(cx, |cx| {
        selected_of(cx, window, "review-wrap") == Some(true)
    });
    wait_until(cx, |cx| {
        row_top(cx, window, "comment-New-3") - row_top(cx, window, "comment-New-2") > before * 2.
    });
    assert_eq!(
        cx.update(|cx| review_read::review_horizontal_limit(kagan.read(cx))),
        Some(0.)
    );
    click(cx, window, "comment-New-2");
    wait_until(cx, |cx| shows(cx, window, "review-comment-input"));
    assert!(
        row_top(cx, window, "review-comment-input") > row_top(cx, window, "comment-New-2") + before
    );
    assert!(row_top(cx, window, "review-comment-input") < row_top(cx, window, "comment-New-3"));
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let _reopened = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    click(cx, window, &format!("thread-{}", thread.0));
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| {
        selected_of(cx, window, "review-wrap") == Some(true)
    });
    cx.simulate_keystrokes(window, &chord("ToggleWrap"));
    wait_until(cx, |cx| {
        selected_of(cx, window, "review-wrap") == Some(false)
    });
    let toggle =
        cx.update(|cx| Workspace::dispatch(Op::Window(kg_domain::WindowOp::ToggleReviewWrap), cx));
    cx.foreground_executor().block_test(toggle).unwrap();
    wait_until(cx, |cx| {
        selected_of(cx, window, "review-wrap") == Some(true)
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn comments_on_long_lines_stay_in_the_review(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_thread, repo) = repo_thread(cx, window, data.path());
    std::fs::write(
        repo.join("notes.txt"),
        format!("one\n{}\n", "x".repeat(200)),
    )
    .unwrap();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    assert_eq!(
        super::panels::disabled_of(cx, window, "review-discard"),
        Some(true)
    );
    click(cx, window, "comment-New-2");
    let (left, _) = super::sidebars::span(cx, window, "review").unwrap();
    let (right, _) = super::sidebars::span(cx, window, "review-file-notes.txt").unwrap();
    let width = right - left;
    let (x, input_width) = super::sidebars::span(cx, window, "review-comment-input").unwrap();
    assert!(
        x >= left && x + input_width <= left + width,
        "input {x}+{input_width}, review {left}+{width}"
    );
    cx.simulate_input(window, "A comment");
    click(cx, window, "review-add-comment");
    let (x, button_width) = super::sidebars::span(cx, window, "review-remove-0").unwrap();
    assert!(
        x >= left && x + button_width <= left + width,
        "remove {x}+{button_width}, review {left}+{width}"
    );
    send(
        cx,
        window,
        gpui_kit::ScrollEvent {
            position: point(px(left + 100.), px(160.)),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(-300.), px(0.))),
            modifiers: Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
            source: gpui_kit::ScrollSource::Pointer,
        }
        .to_platform_input(),
    );
    cx.run_until_parked();
    assert!(
        cx.update(|cx| review_read::review_horizontal_offset(kagan.read(cx)))
            .is_some_and(|offset| offset < 0.)
    );
    let (x, button_width) = super::sidebars::span(cx, window, "review-remove-0").unwrap();
    assert!(
        x >= left && x + button_width <= left + width,
        "scrolled remove {x}+{button_width}, review {left}+{width}"
    );
    assert_eq!(
        super::panels::disabled_of(cx, window, "review-discard"),
        Some(false)
    );
}

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
                .map(|element| element.bounds().origin.y)
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
        super::sidebars::span(cx, window, "toast-dismiss").expect("dismiss");
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
        super::sidebars::span(cx, window, "toast-message").expect("message");
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

fn disabled(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.disabled())
    })
    .unwrap()
}

fn commit_setup(repo: &Path, body: &str) {
    let path = repo.join(".kagan/setup");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    git(repo, &["add", "--", ".kagan/setup"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "setup",
        ],
    );
}

#[gpui_kit::test]
fn the_start_screen_names_setup_before_start(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    set_program(cx, window, &kagan, "");
    wait_until(cx, |cx| {
        label(cx, window, "setup-script").contains(".kagan/setup")
    });
    assert_eq!(disabled(cx, window, "start"), Some(true));
    assert!(shown(cx, &kagan).is_none());
    set_program(cx, window, &kagan, shell());
    wait_until(cx, |cx| disabled(cx, window, "start") == Some(false));
    assert!(label(cx, window, "setup-script").contains(".kagan/setup"));
}

#[gpui_kit::test]
fn the_project_folder_choice_does_not_name_setup(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    click(cx, window, "folder-project");
    cx.run_until_parked();
    assert!(!shows(cx, window, "setup-script"));
    click(cx, window, "folder-worktree");
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    let _ = kagan;
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_setup_link_is_named_and_not_offered_as_a_run(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let target = data.path().join("real-setup.sh");
    std::fs::write(&target, "exit 1\n").unwrap();
    std::fs::create_dir_all(repo.join(".kagan")).unwrap();
    std::os::unix::fs::symlink(&target, repo.join(".kagan/setup")).unwrap();
    git(&repo, &["add", "--", ".kagan/setup"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "link",
        ],
    );
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        let text = label(cx, window, "setup-script");
        text.contains(".kagan/setup") && text.contains("will not")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn retry_runs_a_failed_setup_again_without_a_second_worktree(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(
        &repo,
        "if [ -f .kagan/once ]; then exit 0; fi\ntouch .kagan/once\nexit 1\n",
    );
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| {
        label(cx, window, "setup-failure").contains(".kagan/setup")
    });
    assert_eq!(
        label(cx, window, "setup-retry"),
        "Retry setup in this worktree"
    );
    assert!(shown(cx, &kagan).is_none());
    assert_eq!(
        std::fs::read_dir(data.path().join("worktrees"))
            .unwrap()
            .count(),
        1
    );
    click(cx, window, "setup-retry");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    assert_eq!(
        std::fs::read_dir(data.path().join("worktrees"))
            .unwrap()
            .count(),
        1
    );
    let started = shown(cx, &kagan).unwrap();
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(&started))
            .is_some()
    );
}

#[gpui_kit::test]
fn a_deleted_setup_is_still_named_from_the_commit(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    std::fs::remove_file(repo.join(".kagan/setup")).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    set_program(cx, window, &kagan, "");
    wait_until(cx, |cx| {
        label(cx, window, "setup-script").contains(".kagan/setup")
    });
    assert_eq!(disabled(cx, window, "start"), Some(true));
}

#[gpui_kit::test]
fn an_uncommitted_setup_is_not_named(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let path = repo.join(".kagan/setup");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "exit 1\n").unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    cx.run_until_parked();
    assert!(!shows(cx, window, "setup-script"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_uncommitted_link_does_not_replace_the_named_script(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    std::fs::remove_file(repo.join(".kagan/setup")).unwrap();
    let target = data.path().join("other-setup.sh");
    std::fs::write(&target, "exit 1\n").unwrap();
    std::os::unix::fs::symlink(&target, repo.join(".kagan/setup")).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        let text = label(cx, window, "setup-script");
        text.contains("Runs .kagan/setup") && !text.contains("will not")
    });
}

#[gpui_kit::test]
fn a_moved_branch_runs_the_setup_the_screen_named(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "touch named-ran\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "setup-script").contains("Runs .kagan/setup")
    });
    let pinned = head_commit(&repo);
    commit_setup(&repo, "touch moved-ran\n");
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.status, EnvironmentStatus::Ready);
    assert_eq!(head_commit(&environment.path), pinned);
    assert!(environment.path.join("named-ran").is_file());
    assert!(!environment.path.join("moved-ran").exists());
}

#[gpui_kit::test]
fn the_start_screen_can_show_the_pinned_setup_script(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "echo from-commit\n");
    std::fs::write(repo.join(".kagan/setup"), "echo from-worktree\n").unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-disclose"));
    assert_eq!(label(cx, window, "setup-disclose"), "Show script");
    assert!(!shows(cx, window, "setup-body"));
    click(cx, window, "setup-disclose");
    wait_until(cx, |cx| shows(cx, window, "setup-body"));
    let script = label(cx, window, "setup-body");
    assert!(script.contains("echo from-commit"), "{script}");
    assert!(!script.contains("from-worktree"), "{script}");
    assert_eq!(label(cx, window, "setup-disclose"), "Hide script");
}

#[gpui_kit::test]
fn a_long_setup_script_is_capped(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let mut body = String::new();
    for n in 0..45 {
        writeln!(body, "echo line-{n}").unwrap();
    }
    commit_setup(&repo, &body);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-disclose"));
    click(cx, window, "setup-disclose");
    wait_until(cx, |cx| shows(cx, window, "setup-body"));
    let script = label(cx, window, "setup-body");
    assert!(script.contains("echo line-0"), "{script}");
    assert!(script.contains("echo line-39"), "{script}");
    assert!(!script.contains("echo line-40"), "{script}");
    assert_eq!(
        label(cx, window, "setup-truncated"),
        "The rest of the script is not shown."
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_changed_script_hides_retry_and_leaves_start(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 1\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shows(cx, window, "setup-retry"));
    let old = only_worktree(data.path());
    std::fs::write(old.join(".kagan/setup"), "echo edited\n").unwrap();
    click(cx, window, "setup-retry");
    wait_until(cx, |cx| {
        label(cx, window, "setup-failure").contains("Start will use the version shown")
    });
    let failure = label(cx, window, "setup-failure");
    assert!(failure.contains(".kagan/setup changed"), "{failure}");
    assert!(!shows(cx, window, "setup-retry"));
    assert!(shows(cx, window, "start"));
    assert!(!old.exists());
    assert!(worktree_dirs(data.path()).is_empty());
    click(cx, window, "start");
    wait_until(cx, |cx| shows(cx, window, "setup-retry"));
    let fresh = only_worktree(data.path());
    assert_eq!(fresh, old);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_blocked_removal_is_shown_and_can_be_tried_again(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "touch kept-marker\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let thread = shown(cx, &kagan).unwrap();
    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| {
        thread_record(cx, &thread).status == ThreadStatus::Idle
    });
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &thread).archived_at.is_some());
    let mut environment = environment_of(cx, &thread);
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        environment = environment_of(cx, &thread);
        if environment.status == EnvironmentStatus::Error {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let reason = environment.error.as_deref().unwrap_or("");
    assert!(reason.contains("kept-marker"), "{reason}");
    assert!(reason.contains("git-ignore"), "{reason}");
    click(cx, window, "archived");
    let removal = format!("removal-{}", thread.0);
    wait_until(cx, |cx| label(cx, window, &removal).contains("kept-marker"));
    assert!(label(cx, window, &removal).contains("remove the worktree again"));
    std::fs::remove_file(environment.path.join("kept-marker")).unwrap();
    click(cx, window, &format!("remove-again-{}", environment.id.0));
    for _ in 0..40 {
        cx.run_until_parked();
        environment = environment_of(cx, &thread);
        if !environment.path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!environment.path.exists(), "{environment:?}");
    assert!(environment.error.is_none(), "{environment:?}");
    cx.run_until_parked();
    assert!(!shows(cx, window, &removal));
}

fn worktree_dirs(data: &Path) -> Vec<std::path::PathBuf> {
    let Ok(read) = std::fs::read_dir(data.join("worktrees")) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = read
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| std::fs::read_dir(entry.path()).ok())
        .flat_map(std::iter::Iterator::flatten)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    paths
}

fn only_worktree(data: &Path) -> std::path::PathBuf {
    let paths = worktree_dirs(data);
    assert_eq!(paths.len(), 1, "{paths:?}");
    paths.into_iter().next().unwrap()
}

fn right_click(cx: &mut TestAppContext, window: AnyWindowHandle, row: &str) {
    let at = cx
        .update_window(window, |_, window, _| {
            let bounds = crate::ui_registry::find(window, &ElementId::Name(row.to_owned().into()))
                .unwrap()
                .bounds();
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            )
        })
        .unwrap();
    send(
        cx,
        window,
        MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
    );
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
    send(
        cx,
        window,
        MouseDownEvent {
            button: MouseButton::Right,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
    );
    send(
        cx,
        window,
        MouseUpEvent {
            button: MouseButton::Right,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
        }
        .to_platform_input(),
    );
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn project_menu_asks_and_removal_updates_the_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("project-to-remove");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    let row = format!("project-{}", project.id);
    wait_until(cx, |cx| shows(cx, window, &row));
    right_click(cx, window, &row);
    wait_until(cx, |cx| shows(cx, window, "project-remove"));
    click(cx, window, "project-remove");
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id()
            .clone()
    });
    let cancelled = cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx));
    cx.foreground_executor().block_test(cancelled).unwrap();
    wait_until(cx, |cx| approvals_waiting(cx) == 0);
    assert!(shows(cx, window, &row));
    let confirmed = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Remove {
                id: project.id.clone(),
            }),
            cx,
        )
    });
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id()
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(confirmed).unwrap();
    wait_until(cx, |cx| !shows(cx, window, &row));
    assert!(folder.exists());
}

#[gpui_kit::test]
fn the_personal_project_has_no_remove_menu(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("standard-project");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    let row = format!("project-{}", project.id);
    wait_until(cx, |cx| shows(cx, window, &row));
    right_click(cx, window, "project-personal");
    cx.run_until_parked();
    assert!(!shows(cx, window, "project-remove"));
    right_click(cx, window, &row);
    wait_until(cx, |cx| shows(cx, window, "project-remove"));
}

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
    assert_eq!(asked.options()[0].label, "Cancel");
    assert_eq!(asked.options()[1].label, "Remove");
    let cancel = format!("answer-{}-decline", asked.id());
    let remove = format!("answer-{}-approve", asked.id());
    set_preference(cx, "main.right-sidebar", serde_json::json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    assert_eq!(focused_of(cx, window, &cancel), Some(true));
    assert_ne!(focused_of(cx, window, &remove), Some(true));
    let answered =
        cx.update(|cx| Workspace::answer_standalone(asked.id().clone(), "decline".into(), cx));
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
        super::panels::disabled_of(cx, window, "review-send"),
        Some(true)
    );
    click(cx, window, "review-add-comment");
    assert!(!shows(cx, window, "review-add-comment"));
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        1
    );
    assert_eq!(
        super::panels::disabled_of(cx, window, "review-send"),
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
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("directly"));
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
        super::panels::disabled_of(cx, window, "review-send"),
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
    let (x, width) = super::sidebars::span(cx, window, "review-comment-input").expect("the box");
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
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("keep this draft"));
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
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("ready"));
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
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("end"));
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
        let (x, width) = super::sidebars::span(cx, window, id).expect(id);
        assert!(x >= 0. && x + width <= 789.5, "{id}: {x}+{width}");
    }
    let (_, width) = super::sidebars::span(cx, window, "review-title").expect("title");
    assert!(width > 40., "title width {width}");
    cx.simulate_window_resize(window, size(px(560.), px(720.)));
    cx.run_until_parked();
    for id in ["review-close", "review-find", "review-viewed"] {
        let (x, width) = super::sidebars::span(cx, window, id).expect(id);
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
        let (x, width) = super::sidebars::span(cx, window, id).expect(id);
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
                .bounds()
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
                .bounds()
        })
        .unwrap();
    let child_bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(child_row.clone().into()))
                .unwrap()
                .bounds()
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
