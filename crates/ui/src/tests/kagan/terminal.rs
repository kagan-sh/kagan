use crate::kagan::screen_read;
use std::time::Duration;

use gpui_kit::{
    Action, AnyWindowHandle, AppContext as _, ClipboardItem, ElementId, Entity, NoAction,
    TestAppContext, px,
};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, point,
};
use kg_core::Workspace;
use kg_domain::{KeyPlatform, Keymap, Op, ThreadOp};

use crate::kagan::{
    ActualSize, AddProject, ArchiveThread, Bigger, FocusSidebar, Kagan, NewThread, OpenSettings,
    RestoreThread, RetryBranches, RetryPrograms, Smaller, StartAgain, StopThread, ThreadActions,
    ToggleCustomCommand, TogglePinned, ToggleRightSidebar, ToggleSidebar, UseProjectFolder,
    UseWorktreeFolder, chord_for, is_chord,
};

use super::{click, grid_text, input, open, run_shell, shows, wait_for_output, wait_for_screen};

#[gpui_kit::test]
fn a_program_that_ends_keeps_its_last_output_on_screen(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&id, &line(&split_command("ke", "yed"))), cx).detach();
    });
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("keyed"));

    cx.dispatch_action(window, StopThread);
    wait_for_screen(cx, &kagan, |cx| !shows(cx, window, "stop"));
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
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("shown-out"));
}

#[gpui_kit::test]
fn a_key_reaches_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    // The echo shows the split command, so "keyed" can only appear once Enter runs it.
    cx.simulate_input(window, &split_command("ke", "yed"));
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains(&split_command("ke", "yed"))
    });
    assert!(!grid_text(cx, &kagan).contains("keyed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("keyed"));
}

#[gpui_kit::test]
fn paste_sends_the_clipboard_to_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    cx.write_to_clipboard(ClipboardItem::new_string("pasted-words".into()));
    cx.dispatch_action(window, crate::terminal::Paste);
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains("pasted-words")
    });
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
    wait_for_output(cx, &kagan, |cx| grid_text(cx, &kagan).contains("row200"));

    cx.simulate_keystrokes(window, "shift-pageup");
    wait_for_screen(cx, &kagan, |cx| !grid_text(cx, &kagan).contains("row200"));
    assert!(!grid_text(cx, &kagan).contains("5;2~"));
}

#[gpui_kit::test]
fn the_copy_key_copies_the_selected_word(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));

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
    wait_for_screen(cx, &kagan, |cx| {
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
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));

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
    wait_for_screen(cx, &kagan, |cx| {
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
    wait_for_output(cx, &kagan, |cx| {
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
    wait_for_screen(cx, &kagan, |cx| !selection(cx, &kagan).is_empty());
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
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));

    // The character at column 2 of "picked" is "c".
    let (row, _) = shown_row(cx, &kagan, "picked");
    let left = cell_point_at(cx, window, &kagan, row, 2, 0.2);
    press(cx, window, left, 1, Modifiers::default());
    let right = cell_point_at(cx, window, &kagan, row, 2, 0.8);
    drag_to(cx, window, right);
    release(cx, window, right, 1, Modifiers::default());
    wait_for_screen(cx, &kagan, |cx| !selection(cx, &kagan).is_empty());
    assert_eq!(selection(cx, &kagan), vec![(row, 2..3)]);
}

#[gpui_kit::test]
fn the_release_is_where_a_selection_ends(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));

    let (row, _) = shown_row(cx, &kagan, "picked");
    let start = cell_point_at(cx, window, &kagan, row, 0, 0.3);
    press(cx, window, start, 1, Modifiers::default());
    let middle = cell_point(cx, window, &kagan, row, 2);
    drag_to(cx, window, middle);
    // The last move is to column 2; the release is in the right half of column 4, so the
    // selection ends with that character.
    let end = cell_point_at(cx, window, &kagan, row, 4, 0.8);
    release(cx, window, end, 1, Modifiers::default());
    wait_for_screen(cx, &kagan, |cx| selection(cx, &kagan) == vec![(row, 0..5)]);
}

