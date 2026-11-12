use super::chord;
use crate::kagan::review_read;

#[cfg(unix)]
use gpui_kit::px;
use gpui_kit::{AppContext as _, TestAppContext};
#[cfg(unix)]
use gpui_kit::{InputEvent as _, Modifiers, point};
use kg_core::Workspace;
use kg_domain::Op;

use crate::kagan::changes::POLL;

#[cfg(unix)]
use super::send;
use super::{click, open, repo_thread, row_top, selected_of, shows, wait_until};

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
        crate::tests::plugin::panels::disabled_of(cx, window, "review-discard"),
        Some(true)
    );
    click(cx, window, "comment-New-2");
    let (left, _) = crate::tests::sidebar::layout::span(cx, window, "review").unwrap();
    let (right, _) =
        crate::tests::sidebar::layout::span(cx, window, "review-file-notes.txt").unwrap();
    let width = right - left;
    let (x, input_width) =
        crate::tests::sidebar::layout::span(cx, window, "review-comment-input").unwrap();
    assert!(
        x >= left && x + input_width <= left + width,
        "input {x}+{input_width}, review {left}+{width}"
    );
    cx.simulate_input(window, "A comment");
    click(cx, window, "review-add-comment");
    let (x, button_width) =
        crate::tests::sidebar::layout::span(cx, window, "review-remove-0").unwrap();
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
    let (x, button_width) =
        crate::tests::sidebar::layout::span(cx, window, "review-remove-0").unwrap();
    assert!(
        x >= left && x + button_width <= left + width,
        "scrolled remove {x}+{button_width}, review {left}+{width}"
    );
    assert_eq!(
        crate::tests::plugin::panels::disabled_of(cx, window, "review-discard"),
        Some(false)
    );
}
