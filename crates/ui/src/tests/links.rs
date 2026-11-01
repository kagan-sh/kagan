use gpui_kit::{Modifiers, TestAppContext};

use super::kagan::{
    cell_point, click, grid_text, label, open, present, press, release, run_shell, shown_row,
    wait_until,
};
use crate::kagan::changes::POLL;
use crate::kagan::{NewTerminal, OpenLocalLink};

#[gpui_kit::test]
fn a_secondary_click_on_a_printed_link_opens_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_input(window, "printf 'at http%s now\\n' '://localhost:4321/x'\n");
    wait_until(cx, |cx| {
        grid_text(cx, &kagan).contains("http://localhost:4321/x")
    });
    let (row, column) = shown_row(cx, &kagan, "http://localhost:4321/x");
    let at = cell_point(cx, window, &kagan, row, column + 3);

    press(cx, window, at, 1, Modifiers::none());
    release(cx, window, at, 1, Modifiers::none());
    assert_eq!(cx.opened_url(), None);

    let secondary = cx.update(|cx| Modifiers::secondary_key(cx));
    press(cx, window, at, 1, secondary);
    release(cx, window, at, 1, secondary);
    assert_eq!(cx.opened_url().as_deref(), Some("http://localhost:4321/x"));
}

#[gpui_kit::test]
fn the_row_over_the_terminal_offers_the_newest_local_link(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(!present(cx, window, "open-local-link"));
    cx.simulate_input(
        window,
        "printf 'Local: http%s\\nDocs: https://example.com\\n' '://localhost:3000/'\n",
    );
    wait_until(cx, |cx| {
        cx.executor().advance_clock(POLL);
        present(cx, window, "open-local-link")
    });
    assert_eq!(label(cx, window, "open-local-link"), "Open localhost:3000");
    click(cx, window, "open-local-link");
    assert_eq!(cx.opened_url().as_deref(), Some("http://localhost:3000/"));

    cx.dispatch_action(window, NewTerminal);
    wait_until(cx, |cx| present(cx, window, "terminal-tab-1"));
    cx.simulate_input(window, "printf 'http%s\\n' '://127.0.0.1:5173'\n");
    wait_until(cx, |cx| {
        cx.executor().advance_clock(POLL);
        label(cx, window, "open-local-link") == "Open 127.0.0.1:5173"
    });
    cx.dispatch_action(window, OpenLocalLink);
    cx.run_until_parked();
    assert_eq!(cx.opened_url().as_deref(), Some("http://127.0.0.1:5173"));
}