#[gpui_kit::test]
fn a_release_outside_the_terminal_ends_the_selection(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("one two", " three"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains("one two three")
    });

    let (row, _) = shown_row(cx, &kagan, "one two three");
    let start = cell_point(cx, window, &kagan, row, 0);
    press(cx, window, start, 1, Modifiers::default());
    let end = cell_point(cx, window, &kagan, row, 4);
    drag_to(cx, window, end);
    wait_for_screen(cx, &kagan, |cx| !selection(cx, &kagan).is_empty());
    let before = selection(cx, &kagan);

    // The release is lost, so the pointer comes back with no button down.
    hover_to(cx, window, point(px(2.), px(2.)));
    let back = cell_point(cx, window, &kagan, row, 10);
    hover_to(cx, window, back);
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(selection(cx, &kagan), before);
}

pub(in crate::tests) fn selection(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
) -> Vec<(usize, std::ops::Range<usize>)> {
    cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
        .map_or_else(Vec::new, |grid| grid.selection)
}

/// The row and column of the first line that is exactly `text`, on the shown terminal.
pub(in crate::tests) fn shown_row(
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
pub(in crate::tests) fn cell_point(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
    row: usize,
    column: usize,
) -> Point<Pixels> {
    cell_point_at(cx, window, kagan, row, column, 0.5)
}

/// A pixel across a cell, `across` of the way from its left edge to its right edge.
pub(in crate::tests) fn cell_point_at(
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
                .bounds
        })
        .unwrap();
    point(
        bounds.origin.x + bounds.size.width * ((column + across) / cols),
        bounds.origin.y + bounds.size.height * ((row + 0.5) / rows),
    )
}

pub(in crate::tests) fn send(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    event: gpui_kit::PlatformInput,
) {
    cx.update_window(window, |_, window, cx| window.dispatch_event(event, cx))
        .unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn press(
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

pub(in crate::tests) fn release(
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

pub(in crate::tests) fn drag_to(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    at: Point<Pixels>,
) {
    let event = MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    };
    send(cx, window, event.to_platform_input());
}

pub(in crate::tests) fn hover_to(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    at: Point<Pixels>,
) {
    let event = MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::default(),
    };
    send(cx, window, event.to_platform_input());
}

/// The program every test thread runs: a shell on each platform.
pub(in crate::tests) fn shell() -> &'static str {
    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

/// One line as typed into that shell.
pub(in crate::tests) fn line(command: &str) -> String {
    if cfg!(windows) {
        format!("{command}\r\n")
    } else {
        format!("{command}\n")
    }
}

/// An echo command as typed. Its output is `left` + `right`, but the typed text is not, so
/// output can be told from the echo of the command.
pub(in crate::tests) fn split_command(left: &str, right: &str) -> String {
    if cfg!(windows) {
        format!("echo {left}^{right}")
    } else {
        format!("echo {left}''{right}")
    }
}

#[gpui_kit::test]
fn copy_on_select_copies_a_drag_only_while_the_switch_is_on(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));
    cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into())));

    let drag = |cx: &mut TestAppContext| {
        let (row, _) = shown_row(cx, &kagan, "picked");
        let start = cell_point_at(cx, window, &kagan, row, 0, 0.3);
        press(cx, window, start, 1, Modifiers::default());
        let middle = cell_point(cx, window, &kagan, row, 2);
        drag_to(cx, window, middle);
        let end = cell_point_at(cx, window, &kagan, row, 4, 0.8);
        release(cx, window, end, 1, Modifiers::default());
    };
    let clipboard = |cx: &mut TestAppContext| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
    };

    drag(cx);
    wait_for_screen(cx, &kagan, |cx| clipboard(cx).as_deref() == Some("picke"));

    let off = Op::Preference(kg_domain::PreferenceOp::Set {
        key: kg_domain::prefs::TERMINAL_COPY_ON_SELECT.into(),
        value: serde_json::json!(false),
    });
    let task = cx.update(|cx| kg_core::Workspace::dispatch(off, cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
    cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into())));
    drag(cx);
    wait_for_screen(cx, &kagan, |cx| selection(cx, &kagan).len() == 1);
    cx.run_until_parked();
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("before"),
        "the switch is off"
    );

    let on = Op::Preference(kg_domain::PreferenceOp::Set {
        key: kg_domain::prefs::TERMINAL_COPY_ON_SELECT.into(),
        value: serde_json::json!(true),
    });
    let task = cx.update(|cx| kg_core::Workspace::dispatch(on, cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
    drag(cx);
    wait_for_screen(cx, &kagan, |cx| clipboard(cx).as_deref() == Some("picke"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn copy_on_select_skips_a_reported_release_and_copies_a_shift_drag(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));
    cx.simulate_input(window, "printf '\\033[?1000h\\033[?1006h'");
    cx.simulate_keystrokes(window, "enter");
    wait_for_output(cx, &kagan, |cx| {
        cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))
            .is_some_and(|grid| grid.mouse.is_some())
    });
    cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into())));

    let drag = |cx: &mut TestAppContext, modifiers: Modifiers| {
        let (row, _) = shown_row(cx, &kagan, "picked");
        let start = cell_point_at(cx, window, &kagan, row, 0, 0.3);
        press(cx, window, start, 1, modifiers);
        let middle = cell_point(cx, window, &kagan, row, 2);
        drag_to(cx, window, middle);
        let end = cell_point_at(cx, window, &kagan, row, 4, 0.8);
        release(cx, window, end, 1, modifiers);
    };
    let clipboard = |cx: &mut TestAppContext| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
    };

    // The program asked for the mouse, so this release is its own.
    drag(cx, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(selection(cx, &kagan).is_empty(), "a reported drag selected");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("before"),
        "a reported release copied"
    );

    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    drag(cx, shift);
    wait_for_screen(cx, &kagan, |cx| clipboard(cx).as_deref() == Some("picke"));
}

