use super::chord;
use crate::kagan::screen_read;

use gpui_kit::TestAppContext;
use kg_core::Workspace;

use crate::kagan::{ArchiveThread, TogglePinned, ToggleSidebar};

use super::{
    click, focused_of, open, press_key, run_shell, selected_of, shows, thread_record, wait_until,
};

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

    crate::tests::sidebar::layout::focus_row(cx, window, &format!("thread-{}", id.0));
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
fn an_archived_thread_that_stays_open_says_it_is_archived_not_an_error(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "changes-not-git"));
    let archive = cx.update(|cx| {
        Workspace::dispatch(
            kg_domain::Op::Thread(kg_domain::ThreadOp::Archive { id: id.clone() }),
            cx,
        )
    });
    cx.foreground_executor().block_test(archive).unwrap();
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    wait_until(cx, |cx| shows(cx, window, "changes-archived"));
    assert!(!shows(cx, window, "changes-error"));
}
