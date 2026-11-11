use crate::kagan::screen_read;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, InputEvent as _, TestAppContext};

use crate::kagan::{ArchiveThread, FocusSidebar, Kagan, TogglePinned};

use crate::tests::kagan::{
    click, focused_of, grid_text, label, open, present, press_key, run_shell, selected_of, shows,
    thread_record, value_of, wait_for_screen, wait_until,
};
use crate::tests::sidebar::layout::focus_row;

fn terminal_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_focused(kagan.read(cx), window, cx)
    })
    .unwrap()
}

fn program_receives(cx: &mut TestAppContext, window: AnyWindowHandle, kagan: &Entity<Kagan>) {
    cx.simulate_input(window, "qx");
    wait_for_screen(cx, kagan, |cx| grid_text(cx, kagan).contains("qx"));
}

#[gpui_kit::test]
fn rename_keeps_the_program_and_a_click_on_the_title_does_not_start_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let before = thread_record(cx, &id);
    let view = cx.update(|cx| screen_read::terminal_view(kagan.read(cx)));
    click(cx, window, "thread-title");
    assert!(!shows(cx, window, "thread-title-input"));

    click(cx, window, "thread-actions-trigger");
    click(cx, window, "thread-rename");
    assert_eq!(focused_of(cx, window, "thread-title-input"), Some(true));
    assert_eq!(
        value_of(cx, window, "thread-title-input"),
        Some(before.title.clone())
    );

    cx.simulate_input(window, "Notes");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| thread_record(cx, &id).title == "Notes");
    let after = thread_record(cx, &id);
    assert_eq!(after.provider, before.provider);
    assert_eq!(after.environment_id, before.environment_id);
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );
    assert!(shows(cx, window, "thread-title"));
    assert!(!shows(cx, window, "thread-title-input"));
    assert!(terminal_focused(cx, window, &kagan));
    program_receives(cx, window, &kagan);
}

#[gpui_kit::test]
fn escape_cancels_a_rename(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let title = thread_record(cx, &id).title;
    click(cx, window, "thread-actions-trigger");
    click(cx, window, "thread-rename");
    cx.simulate_input(window, "Notes");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "thread-title-input"));
    assert_eq!(thread_record(cx, &id).title, title);
    assert_eq!(label(cx, window, &format!("thread-{}", id.0)), title);
    assert!(terminal_focused(cx, window, &kagan));
    program_receives(cx, window, &kagan);
}

#[gpui_kit::test]
fn a_good_rename_clears_an_earlier_error(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "thread-actions-trigger");
    click(cx, window, "thread-rename");
    press_key(cx, window, "backspace");
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert!(label(cx, window, "note").contains("Could not rename the thread"));
    click(cx, window, "thread-actions-trigger");
    click(cx, window, "thread-rename");
    press_key(cx, window, "backspace");
    cx.simulate_input(window, "Good");
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert_eq!(thread_record(cx, &id).title, "Good");
    assert!(!shows(cx, window, "note"));
}

#[gpui_kit::test]
fn a_failed_rename_keeps_its_error_and_a_later_focus(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let title = thread_record(cx, &id).title;
    click(cx, window, "thread-actions-trigger");
    click(cx, window, "thread-rename");
    press_key(cx, window, "backspace");
    assert_eq!(
        value_of(cx, window, "thread-title-input"),
        Some(String::new())
    );

    cx.update_window(window, |_, window, cx| {
        let keystroke = kg_core::parse_key("enter").unwrap();
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
        let kagan = kagan.clone();
        window.defer(cx, move |window, cx| {
            kagan.update(cx, |kagan, cx| {
                kagan.focus_sidebar(&FocusSidebar, window, cx);
            });
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(thread_record(cx, &id).title, title);
    assert!(
        label(cx, window, "note").contains("Could not rename the thread"),
        "{}",
        label(cx, window, "note")
    );
    assert_eq!(
        focused_of(cx, window, &format!("thread-{}", id.0)),
        Some(true)
    );
    assert!(!terminal_focused(cx, window, &kagan));
}

#[gpui_kit::test]
fn the_project_name_selects_and_the_plus_starts_a_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert_eq!(
        label(cx, window, "project-disclose-prj_kagan"),
        "Hide threads in Kagan project"
    );
    assert_eq!(
        label(cx, window, "project-new-prj_kagan"),
        "New thread in Kagan project"
    );

    click(cx, window, "project-prj_kagan");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(kg_domain::ProjectId("prj_kagan".into()))
    );
    assert!(shows(cx, window, "start-prompt"));
    assert_eq!(selected_of(cx, window, "project-prj_kagan"), Some(true));
    assert_eq!(
        selected_of(cx, window, &format!("thread-{}", id.0)),
        Some(false)
    );

    focus_row(cx, window, "project-prj_kagan");
    press_key(cx, window, "left");
    cx.run_until_parked();
    assert!(!present(cx, window, &format!("thread-{}", id.0)));
    assert_eq!(
        label(cx, window, "project-disclose-prj_kagan"),
        "Show threads in Kagan project"
    );
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), None);

    focus_row(cx, window, "project-prj_kagan");
    press_key(cx, window, "right");
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("thread-{}", id.0)));

    click(cx, window, "project-new-prj_kagan");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(kg_domain::ProjectId("prj_kagan".into()))
    );
    assert!(shows(cx, window, "start-prompt"));
}