#[gpui_kit::test]
fn a_plain_click_with_copy_on_leaves_the_clipboard_alone(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));
    cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into())));

    let (row, _) = shown_row(cx, &kagan, "picked");
    let at = cell_point(cx, window, &kagan, row, 2);
    press(cx, window, at, 1, Modifiers::default());
    release(cx, window, at, 1, Modifiers::default());
    cx.run_until_parked();
    let clipboard = cx
        .update(|cx| cx.read_from_clipboard())
        .and_then(|item| item.text());
    assert_eq!(clipboard.as_deref(), Some("before"), "a plain click copied");
}

#[gpui_kit::test]
fn a_double_click_with_copy_on_copies_the_word(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, &split_command("pick", "ed"));
    cx.simulate_keystrokes(window, "enter");
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("picked"));
    cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into())));
    let (row, _) = shown_row(cx, &kagan, "picked");
    let at = cell_point(cx, window, &kagan, row, 2);
    press(cx, window, at, 1, Modifiers::default());
    release(cx, window, at, 1, Modifiers::default());
    press(cx, window, at, 2, Modifiers::default());
    release(cx, window, at, 2, Modifiers::default());
    wait_for_screen(cx, &kagan, |cx| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .is_some_and(|text| text == "picked")
    });
}

// The rows the terminal wrapped at the edge (a prompt that fills a row) must read as one line, or
// a word typed across the edge is missed.
#[test]
fn a_row_the_terminal_wrapped_reads_as_one_line() {
    use kg_core::{Cell, Color, Grid, Weight};
    let row = |text: &str| {
        text.chars()
            .map(|c| Cell {
                c,
                zerowidth: Vec::new(),
                fg: Color::Foreground,
                bg: Color::Background,
                weight: Weight::Normal,
                italic: false,
                underline: false,
                strikeout: false,
            })
            .collect::<Vec<_>>()
    };
    let grid = Grid {
        lines: vec![row("C:\\personal\\cmd>pa"), row("sted-words")],
        wrapped: vec![true, false],
        ..Grid::default()
    };
    assert!(super::screen_text(&grid).contains("pasted-words"));
}
