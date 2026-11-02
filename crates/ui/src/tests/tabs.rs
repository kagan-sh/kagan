use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, TerminalOp, ThreadId, ThreadOp};

#[cfg(not(target_os = "macos"))]
use super::kagan::label;
use super::kagan::{click, focused_of, open, present, run_shell, selected_of, shows, wait_until};
use crate::kagan::screen_read::{shown_tab, tab_focused, tab_grid, tab_numbers};
use crate::kagan::{CloseTerminal, Kagan, NewTerminal, NextTerminal, PreviousTerminal};

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

fn shown(cx: &mut TestAppContext, window: AnyWindowHandle) -> (Entity<Kagan>, ThreadId) {
    with_sh(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let kagan = cx
        .update(|cx| crate::kagan::front(cx).map(|(_, kagan)| kagan))
        .unwrap();
    (kagan, id)
}

fn add(cx: &mut TestAppContext, window: AnyWindowHandle, item: &str) {
    click(cx, window, "right-tab-add");
    click(cx, window, item);
}

fn tab_text(cx: &mut TestAppContext, kagan: &Entity<Kagan>, number: u32) -> String {
    cx.update(|cx| tab_grid(kagan.read(cx), number, cx))
        .map(|grid| {
            grid.lines
                .iter()
                .map(|line| line.iter().map(|cell| cell.c).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn tabs(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> (Vec<u32>, Option<u32>) {
    cx.update(|cx| (tab_numbers(kagan.read(cx)), shown_tab(kagan.read(cx), cx)))
}

fn has_keys(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
    number: u32,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        tab_focused(kagan.read(cx), number, window, cx)
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_plus_opens_a_right_terminal_that_takes_the_keys(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, _) = shown(cx, window);
    assert!(!present(cx, window, "new-terminal"));

    add(cx, window, "right-add-terminal");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    assert_eq!(selected_of(cx, window, "right-tab-terminal-1"), Some(true));
    assert!(has_keys(cx, window, &kagan, 1));

    cx.simulate_input(window, "printf 'ex%s\\n' tra\n");
    wait_until(cx, |cx| tab_text(cx, &kagan, 1).contains("extra"));

    click(cx, window, "changes-header");
    assert_eq!(tabs(cx, &kagan).1, None);
    assert!(shows(cx, window, "center"));
}

#[gpui_kit::test]
fn keys_step_through_terminals_and_closing_one_keeps_the_keyboard_right(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, _) = shown(cx, window);
    cx.dispatch_action(window, NewTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    cx.dispatch_action(window, NewTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1, 2], Some(2)));

    cx.dispatch_action(window, PreviousTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan).1 == Some(1));
    assert!(has_keys(cx, window, &kagan, 1));
    cx.dispatch_action(window, NextTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan).1 == Some(2));

    cx.dispatch_action(window, CloseTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], None));
    assert_eq!(focused_of(cx, window, "right-tab-add"), Some(true));

    click(cx, window, "right-tab-close-terminal-1");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![], None));
    assert!(!present(cx, window, "right-tab-terminal-1"));
    assert_eq!(focused_of(cx, window, "right-tab-add"), Some(true));
}

#[gpui_kit::test]
fn a_shell_that_exits_takes_its_right_tab_away(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, id) = shown(cx, window);
    add(cx, window, "right-add-terminal");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    cx.simulate_input(window, "exit\n");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![], None));
    assert!(cx.update(|cx| Workspace::global(cx).read(cx).extras(&id).is_empty()));
    assert_eq!(focused_of(cx, window, "right-tab-add"), Some(true));
}

#[gpui_kit::test]
fn a_terminal_opened_with_kg_shows_as_a_right_tab_without_taking_the_view(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, id) = shown(cx, window);
    let task = cx.update(|cx| {
        Workspace::dispatch(Op::Thread(ThreadOp::Terminal(TerminalOp::New { id })), cx)
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], None));
    assert!(present(cx, window, "right-tab-terminal-1"));
}

#[gpui_kit::test]
fn a_new_terminal_opens_a_closed_right_sidebar(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, _) = shown(cx, window);
    click(cx, window, "right-sidebar-toggle");
    wait_until(cx, |cx| !shows(cx, window, "right-sidebar"));
    cx.dispatch_action(window, NewTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    assert!(shows(cx, window, "right-tab-terminal-1"));
    assert!(has_keys(cx, window, &kagan, 1));
}

#[gpui_kit::test]
fn the_browser_opens_from_the_plus_and_closes(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    shown(cx, window);
    add(cx, window, "right-add-browser");
    assert_eq!(selected_of(cx, window, "right-tab-browser"), Some(true));
    assert!(shows(cx, window, "browser"));
    click(cx, window, "right-tab-add");
    assert!(!present(cx, window, "right-add-browser"));
    assert!(present(cx, window, "right-add-terminal"));
    cx.dispatch_action(window, gpui_kit::base::actions::Cancel);
    cx.run_until_parked();

    click(cx, window, "right-tab-close-browser");
    assert!(!present(cx, window, "right-tab-browser"));
    assert!(!present(cx, window, "browser"));
    assert_eq!(focused_of(cx, window, "right-tab-add"), Some(true));
}

#[cfg(not(target_os = "macos"))]
#[gpui_kit::test]
fn off_macos_the_browser_shows_a_note(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    shown(cx, window);
    add(cx, window, "right-add-browser");
    assert_eq!(
        label(cx, window, "browser-hint"),
        "The browser works on macOS only for now."
    );
}