#[gpui_kit::test]
fn a_pinned_thread_shows_a_pin_and_keeps_its_mark(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let pin = format!("thread-pinned-{}", id.0);
    let mark = format!("thread-mark-{}", id.0);
    assert!(!present(cx, window, &pin));
    assert!(shows(cx, window, &mark));

    cx.dispatch_action(window, TogglePinned);
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
    assert!(shows(cx, window, &pin));
    assert_eq!(label(cx, window, &pin), "Pinned");
    assert!(shows(cx, window, &mark));

    cx.dispatch_action(window, TogglePinned);
    wait_until(cx, |cx| !thread_record(cx, &id).pinned);
    assert!(!present(cx, window, &pin));
    assert!(shows(cx, window, &mark));
}

#[gpui_kit::test]
fn archived_threads_leave_the_sidebar_and_can_be_searched(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let row = format!("thread-{}", id.0);
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    assert!(!present(cx, window, &row));

    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, &row));
    assert_eq!(
        label(cx, window, "archived-note"),
        "Restoring brings the thread back. The program stays stopped."
    );

    cx.simulate_input(window, "zzzz");
    wait_until(cx, |cx| {
        label(cx, window, "archived-empty") == "No archived threads match."
    });
    assert!(!present(cx, window, &row));

    cx.simulate_keystrokes(window, "backspace backspace backspace backspace");
    wait_until(cx, |cx| shows(cx, window, &row));
    click(cx, window, &format!("restore-{}", id.0));
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_none());
    assert!(shows(cx, window, &row));
}

#[gpui_kit::test]
fn clicking_an_archived_row_out_of_view_scrolls_to_it(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{id}"));
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    let mut thread = thread_record(cx, &id);
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    for index in 0..40 {
        thread.id = kg_domain::ThreadId(format!("thr_archived_{index:02}"));
        thread.title = format!("Archived {index}");
        thread.created_at = kg_domain::Timestamp(index);
        thread.updated_at = thread.created_at;
        store.put_thread(&thread).unwrap();
    }
    drop(store);
    click(cx, window, "archived");
    let row = "thread-thr_archived_00";
    wait_until(cx, |cx| present(cx, window, row));
    assert!(!shows(cx, window, row));
    let task = cx.update(|cx| {
        kg_core::Workspace::dispatch(
            kg_domain::Op::Ui(kg_domain::UiOp::Click {
                element_id: row.into(),
                within: None,
                window: 0,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx))
            == Some(kg_domain::ThreadId("thr_archived_00".into()))
    });
}

#[gpui_kit::test]
fn archived_is_hidden_when_there_is_nothing_to_restore(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(!shows(cx, window, "archived"));
}

#[gpui_kit::test]
fn sidebar_arrows_move_focus_and_leave_the_terminal_its_keys(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let older = run_shell(cx);
    let newer = run_shell(cx);
    cx.run_until_parked();

    let older_row = format!("thread-{}", older.0);
    let newer_row = format!("thread-{}", newer.0);
    cx.dispatch_action(window, FocusSidebar);
    cx.run_until_parked();
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));

    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    let first_newer = focused_of(cx, window, &newer_row) == Some(true);
    let first_older = focused_of(cx, window, &older_row) == Some(true);
    assert!(first_newer || first_older);
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), None);

    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    let second = if first_newer { &older_row } else { &newer_row };
    assert_eq!(focused_of(cx, window, second), Some(true));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), None);

    click(cx, window, &format!("thread-{}", newer.0));
    wait_until(cx, |cx| shows(cx, window, "terminal"));
    cx.simulate_keystrokes(window, "up");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(newer.clone())
    );
    assert_ne!(
        focused_of(cx, window, &format!("thread-{}", newer.0)),
        Some(true)
    );
}

#[gpui_kit::test]
fn needs_you_says_a_terminal_prompt_is_not_a_question(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    assert!(!shows(cx, window, "needs-you"));
}
