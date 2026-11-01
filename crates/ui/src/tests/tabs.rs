use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, TerminalOp, ThreadId, ThreadOp};

use super::kagan::{click, open, present, run_shell, wait_until};
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

fn tab_text(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> String {
    cx.update(|cx| tab_grid(kagan.read(cx), cx))
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
    cx.update(|cx| (tab_numbers(kagan.read(cx)), shown_tab(kagan.read(cx))))
}

#[gpui_kit::test]
fn the_plus_opens_a_terminal_tab_that_takes_the_keys(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, _) = shown(cx, window);
    assert!(!present(cx, window, "terminal-tabs"));

    click(cx, window, "new-terminal");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    assert!(present(cx, window, "terminal-tab-agent"));
    assert!(present(cx, window, "terminal-tab-1"));
    assert!(
        cx.update_window(window, |_, window, cx| tab_focused(
            kagan.read(cx),
            window,
            cx
        ))
        .unwrap()
    );

    cx.simulate_input(window, "printf 'ex%s\\n' tra\n");
    wait_until(cx, |cx| tab_text(cx, &kagan).contains("extra"));

    click(cx, window, "terminal-tab-agent");
    assert_eq!(tabs(cx, &kagan).1, None);
    assert!(!tab_text(cx, &kagan).contains("extra"));
}

#[gpui_kit::test]
fn keys_switch_tabs_and_close_the_shown_one(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, _) = shown(cx, window);
    cx.dispatch_action(window, NewTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    cx.dispatch_action(window, NewTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1, 2], Some(2)));

    cx.dispatch_action(window, NextTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan).1.is_none());
    cx.dispatch_action(window, PreviousTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan).1 == Some(2));

    cx.dispatch_action(window, CloseTerminal);
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    click(cx, window, "terminal-close-1");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![], None));
    assert!(!present(cx, window, "terminal-tabs"));
    assert!(
        cx.update_window(window, |_, window, cx| tab_focused(
            kagan.read(cx),
            window,
            cx
        ))
        .unwrap()
    );
}

#[gpui_kit::test]
fn a_shell_that_exits_takes_its_tab_away(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, id) = shown(cx, window);
    click(cx, window, "new-terminal");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], Some(1)));
    cx.simulate_input(window, "exit\n");
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![], None));
    assert!(cx.update(|cx| Workspace::global(cx).read(cx).extras(&id).is_empty()));
}

#[gpui_kit::test]
fn a_terminal_opened_with_kg_shows_as_a_tab_without_taking_the_view(cx: &mut TestAppContext) {
    let (_, window, _data) = open(cx);
    let (kagan, id) = shown(cx, window);
    let task = cx.update(|cx| {
        Workspace::dispatch(Op::Thread(ThreadOp::Terminal(TerminalOp::New { id })), cx)
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| tabs(cx, &kagan) == (vec![1], None));
    assert!(present(cx, window, "terminal-tab-1"));
}
