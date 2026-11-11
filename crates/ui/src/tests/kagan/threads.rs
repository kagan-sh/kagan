use super::chord;
use crate::kagan::screen_read;

use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::{AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentSpec, InteractionOption, Op, OpResult, PreferenceOp, ProjectId, Run, ThreadId,
    ThreadOp, Visibility,
};

use crate::kagan::{ArchiveThread, TogglePinned};

use super::{
    click, focused_of, highlighted_at, label, open, open_with_plugins, point_at, press_key,
    quad_at_row, role_of, row_top, run_program, run_shell, set_preference, shell, shows,
    thread_record, value_of, wait_until,
};

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
        crate::tests::sidebar::layout::span(cx, window, "thread-heading").expect("heading");
    let (left_x, left_w) = crate::tests::sidebar::layout::span(cx, window, "left-sidebar-toggle")
        .expect("left toggle");
    let (right_x, _) = crate::tests::sidebar::layout::span(cx, window, "right-sidebar-toggle")
        .expect("right toggle");
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
        assert!((f32::from(grid.bounds.origin.y) - (bar + inset)).abs() < 1.);
        let title = crate::ui_registry::find(window, &ElementId::Name("thread-title".into()))
            .expect("title");
        assert!(f32::from(title.bounds.origin.y + title.bounds.size.height) <= bar);
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

pub(in crate::tests) fn run_owned(cx: &mut TestAppContext, owner: &ThreadId) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Visible),
        parent_thread_id: None,
        quiet: false,
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
                approval_thread_id: Some(id.clone()),
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
fn a_default_thread_title_is_the_program_name_not_its_arguments(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let program = format!("{} {}", shell(), "x".repeat(100));
    let id = run_program(cx, &program);
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    let name = std::path::Path::new(shell())
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    cx.update_window(window, |_, window, _| {
        let row = crate::ui_registry::find(window, &ElementId::Name(format!("thread-{id}").into()))
            .expect("thread row");
        assert!(row.label.is_some_and(|label| label.to_string() == name));
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
        role: None,
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
    click(cx, window, "settings-section-installed");
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
fn archive_from_a_row_menu_archives_that_row(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let row = run_shell(cx);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    crate::tests::sidebar::layout::hover_row(cx, window, &format!("thread-{}", row.0));
    click(cx, window, &format!("thread-actions-{}", row.0));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(other.clone())
    );
    click(cx, window, "thread-archive");
    wait_until(cx, |cx| thread_record(cx, &row).archived_at.is_some());
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(other.clone())
    );
    assert!(thread_record(cx, &other).archived_at.is_none());
}

#[gpui_kit::test]
fn rename_from_a_row_menu_renames_that_row(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let row = run_shell(cx);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    crate::tests::sidebar::layout::hover_row(cx, window, &format!("thread-{}", row.0));
    click(cx, window, &format!("thread-actions-{}", row.0));
    click(cx, window, "thread-rename");
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(row.clone())
    );
    cx.simulate_input(window, "Notes");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| thread_record(cx, &row).title == "Notes");
    assert_ne!(thread_record(cx, &other).title, "Notes");
}

#[gpui_kit::test]
fn save_diagnostics_from_a_row_menu_writes_the_report(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let row = run_shell(cx);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    crate::tests::sidebar::layout::hover_row(cx, window, &format!("thread-{}", row.0));
    click(cx, window, &format!("thread-actions-{}", row.0));
    click(cx, window, "save-diagnostics");
    let dir = data.path().join("diagnostics");
    wait_until(cx, |_cx| {
        std::fs::read_dir(&dir).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        })
    });
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(other));
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
fn a_muted_thread_shows_its_mark_beside_the_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(!shows(cx, window, "title-muted"));
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Notify {
                id: id.clone(),
                level: kg_domain::ThreadNotify::Muted,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| shows(cx, window, "title-muted"));
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Notify {
                id: id.clone(),
                level: kg_domain::ThreadNotify::Default,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| !shows(cx, window, "title-muted"));
}
