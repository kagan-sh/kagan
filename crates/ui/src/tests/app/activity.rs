use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
use kg_core::{Target, Updater, Workspace};
use kg_domain::{InteractionOption, Op, ThreadId, ThreadOp, UpdateState};

use crate::kagan::screen_read;
use crate::tests::kagan::{
    click, focused_of, open, run_shell, selected_of, shown, shows, value_of, wait_until,
};

#[cfg(unix)]
use crate::tests::kagan::run_program;

fn height_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map(|element| element.bounds.size.height.as_f32())
            .unwrap_or_default()
    })
    .unwrap()
}

fn mark_read(cx: &mut TestAppContext, id: &ThreadId) {
    let task =
        cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

fn raise(cx: &mut TestAppContext, id: &ThreadId, title: &str) -> gpui_kit::Task<TestAnswer> {
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let asked = cx.update(|cx| Workspace::ask(id, title.into(), options, cx));
    cx.run_until_parked();
    asked
}

type TestAnswer = std::result::Result<String, kg_domain::OpError>;

#[cfg(unix)]
fn busy_program() -> String {
    "/bin/sh -c \"printf '\\033]0;\\342\\240\\213 working\\007'; sleep 30\"".into()
}

fn offer_update(cx: &mut TestAppContext, dir: &std::path::Path) {
    cx.update(|cx| {
        Updater::global(cx).update(cx, |updater, cx| {
            updater.use_state(
                UpdateState::Available {
                    version: "99.0.0".into(),
                    notes: String::new(),
                },
                Target::find("macos", &dir.join("kagan"), None, None, &[]),
                cx,
            );
        });
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_strip_starts_closed_and_quiet(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(shows(cx, window, "activity-strip"));
    assert_eq!(
        value_of(cx, window, "activity-summary").as_deref(),
        Some("Quiet")
    );
    assert_eq!(selected_of(cx, window, "activity-summary"), Some(false));
    assert!(!shows(cx, window, "activity-log"));
    assert!(!shows(cx, window, "activity-needs"));
    assert!(!shows(cx, window, "activity-update"));
    let height = height_of(cx, window, "activity-strip");
    assert!((height - 28.).abs() < 0.5, "the closed strip is {height}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_busy_agent_and_a_found_update_show_on_the_strip(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let id = run_program(cx, &busy_program());
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        value_of(cx, window, "activity-summary").as_deref() == Some("1 agent working")
    });
    assert!(!shows(cx, window, "activity-update"));
    offer_update(cx, data.path());
    assert!(shows(cx, window, "activity-update"));
}

#[gpui_kit::test]
fn a_waiting_question_shows_and_its_click_opens_the_next_thread_needing_you(
    cx: &mut TestAppContext,
) {
    let (kagan, window, _data) = open(cx);
    let oldest = run_shell(cx);
    let newest = run_shell(cx);
    let current = run_shell(cx);
    for id in [&oldest, &newest, &current] {
        mark_read(cx, id);
    }
    click(cx, window, &format!("thread-{}", current.0));
    let asked = raise(cx, &oldest, "First?");
    assert_eq!(value_of(cx, window, "activity-needs").as_deref(), Some("1"));
    click(cx, window, "activity-needs");
    assert_eq!(shown(cx, &kagan), Some(oldest));
    assert!(
        !shows(cx, window, "activity-log"),
        "the needs-you click only navigates"
    );
    drop(asked);
}

#[gpui_kit::test]
fn a_click_or_the_keyboard_opens_the_log_and_escape_closes_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    click(cx, window, "activity-summary");
    assert!(shows(cx, window, "activity-log"));
    assert_eq!(selected_of(cx, window, "activity-summary"), Some(true));
    assert_eq!(focused_of(cx, window, "activity-strip"), Some(true));

    crate::tests::kagan::press_key(cx, window, "space");
    assert!(!shows(cx, window, "activity-log"));
    assert_eq!(selected_of(cx, window, "activity-summary"), Some(false));
    assert_eq!(
        focused_of(cx, window, "activity-strip"),
        Some(true),
        "the keyboard toggle kept the strip"
    );

    crate::tests::kagan::press_key(cx, window, "enter");
    assert!(shows(cx, window, "activity-log"));

    crate::tests::kagan::press_key(cx, window, "escape");
    assert!(!shows(cx, window, "activity-log"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_open_strip_keeps_its_log_across_a_thread_switch(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_program(cx, &busy_program());
    click(cx, window, &format!("thread-{}", first.0));
    click(cx, window, "activity-summary");
    wait_until(cx, |cx| shows(cx, window, "activity-row-0"));

    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", second.0));
    assert!(shows(cx, window, "activity-strip"));
    assert!(shows(cx, window, "activity-log"));
    assert!(shows(cx, window, "activity-row-0"));
}

#[gpui_kit::test]
fn a_question_does_not_take_the_keyboard_or_open_the_strip(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let asking = run_shell(cx);
    let shown_thread = run_shell(cx);
    for id in [&asking, &shown_thread] {
        mark_read(cx, id);
    }
    click(cx, window, &format!("thread-{}", shown_thread.0));
    assert!(
        cx.update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap()
    );

    let asked = raise(cx, &asking, "Second?");
    assert!(
        cx.update_window(window, |_, window, cx| {
            screen_read::terminal_focused(kagan.read(cx), window, cx)
        })
        .unwrap(),
        "the question moved the keyboard"
    );
    assert_eq!(focused_of(cx, window, "activity-strip"), Some(false));
    assert!(!shows(cx, window, "activity-log"));
    drop(asked);
}
